use bridgevm_hvf::{
    dtb::VirtFdtConfig,
    fwcfg::GuestMemoryMut,
    machine,
    msix::MsixMessage,
    pcie,
    platform_virt::{FlatGuestRam, MmioOp, MmioOutcome, VirtPlatform},
};

pub const IMAN: u64 = 0x1020;
pub const TABLE: u64 = pcie::XHCI_MSIX_TABLE_OFFSET as u64;
pub const PBA: u64 = pcie::XHCI_MSIX_PBA_OFFSET as u64;
const BAR: u64 = machine::PCIE_MMIO_32.base + 0x2_0000;
const COMMAND: u64 = machine::RAM_BASE + 0x1000;
const ERST: u64 = machine::RAM_BASE + 0x2000;
const EVENT: u64 = machine::RAM_BASE + 0x3000;

pub fn message() -> MsixMessage {
    MsixMessage {
        vector: 0,
        address: machine::GIC_ITS.base + 0x40,
        data: 0x83,
    }
}

pub struct Fixture {
    platform: VirtPlatform,
    mem: FlatGuestRam,
}

impl Fixture {
    pub fn new(interrupter_enabled: bool, vector_masked: bool, msix_enabled: bool) -> Self {
        let mut x = Self {
            platform: VirtPlatform::new(VirtFdtConfig::default()),
            mem: FlatGuestRam::new(machine::RAM_BASE, 0x1_0000),
        };
        x.config(pcie::REG_BAR0, 4, BAR);
        x.config(
            pcie::REG_COMMAND_STATUS,
            2,
            u64::from(pcie::CMD_MEMORY_SPACE | pcie::CMD_BUS_MASTER),
        );
        x.write(TABLE, 8, message().address);
        x.write(TABLE + 8, 4, u64::from(message().data));
        x.write(TABLE + 12, 4, u64::from(vector_masked));
        x.configure_msix(msix_enabled, false);
        assert!(x.mem.write_bytes(ERST, &EVENT.to_le_bytes()));
        assert!(x.mem.write_bytes(ERST + 8, &16u32.to_le_bytes()));
        for (offset, size, value) in [
            (0x58, 8, COMMAND | 1),
            (0x70, 8, machine::RAM_BASE + 0x4000),
            (0x78, 4, 4),
            (IMAN, 4, if interrupter_enabled { 2 } else { 0 }),
            (0x1024, 4, 0), // No moderation delay in this notification test.
            (0x1028, 4, 1),
            (0x1030, 8, ERST),
            (0x1038, 8, EVENT),
            (0x40, 4, 5), // Start only after primary ring initialization; INTE set.
        ] {
            x.write(offset, size, value);
        }
        assert!(x.messages().is_empty());
        x
    }

    fn config(&mut self, register: u16, size: u8, value: u64) {
        let bdf = pcie::XHCI_BDF;
        let address = machine::PCIE_ECAM.base
            + (u64::from(bdf.1) << 15)
            + (u64::from(bdf.2) << 12)
            + u64::from(register);
        assert_eq!(
            self.platform
                .on_mmio(address, MmioOp::Write { size, value }, &mut self.mem),
            MmioOutcome::WriteAck
        );
    }

    pub fn configure_msix(&mut self, enabled: bool, masked: bool) {
        self.config(
            u16::from(pcie::XHCI_MSIX_CAP_OFFSET) + 2,
            2,
            (u64::from(enabled) << 15) | (u64::from(masked) << 14),
        );
    }

    pub fn write(&mut self, offset: u64, size: u8, value: u64) {
        assert_eq!(
            self.platform
                .on_mmio(BAR + offset, MmioOp::Write { size, value }, &mut self.mem),
            MmioOutcome::WriteAck
        );
    }

    pub fn read(&mut self, offset: u64) -> u64 {
        match self
            .platform
            .on_mmio(BAR + offset, MmioOp::Read { size: 4 }, &mut self.mem)
        {
            MmioOutcome::ReadValue(value) => value,
            other => panic!("unexpected read result: {other:?}"),
        }
    }

    pub fn messages(&mut self) -> Vec<MsixMessage> {
        self.platform.take_pending_msix()
    }

    pub fn complete(&mut self, index: u64) {
        // Ordinary Enable Slot command; completion must already be in guest RAM.
        let trb = COMMAND + index * 16;
        assert!(self
            .mem
            .write_bytes(trb + 12, &((9u32 << 10) | 1).to_le_bytes()));
        self.write(0x2000, 4, 0);
        let event = self.mem.read_bytes(EVENT + index * 16, 16).unwrap();
        assert_eq!(u64::from_le_bytes(event[0..8].try_into().unwrap()), trb);
        assert_eq!(
            u32::from_le_bytes(event[8..12].try_into().unwrap()) >> 24,
            1
        );
        assert_eq!(
            (u32::from_le_bytes(event[12..16].try_into().unwrap()) >> 10) & 0x3f,
            33
        );
    }
}
