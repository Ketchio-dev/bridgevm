use crate::dtb::VirtFdtConfig;
use crate::fwcfg::GuestMemoryMut;
use crate::platform_virt::{FlatGuestRam, MmioOp, MmioOutcome, VirtPlatform};
use crate::{machine, pcie};

pub(super) const XHCI_BAR0: u64 = machine::PCIE_MMIO_32.base + 0x2_0000;
pub(super) const RAM_LEN: usize = 0x10_000;
pub(super) const COMMAND_RING: u64 = machine::RAM_BASE + 0x1000;
pub(super) const ERST: u64 = machine::RAM_BASE + 0x2000;
pub(super) const EVENT_RING: u64 = machine::RAM_BASE + 0x3000;
pub(super) const DCBAA: u64 = machine::RAM_BASE + 0x4000;
pub(super) const TRB_TYPE_ENABLE_SLOT: u32 = 9;
pub(super) const TRB_TYPE_ADDRESS_DEVICE: u32 = 11;
const TRB_TYPE_COMMAND_COMPLETION_EVENT: u32 = 33;
const COMPLETION_CODE_SUCCESS: u32 = 1;
pub(super) const ENABLE_SLOT_ID: u32 = 1;
pub(super) const ADDRESS_DEVICE_SLOT_ID: u32 = 6;

#[derive(Clone, Copy)]
pub(super) struct BarWrite {
    pub(super) offset: u64,
    pub(super) size: u8,
    pub(super) value: u64,
}

#[derive(Clone, Copy)]
pub(super) struct MsixVector {
    pub(super) address: u64,
    pub(super) data: u32,
}

pub(super) fn new_platform_and_ram() -> (VirtPlatform, FlatGuestRam) {
    (
        VirtPlatform::new(VirtFdtConfig::default()),
        FlatGuestRam::new(machine::RAM_BASE, RAM_LEN),
    )
}

pub(super) fn program_xhci_bar0(platform: &mut VirtPlatform, mem: &mut FlatGuestRam) {
    assert_eq!(
        platform.on_mmio(
            pcie_cfg_gpa(pcie::XHCI_BDF.1, pcie::XHCI_BDF.2, pcie::REG_BAR0),
            MmioOp::Write {
                size: 4,
                value: XHCI_BAR0,
            },
            mem,
        ),
        MmioOutcome::WriteAck
    );
    assert_eq!(
        platform.on_mmio(
            pcie_cfg_gpa(pcie::XHCI_BDF.1, pcie::XHCI_BDF.2, pcie::REG_COMMAND_STATUS),
            MmioOp::Write {
                size: 2,
                value: u64::from(pcie::CMD_MEMORY_SPACE | pcie::CMD_BUS_MASTER),
            },
            mem,
        ),
        MmioOutcome::WriteAck
    );
}

pub(super) fn write_xhci_bar0(
    platform: &mut VirtPlatform,
    mem: &mut FlatGuestRam,
    write: BarWrite,
) {
    assert_eq!(
        platform.on_mmio(
            XHCI_BAR0 + write.offset,
            MmioOp::Write {
                size: write.size,
                value: write.value,
            },
            mem,
        ),
        MmioOutcome::WriteAck
    );
}

pub(super) fn enable_xhci_msix_vector0(
    platform: &mut VirtPlatform,
    mem: &mut FlatGuestRam,
    vector: MsixVector,
) {
    let table = u64::from(pcie::XHCI_MSIX_TABLE_OFFSET);
    write_xhci_bar0(
        platform,
        mem,
        BarWrite {
            offset: table,
            size: 8,
            value: vector.address,
        },
    );
    write_xhci_bar0(
        platform,
        mem,
        BarWrite {
            offset: table + 8,
            size: 4,
            value: u64::from(vector.data),
        },
    );
    write_xhci_bar0(
        platform,
        mem,
        BarWrite {
            offset: table + 12,
            size: 4,
            value: 0,
        },
    );
    assert_eq!(
        platform.on_mmio(
            pcie_cfg_gpa(
                pcie::XHCI_BDF.1,
                pcie::XHCI_BDF.2,
                u16::from(pcie::XHCI_MSIX_CAP_OFFSET) + 2,
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

pub(super) fn write_command_trb(mem: &mut FlatGuestRam, command_control: u32) {
    let command = [0u32, 0, 0, command_control];
    for (index, dword) in command.iter().enumerate() {
        assert!(mem.write_bytes(
            COMMAND_RING + u64::try_from(index).unwrap() * 4,
            &dword.to_le_bytes()
        ));
    }
}

pub(super) fn write_event_ring_table(mem: &mut FlatGuestRam) {
    assert!(mem.write_bytes(ERST, &EVENT_RING.to_le_bytes()));
    assert!(mem.write_bytes(ERST + 8, &16u32.to_le_bytes()));
}

pub(super) fn command_control(trb_type: u32, slot_id: u32) -> u32 {
    (slot_id << 24) | (trb_type << 10) | 1
}

pub(super) fn assert_success_completion(mem: &FlatGuestRam, expected_slot_id: u32) {
    assert_eq!(read_u32(mem, EVENT_RING), low_u32(COMMAND_RING));
    assert_eq!(read_u32(mem, EVENT_RING + 4), high_u32(COMMAND_RING));
    assert_eq!(read_u32(mem, EVENT_RING + 8) >> 24, COMPLETION_CODE_SUCCESS);
    let control = read_u32(mem, EVENT_RING + 12);
    assert_eq!((control >> 10) & 0x3f, TRB_TYPE_COMMAND_COMPLETION_EVENT);
    assert_eq!(control & 1, 1);
    assert_eq!(control >> 24, expected_slot_id);
}

fn pcie_cfg_gpa(device: u8, function: u8, reg: u16) -> u64 {
    machine::PCIE_ECAM.base
        + (u64::from(device) << 15)
        + (u64::from(function) << 12)
        + u64::from(reg)
}

fn read_u32(mem: &FlatGuestRam, gpa: u64) -> u32 {
    u32::from_le_bytes(mem.read_bytes(gpa, 4).unwrap().try_into().unwrap())
}

fn low_u32(value: u64) -> u32 {
    u32::try_from(value & 0xffff_ffff).unwrap()
}

fn high_u32(value: u64) -> u32 {
    u32::try_from(value >> 32).unwrap()
}
