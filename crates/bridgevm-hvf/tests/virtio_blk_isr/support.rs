//! Synthetic read-only sector and public PCI/MMIO fixture; no guest media.
use bridgevm_hvf::{
    dtb::VirtFdtConfig,
    fwcfg::GuestMemoryMut,
    machine, pcie,
    platform_virt::{
        FlatGuestRam, MmioOp, MmioOutcome, VirtPlatform, VirtPlatformConfig,
        VirtPlatformDeviceConfig,
    },
};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

pub const DESC: u64 = machine::RAM_BASE + 0x1000;
pub const AVAIL: u64 = DESC + 128 * 16;
pub const USED: u64 = machine::RAM_BASE + 0x2000;
const HEADER: u64 = machine::RAM_BASE + 0x4000;
const DATA: u64 = machine::RAM_BASE + 0x5000;
const STATUS: u64 = machine::RAM_BASE + 0x6000;
pub const IRQ: u32 = machine::spi_to_intid(machine::SPI_PCIE_INTA);
pub const BAR: u64 = machine::PCIE_MMIO_32.base + 0x8000;
pub const MSIX: u64 = machine::PCIE_MMIO_32.base + 0x18000;
pub const PIO: u64 = machine::PCIE_PIO.base + 0x1000;

#[derive(Clone, Copy, Debug)]
pub enum Transport {
    Pio,
    Bar,
}

pub struct Media(PathBuf);
impl Media {
    pub fn new() -> Self {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "bridgevm-blk-isr-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&dir).unwrap();
        fs::write(dir.join("sector.raw"), [0x5a; 512]).unwrap();
        Self(dir)
    }
    pub fn path(&self) -> PathBuf {
        self.0.join("sector.raw")
    }
}
impl Drop for Media {
    fn drop(&mut self) {
        assert_eq!(fs::read(self.path()).unwrap(), [0x5a; 512]);
        fs::remove_file(self.path()).unwrap();
        fs::remove_dir(&self.0).unwrap();
    }
}

pub fn memory() -> FlatGuestRam {
    let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0x8000);
    for (index, address, length, flags, next) in [
        (0, HEADER, 16u32, 1u16, 1u16),
        (1, DATA, 512, 3, 2),
        (2, STATUS, 1, 2, 0),
    ] {
        let off = DESC + index * 16;
        assert!(mem.write_bytes(off, &address.to_le_bytes()));
        assert!(mem.write_bytes(off + 8, &length.to_le_bytes()));
        assert!(mem.write_bytes(off + 12, &flags.to_le_bytes()));
        assert!(mem.write_bytes(off + 14, &next.to_le_bytes()));
    }
    mem
}

pub struct Fixture {
    pub platform: VirtPlatform,
    pub mem: FlatGuestRam,
    pub transport: Transport,
    pub _media: Media,
}
impl Fixture {
    pub fn new(transport: Transport) -> Self {
        let media = Media::new();
        let mut platform = VirtPlatform::new_with_config(VirtPlatformConfig {
            fdt: VirtFdtConfig::default(),
            devices: VirtPlatformDeviceConfig {
                virtio_boot_media_present: true,
                ..Default::default()
            },
        });
        platform.attach_pci_boot_media(media.path()).unwrap();
        let mut f = Self {
            platform,
            mem: memory(),
            transport,
            _media: media,
        };
        let bdf = pcie::VIRTIO_BLK_BDF;
        let config = machine::PCIE_ECAM.base + (u64::from(bdf.1) << 15) + (u64::from(bdf.2) << 12);
        for (reg, value) in [
            (pcie::REG_BAR0, 0x1001),
            (pcie::REG_BAR0 + 4, MSIX),
            (pcie::REG_BAR0 + 16, BAR),
            (pcie::REG_COMMAND_STATUS, 7),
        ] {
            f.write(config + u64::from(reg), 4, value);
        }
        match transport {
            Transport::Pio => {
                for (off, size, value) in [
                    (0x12, 1, 0),
                    (0x12, 1, 3),
                    (0x0e, 2, 0),
                    (0x04, 4, 0x60),
                    (0x08, 4, DESC >> 12),
                    (0x12, 1, 7),
                ] {
                    f.write(PIO + off, size, value);
                }
            }
            Transport::Bar => {
                for (off, value) in [
                    (0x70, 0),
                    (0x70, 3),
                    (0x24, 1),
                    (0x20, 1),
                    (0x70, 11),
                    (0x30, 0),
                    (0x38, 128),
                    (0x80, DESC),
                    (0x90, AVAIL),
                    (0xa0, USED),
                    (0x44, 1),
                    (0x70, 15),
                ] {
                    f.write(BAR + off, 4, value);
                }
            }
        }
        assert!(f.platform.take_pending_spi_levels().is_empty());
        f
    }
    pub fn write(&mut self, address: u64, size: u8, value: u64) {
        assert_eq!(
            self.platform
                .on_mmio(address, MmioOp::Write { size, value }, &mut self.mem),
            MmioOutcome::WriteAck
        );
    }
    pub fn read(&mut self, address: u64, size: u8) -> u64 {
        match self
            .platform
            .on_mmio(address, MmioOp::Read { size }, &mut self.mem)
        {
            MmioOutcome::ReadValue(value) => value,
            other => panic!("unexpected read {address:#x}: {other:?}"),
        }
    }
    pub fn isr(&self) -> u64 {
        match self.transport {
            Transport::Pio => PIO + 0x13,
            Transport::Bar => BAR + 0x1000,
        }
    }
    pub fn submit(&mut self, slot: u16) {
        assert!(self.mem.write_bytes(STATUS, &[0xff]));
        assert!(self.mem.write_bytes(AVAIL + 2, &(slot + 1).to_le_bytes()));
        match self.transport {
            Transport::Pio => self.write(PIO + 0x10, 2, 0),
            Transport::Bar => self.write(BAR + 0x3000, 4, 0),
        }
        assert_eq!(self.mem.read_bytes(DATA, 512).unwrap(), [0x5a; 512]);
        assert_eq!(self.mem.read_bytes(STATUS, 1).unwrap(), [0]);
        assert_eq!(
            self.mem.read_bytes(USED + 2, 2).unwrap(),
            (slot + 1).to_le_bytes()
        );
        assert_eq!(self.platform.take_pending_spi_levels(), [(IRQ, true)]);
    }
}
