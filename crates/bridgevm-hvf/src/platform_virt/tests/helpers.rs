//! Split test module.

use super::super::*;
use crate::dtb::VirtFdtConfig;
use crate::fwcfg::GuestMemoryMut;
use crate::fwcfg::DMA_CTL_SELECT;
use crate::fwcfg::DMA_CTL_WRITE;
use crate::fwcfg::KEY_CMDLINE_DATA;
use crate::fwcfg::KEY_CMDLINE_SIZE;
use crate::fwcfg::KEY_FILE_DIR;
use crate::fwcfg::KEY_INITRD_DATA;
use crate::fwcfg::KEY_INITRD_SIZE;
use crate::fwcfg::KEY_KERNEL_DATA;
use crate::fwcfg::KEY_KERNEL_SIZE;
use crate::machine;
use crate::ramfb::DRM_FORMAT_XRGB8888;
use crate::ramfb::RAMFB_CONFIG_SIZE;
use crate::virtio_blk::INSTALLER_ISO_SLOT;
use std::path::PathBuf;
use std::time::SystemTime;

pub(super) const REG_DATA: u64 = 0x0;
pub(super) const REG_SELECTOR: u64 = 0x8;
pub(super) const REG_DMA: u64 = 0x10;
pub(super) const NET_COMMON_QUEUE_SELECT: u64 = 0x16;
pub(super) const NET_COMMON_QUEUE_SIZE: u64 = 0x18;
pub(super) const NET_COMMON_QUEUE_MSIX_VECTOR: u64 = 0x1a;
pub(super) const NET_COMMON_QUEUE_ENABLE: u64 = 0x1c;
pub(super) const NET_COMMON_QUEUE_DESC: u64 = 0x20;
pub(super) const NET_COMMON_QUEUE_DRIVER: u64 = 0x28;
pub(super) const NET_COMMON_QUEUE_DEVICE: u64 = 0x30;
pub(super) const NET_NOTIFY_CFG_OFFSET: u64 = 0x3000;
pub(super) const NET_TX_QUEUE: u16 = 1;
pub(super) const NET_VIRTIO_HDR_LEN: usize = 12;
pub(super) const NET_DESC_F_NEXT: u16 = 1;

pub(super) fn platform() -> VirtPlatform {
    VirtPlatform::new(VirtFdtConfig::default())
}

pub(super) fn platform_with_ramfb() -> VirtPlatform {
    VirtPlatform::new_with_ramfb(VirtFdtConfig::default())
}

pub(super) fn platform_with_devices(devices: VirtPlatformDeviceConfig) -> VirtPlatform {
    VirtPlatform::new_with_config(VirtPlatformConfig {
        fdt: VirtFdtConfig::default(),
        devices,
    })
}

#[derive(Debug)]
pub(super) struct TestTpmBackend;

impl crate::tpm_tis::Tpm2Backend for TestTpmBackend {
    fn execute(
        &mut self,
        _locality: u8,
        _command: &[u8],
    ) -> Result<Vec<u8>, crate::tpm_tis::TpmBackendError> {
        Ok(vec![0x80, 0x01, 0, 0, 0, 10, 0, 0, 0, 0])
    }
}

pub(super) fn pcie_cfg_gpa(device: u8, function: u8, reg: u16) -> u64 {
    machine::PCIE_ECAM.base
        + (u64::from(device) << 15)
        + (u64::from(function) << 12)
        + u64::from(reg)
}

pub(super) fn temp_path(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "bridgevm-hvf-platform-virt-{name}-{}-{nanos}",
        std::process::id()
    ))
}

pub(super) fn write_vring_desc(
    mem: &mut FlatGuestRam,
    table: u64,
    index: u16,
    addr: u64,
    len: u32,
    flags: u16,
    next: u16,
) {
    let gpa = table + u64::from(index) * 16;
    assert!(mem.write_bytes(gpa, &addr.to_le_bytes()));
    assert!(mem.write_bytes(gpa + 8, &len.to_le_bytes()));
    assert!(mem.write_bytes(gpa + 12, &flags.to_le_bytes()));
    assert!(mem.write_bytes(gpa + 14, &next.to_le_bytes()));
}

pub(super) fn write_valid_ramfb_config(p: &mut VirtPlatform, mem: &mut FlatGuestRam) {
    let (selector, size) = fw_cfg_file_entry(p, b"etc/ramfb");
    let src = machine::RAM_BASE + 0x100;
    let ctrl = machine::RAM_BASE + 0x200;
    let mut config = [0u8; RAMFB_CONFIG_SIZE];
    config[0..8].copy_from_slice(&0x4010_0000u64.to_be_bytes());
    config[8..12].copy_from_slice(&DRM_FORMAT_XRGB8888.to_be_bytes());
    config[12..16].copy_from_slice(&0u32.to_be_bytes());
    config[16..20].copy_from_slice(&1024u32.to_be_bytes());
    config[20..24].copy_from_slice(&768u32.to_be_bytes());
    config[24..28].copy_from_slice(&(1024u32 * 4).to_be_bytes());
    let control = (u32::from(selector) << 16) | DMA_CTL_SELECT | DMA_CTL_WRITE;
    let mut dma = Vec::new();
    dma.extend_from_slice(&control.to_be_bytes());
    dma.extend_from_slice(&(size as u32).to_be_bytes());
    dma.extend_from_slice(&src.to_be_bytes());
    assert!(mem.write_bytes(src, &config));
    assert!(mem.write_bytes(ctrl, &dma));

    assert_eq!(
        p.on_mmio(
            machine::FW_CFG.base + REG_DMA,
            MmioOp::Write {
                size: 8,
                value: ctrl.swap_bytes(),
            },
            mem,
        ),
        MmioOutcome::WriteAck
    );
}

pub(super) fn read_virtio_iso_sector(
    p: &mut VirtPlatform,
    mem: &mut FlatGuestRam,
    sector: u64,
    expected_prefix_len: usize,
) -> Vec<u8> {
    const REG_GUEST_PAGE_SIZE: u64 = 0x28;
    const REG_QUEUE_NUM: u64 = 0x38;
    const REG_QUEUE_ALIGN: u64 = 0x3c;
    const REG_QUEUE_PFN: u64 = 0x40;
    const REG_QUEUE_NOTIFY: u64 = 0x50;
    const DESC_F_NEXT: u16 = 1;
    const DESC_F_WRITE: u16 = 2;
    const VIRTIO_BLK_T_IN: u32 = 0;

    let slot_base = machine::virtio_mmio_slot(INSTALLER_ISO_SLOT).base;
    let desc = machine::RAM_BASE + 0x9000;
    let avail = desc + 8 * 16;
    let header = machine::RAM_BASE + 0xb000;
    let data = machine::RAM_BASE + 0xc000;
    let status = machine::RAM_BASE + 0xd000;
    assert!(mem.write_bytes(header, &VIRTIO_BLK_T_IN.to_le_bytes()));
    assert!(mem.write_bytes(header + 8, &sector.to_le_bytes()));
    write_vring_desc(mem, desc, 0, header, 16, DESC_F_NEXT, 1);
    write_vring_desc(mem, desc, 1, data, 512, DESC_F_NEXT | DESC_F_WRITE, 2);
    write_vring_desc(mem, desc, 2, status, 1, DESC_F_WRITE, 0);
    assert!(mem.write_bytes(avail + 2, &1u16.to_le_bytes()));
    assert!(mem.write_bytes(avail + 4, &0u16.to_le_bytes()));

    for (reg, value) in [
        (REG_QUEUE_NUM, 8),
        (REG_GUEST_PAGE_SIZE, 4096),
        (REG_QUEUE_ALIGN, 4096),
        (REG_QUEUE_PFN, desc >> 12),
    ] {
        assert_eq!(
            p.on_mmio(slot_base + reg, MmioOp::Write { size: 4, value }, mem),
            MmioOutcome::WriteAck
        );
    }
    assert_eq!(
        p.on_mmio(
            slot_base + REG_QUEUE_NOTIFY,
            MmioOp::Write { size: 4, value: 0 },
            mem,
        ),
        MmioOutcome::WriteAck
    );

    mem.read_bytes(data, expected_prefix_len).unwrap()
}

pub(super) fn read_pci_boot_media_sector(
    p: &mut VirtPlatform,
    mem: &mut FlatGuestRam,
    bar: u64,
    sector: u64,
    expected_prefix_len: usize,
) -> Vec<u8> {
    const PCI_NOTIFY_CFG_OFFSET: u64 = 0x3000;
    const REG_QUEUE_NUM: u64 = 0x038;
    const REG_QUEUE_READY: u64 = 0x044;
    const REG_QUEUE_NOTIFY: u64 = 0x050;
    const REG_QUEUE_DESC_LOW: u64 = 0x080;
    const REG_QUEUE_DRIVER_LOW: u64 = 0x090;
    const REG_QUEUE_DEVICE_LOW: u64 = 0x0a0;
    const DESC_F_NEXT: u16 = 1;
    const DESC_F_WRITE: u16 = 2;
    const VIRTIO_BLK_T_IN: u32 = 0;

    let desc = machine::RAM_BASE + 0x10000;
    let avail = machine::RAM_BASE + 0x11000;
    let used = machine::RAM_BASE + 0x12000;
    let header = machine::RAM_BASE + 0x13000;
    let data = machine::RAM_BASE + 0x14000;
    let status = machine::RAM_BASE + 0x15000;
    assert!(mem.write_bytes(header, &VIRTIO_BLK_T_IN.to_le_bytes()));
    assert!(mem.write_bytes(header + 8, &sector.to_le_bytes()));
    write_vring_desc(mem, desc, 0, header, 16, DESC_F_NEXT, 1);
    write_vring_desc(mem, desc, 1, data, 512, DESC_F_NEXT | DESC_F_WRITE, 2);
    write_vring_desc(mem, desc, 2, status, 1, DESC_F_WRITE, 0);
    assert!(mem.write_bytes(avail + 2, &1u16.to_le_bytes()));
    assert!(mem.write_bytes(avail + 4, &0u16.to_le_bytes()));

    for (reg, value) in [
        (REG_QUEUE_NUM, 8),
        (REG_QUEUE_DESC_LOW, desc),
        (REG_QUEUE_DRIVER_LOW, avail),
        (REG_QUEUE_DEVICE_LOW, used),
        (REG_QUEUE_READY, 1),
    ] {
        assert_eq!(
            p.on_mmio(bar + reg, MmioOp::Write { size: 4, value }, mem),
            MmioOutcome::WriteAck
        );
    }
    assert_eq!(
        p.on_mmio(
            bar + PCI_NOTIFY_CFG_OFFSET + REG_QUEUE_NOTIFY,
            MmioOp::Write { size: 4, value: 0 },
            mem,
        ),
        MmioOutcome::WriteAck
    );

    mem.read_bytes(data, expected_prefix_len).unwrap()
}

pub(super) fn program_nvme_bar0(p: &mut VirtPlatform, mem: &mut FlatGuestRam) {
    p.on_mmio(
        pcie_cfg_gpa(1, 0, crate::pcie::REG_BAR0),
        MmioOp::Write {
            size: 4,
            value: machine::PCIE_MMIO_32.base,
        },
        mem,
    );
    p.on_mmio(
        pcie_cfg_gpa(1, 0, crate::pcie::REG_COMMAND_STATUS),
        MmioOp::Write {
            size: 2,
            value: u64::from(crate::pcie::CMD_MEMORY_SPACE | crate::pcie::CMD_BUS_MASTER),
        },
        mem,
    );
}

pub(super) fn program_virtio_blk_bar4(p: &mut VirtPlatform, mem: &mut FlatGuestRam, base: u64) {
    p.on_mmio(
        pcie_cfg_gpa(3, 0, crate::pcie::REG_BAR0 + 4 * 4),
        MmioOp::Write {
            size: 4,
            value: base,
        },
        mem,
    );
    p.on_mmio(
        pcie_cfg_gpa(3, 0, crate::pcie::REG_COMMAND_STATUS),
        MmioOp::Write {
            size: 2,
            value: u64::from(crate::pcie::CMD_MEMORY_SPACE | crate::pcie::CMD_BUS_MASTER),
        },
        mem,
    );
}

pub(super) fn program_virtio_blk_bar1(p: &mut VirtPlatform, mem: &mut FlatGuestRam, base: u64) {
    p.on_mmio(
        pcie_cfg_gpa(3, 0, crate::pcie::REG_BAR0 + 4),
        MmioOp::Write {
            size: 4,
            value: base,
        },
        mem,
    );
    p.on_mmio(
        pcie_cfg_gpa(3, 0, crate::pcie::REG_COMMAND_STATUS),
        MmioOp::Write {
            size: 2,
            value: u64::from(crate::pcie::CMD_MEMORY_SPACE | crate::pcie::CMD_BUS_MASTER),
        },
        mem,
    );
}

pub(super) fn program_virtio_blk_bar0_pio(p: &mut VirtPlatform, mem: &mut FlatGuestRam, port: u64) {
    p.on_mmio(
        pcie_cfg_gpa(3, 0, crate::pcie::REG_BAR0),
        MmioOp::Write {
            size: 4,
            value: port,
        },
        mem,
    );
    p.on_mmio(
        pcie_cfg_gpa(3, 0, crate::pcie::REG_COMMAND_STATUS),
        MmioOp::Write {
            size: 2,
            value: u64::from(crate::pcie::CMD_IO_SPACE | crate::pcie::CMD_BUS_MASTER),
        },
        mem,
    );
}

pub(super) fn program_virtio_net_bar4(p: &mut VirtPlatform, mem: &mut FlatGuestRam, base: u64) {
    p.on_mmio(
        pcie_cfg_gpa(
            crate::pcie::VIRTIO_NET_BDF.1,
            crate::pcie::VIRTIO_NET_BDF.2,
            crate::pcie::REG_BAR0 + 4 * 4,
        ),
        MmioOp::Write {
            size: 4,
            value: base,
        },
        mem,
    );
    p.on_mmio(
        pcie_cfg_gpa(
            crate::pcie::VIRTIO_NET_BDF.1,
            crate::pcie::VIRTIO_NET_BDF.2,
            crate::pcie::REG_COMMAND_STATUS,
        ),
        MmioOp::Write {
            size: 2,
            value: u64::from(crate::pcie::CMD_MEMORY_SPACE | crate::pcie::CMD_BUS_MASTER),
        },
        mem,
    );
}

pub(super) fn program_virtio_net_bar1(p: &mut VirtPlatform, mem: &mut FlatGuestRam, base: u64) {
    p.on_mmio(
        pcie_cfg_gpa(
            crate::pcie::VIRTIO_NET_BDF.1,
            crate::pcie::VIRTIO_NET_BDF.2,
            crate::pcie::REG_BAR0 + 4,
        ),
        MmioOp::Write {
            size: 4,
            value: base,
        },
        mem,
    );
    p.on_mmio(
        pcie_cfg_gpa(
            crate::pcie::VIRTIO_NET_BDF.1,
            crate::pcie::VIRTIO_NET_BDF.2,
            crate::pcie::REG_COMMAND_STATUS,
        ),
        MmioOp::Write {
            size: 2,
            value: u64::from(crate::pcie::CMD_MEMORY_SPACE | crate::pcie::CMD_BUS_MASTER),
        },
        mem,
    );
}

pub(super) fn virtio_net_write(
    p: &mut VirtPlatform,
    mem: &mut FlatGuestRam,
    bar: u64,
    offset: u64,
    size: u8,
    value: u64,
) {
    assert_eq!(
        p.on_mmio(bar + offset, MmioOp::Write { size, value }, mem),
        MmioOutcome::WriteAck
    );
}

pub(super) struct TestVirtQueue {
    pub(super) desc: u64,
    pub(super) avail: u64,
    pub(super) used: u64,
}

pub(super) fn setup_virtio_net_queue(
    p: &mut VirtPlatform,
    mem: &mut FlatGuestRam,
    bar: u64,
    queue: u16,
    layout: TestVirtQueue,
    vector: u16,
) {
    let TestVirtQueue { desc, avail, used } = layout;
    for (offset, size, value) in [
        (NET_COMMON_QUEUE_SELECT, 2, u64::from(queue)),
        (NET_COMMON_QUEUE_SIZE, 2, 8),
        (NET_COMMON_QUEUE_MSIX_VECTOR, 2, u64::from(vector)),
        (NET_COMMON_QUEUE_DESC, 8, desc),
        (NET_COMMON_QUEUE_DRIVER, 8, avail),
        (NET_COMMON_QUEUE_DEVICE, 8, used),
        (NET_COMMON_QUEUE_ENABLE, 2, 1),
    ] {
        virtio_net_write(p, mem, bar, offset, size, value);
    }
}

pub(super) fn enable_virtio_net_msix_vector(
    p: &mut VirtPlatform,
    mem: &mut FlatGuestRam,
    bar1: u64,
    vector: u16,
    address: u64,
    data: u32,
) {
    let entry = bar1 + u64::from(vector) * 16;
    assert_eq!(
        p.on_mmio(
            entry,
            MmioOp::Write {
                size: 8,
                value: address,
            },
            mem,
        ),
        MmioOutcome::WriteAck
    );
    assert_eq!(
        p.on_mmio(
            entry + 8,
            MmioOp::Write {
                size: 4,
                value: u64::from(data),
            },
            mem,
        ),
        MmioOutcome::WriteAck
    );
    assert_eq!(
        p.on_mmio(entry + 12, MmioOp::Write { size: 4, value: 0 }, mem),
        MmioOutcome::WriteAck
    );
    assert_eq!(
        p.on_mmio(
            pcie_cfg_gpa(
                crate::pcie::VIRTIO_NET_BDF.1,
                crate::pcie::VIRTIO_NET_BDF.2,
                u16::from(crate::pcie::VIRTIO_NET_MSIX_CAP_OFFSET) + 2,
            ),
            MmioOp::Write {
                size: 2,
                value: 0x8000,
            },
            mem,
        ),
        MmioOutcome::WriteAck
    );
}

pub(super) fn encode_nvme_sqe(
    opcode: u8,
    command_id: u16,
    nsid: u32,
    prp1: u64,
    cdw10: u32,
    cdw11: u32,
    cdw12: u32,
) -> [u8; 64] {
    let mut e = [0u8; 64];
    let cdw0 = u32::from(opcode) | (u32::from(command_id) << 16);
    e[0..4].copy_from_slice(&cdw0.to_le_bytes());
    e[4..8].copy_from_slice(&nsid.to_le_bytes());
    e[24..32].copy_from_slice(&prp1.to_le_bytes());
    e[40..44].copy_from_slice(&cdw10.to_le_bytes());
    e[44..48].copy_from_slice(&cdw11.to_le_bytes());
    e[48..52].copy_from_slice(&cdw12.to_le_bytes());
    e
}

pub(super) fn find_fw_cfg_file_entry(p: &mut VirtPlatform, name: &[u8]) -> Option<(u16, usize)> {
    p.fw_cfg.select(KEY_FILE_DIR);
    let dir = p.fw_cfg.read_data(p.fw_cfg.file_dir_bytes().len());
    let count = u32::from_be_bytes([dir[0], dir[1], dir[2], dir[3]]) as usize;
    (0..count).find_map(|index| {
        let record = &dir[4 + index * 64..4 + (index + 1) * 64];
        let name_end = record[8..64]
            .iter()
            .position(|&byte| byte == 0)
            .unwrap_or(56);
        (&record[8..8 + name_end] == name).then(|| {
            let size = u32::from_be_bytes([record[0], record[1], record[2], record[3]]) as usize;
            let select = u16::from_be_bytes([record[4], record[5]]);
            (select, size)
        })
    })
}

pub(super) fn fw_cfg_file_entry(p: &mut VirtPlatform, name: &[u8]) -> (u16, usize) {
    find_fw_cfg_file_entry(p, name).unwrap_or_else(|| {
        panic!(
            "default fw_cfg dir missing {}",
            String::from_utf8_lossy(name)
        )
    })
}

pub(super) fn nvme_mmio_write(
    p: &mut VirtPlatform,
    mem: &mut FlatGuestRam,
    offset: u64,
    size: u8,
    value: u64,
) {
    assert_eq!(
        p.on_mmio(
            machine::PCIE_MMIO_32.base + offset,
            MmioOp::Write { size, value },
            mem,
        ),
        MmioOutcome::WriteAck
    );
}

pub(super) fn enable_nvme_controller(
    p: &mut VirtPlatform,
    mem: &mut FlatGuestRam,
    asq: u64,
    acq: u64,
) {
    let qdepth = 4u64;
    nvme_mmio_write(
        p,
        mem,
        crate::nvme::REG_AQA,
        4,
        ((qdepth - 1) << 16) | (qdepth - 1),
    );
    nvme_mmio_write(p, mem, crate::nvme::REG_ASQ, 8, asq);
    nvme_mmio_write(p, mem, crate::nvme::REG_ACQ, 8, acq);
    assert_eq!(
        p.on_mmio(
            machine::PCIE_MMIO_32.base + crate::nvme::REG_CC,
            MmioOp::Write { size: 4, value: 1 },
            mem,
        ),
        MmioOutcome::WriteAck
    );
}

pub(super) fn enable_nvme_msix_vector0(
    p: &mut VirtPlatform,
    mem: &mut FlatGuestRam,
    address: u64,
    data: u32,
) {
    let table = u64::from(crate::pcie::NVME_MSIX_TABLE_OFFSET);
    nvme_mmio_write(p, mem, table, 8, address);
    nvme_mmio_write(p, mem, table + 8, 4, u64::from(data));
    nvme_mmio_write(p, mem, table + 12, 4, 0);
    assert_eq!(
        p.on_mmio(
            pcie_cfg_gpa(1, 0, u16::from(crate::pcie::NVME_MSIX_CAP_OFFSET) + 2),
            MmioOp::Write {
                size: 2,
                value: 0x8000,
            },
            mem,
        ),
        MmioOutcome::WriteAck
    );
}

pub(super) fn submit_admin_sqe(
    p: &mut VirtPlatform,
    mem: &mut FlatGuestRam,
    asq: u64,
    slot: u16,
    sqe: &[u8; 64],
) {
    assert!(mem.write_bytes(asq + u64::from(slot) * crate::nvme::SQ_ENTRY_SIZE, sqe));
    assert_eq!(
        p.on_mmio(
            machine::PCIE_MMIO_32.base + crate::nvme::REG_DOORBELL_BASE,
            MmioOp::Write {
                size: 4,
                value: u64::from(slot + 1),
            },
            mem,
        ),
        MmioOutcome::WriteAck
    );
}

#[test]
fn pcie_nvme_endpoint_routes_bar0_to_controller_registers() {
    let mut p = platform();
    let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0);

    assert_eq!(
        p.on_mmio(
            pcie_cfg_gpa(1, 0, crate::pcie::REG_VENDOR_DEVICE),
            MmioOp::Read { size: 4 },
            &mut mem
        ),
        MmioOutcome::ReadValue(0x0010_1b36)
    );
    assert!(matches!(
        p.on_mmio(
            pcie_cfg_gpa(1, 0, crate::pcie::REG_REVISION_CLASS),
            MmioOp::Read { size: 4 },
            &mut mem
        ),
        MmioOutcome::ReadValue(v) if v >> 8 == u64::from(crate::pcie::NVME_CLASS_CODE)
    ));

    p.on_mmio(
        pcie_cfg_gpa(1, 0, crate::pcie::REG_BAR0),
        MmioOp::Write {
            size: 4,
            value: 0xFFFF_FFFF,
        },
        &mut mem,
    );
    let MmioOutcome::ReadValue(mask) = p.on_mmio(
        pcie_cfg_gpa(1, 0, crate::pcie::REG_BAR0),
        MmioOp::Read { size: 4 },
        &mut mem,
    ) else {
        panic!("BAR0 sizing read did not return a value");
    };
    let size = (!((mask as u32) & !0xF)).wrapping_add(1);
    assert_eq!(size, crate::pcie::NVME_BAR0_SIZE);

    assert_eq!(
        p.on_mmio(
            machine::PCIE_MMIO_32.base + crate::nvme::REG_VS,
            MmioOp::Read { size: 4 },
            &mut mem
        ),
        MmioOutcome::KnownUnimplemented("pcie-mmio-32")
    );
    program_nvme_bar0(&mut p, &mut mem);
    assert_eq!(
        p.on_mmio(
            machine::PCIE_MMIO_32.base + crate::nvme::REG_VS,
            MmioOp::Read { size: 4 },
            &mut mem
        ),
        MmioOutcome::ReadValue(u64::from(crate::nvme::NVME_VERSION_1_4_0))
    );
    assert_eq!(
        p.on_mmio(
            machine::PCIE_MMIO_32.base + crate::nvme::REG_CC,
            MmioOp::Write { size: 4, value: 1 },
            &mut mem
        ),
        MmioOutcome::WriteAck
    );
    assert_eq!(
        p.on_mmio(
            machine::PCIE_MMIO_32.base + crate::nvme::REG_CSTS,
            MmioOp::Read { size: 4 },
            &mut mem
        ),
        MmioOutcome::ReadValue(1)
    );
}

#[test]
fn linux_boot_blobs_register_qemu_numeric_fw_cfg_items() {
    let mut p = platform();
    p.set_linux_boot_blobs(
        b"kernel-image".to_vec(),
        Some(b"initrd-image".to_vec()),
        b"console=ttyAMA0\0".to_vec(),
    );

    p.fw_cfg.select(KEY_KERNEL_SIZE);
    assert_eq!(
        p.fw_cfg.mmio_read(0, 4),
        12,
        "QemuKernelLoaderFsDxe reads KERNEL_SIZE with QemuFwCfgRead32"
    );
    p.fw_cfg.select(KEY_KERNEL_DATA);
    assert_eq!(p.fw_cfg.read_data(12), b"kernel-image");

    p.fw_cfg.select(KEY_INITRD_SIZE);
    assert_eq!(p.fw_cfg.mmio_read(0, 4), 12);
    p.fw_cfg.select(KEY_INITRD_DATA);
    assert_eq!(p.fw_cfg.read_data(12), b"initrd-image");

    p.fw_cfg.select(KEY_CMDLINE_SIZE);
    assert_eq!(p.fw_cfg.mmio_read(0, 4), 16);
    p.fw_cfg.select(KEY_CMDLINE_DATA);
    assert_eq!(p.fw_cfg.read_data(16), b"console=ttyAMA0\0");

    p.fw_cfg.select(crate::fwcfg::KEY_FILE_DIR);
    let dir = p.fw_cfg.read_data(p.fw_cfg.file_dir_bytes().len());
    let blob = String::from_utf8_lossy(&dir);
    assert!(!blob.contains("kernel-image"));
    assert!(!blob.contains("initrd-image"));
}
