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
pub(super) const INPUT_CONTEXT: u64 = machine::RAM_BASE + 0x5000;
pub(super) const EP0_RING: u64 = machine::RAM_BASE + 0x6000;
pub(super) const DATA_STAGE_BUFFER: u64 = machine::RAM_BASE + 0x7000;
pub(super) const DCI3_RING: u64 = machine::RAM_BASE + 0x8000;
pub(super) const DCI3_KEY_BUFFER: u64 = machine::RAM_BASE + 0x8800;
pub(super) const DCI3_RELEASE_BUFFER: u64 = machine::RAM_BASE + 0x8820;
pub(super) const OUTPUT_CONTEXT: u64 = machine::RAM_BASE + 0x9000;
pub(super) const TRB_TYPE_ENABLE_SLOT: u32 = 9;
pub(super) const TRB_TYPE_ADDRESS_DEVICE: u32 = 11;
pub(super) const TRB_TYPE_CONFIGURE_ENDPOINT: u32 = 12;
pub(super) const TRB_TYPE_NORMAL: u32 = 1;
const TRB_TYPE_COMMAND_COMPLETION_EVENT: u32 = 33;
pub(super) const TRB_TYPE_TRANSFER_EVENT: u32 = 32;
pub(super) const COMPLETION_CODE_SUCCESS: u32 = 1;
const TRB_DATA_STAGE_DIRECTION_IN: u32 = 1 << 16;
pub(super) const TRB_SIZE: u64 = 16;
pub(super) const ENABLE_SLOT_ID: u32 = 1;
pub(super) const ADDRESS_DEVICE_SLOT_ID: u32 = 6;
pub(super) const DCI3: u32 = 3;
pub(super) const DCI3_INPUT_CONTEXT_OFFSET: u64 = 0x80;
pub(super) const INPUT_CONTROL_ADD_CONTEXT_OFFSET: u64 = 0x04;
pub(super) const EP_CONTEXT_DWORD1_OFFSET: u64 = 0x04;
pub(super) const EP_TR_DEQUEUE_OFFSET: u64 = 0x08;
pub(super) const EP_CONTEXT_DWORD4_OFFSET: u64 = 0x10;
pub(super) const DCI3_ADD_CONTEXT_FLAG: u32 = 1 << DCI3;
pub(super) const DCI3_DWORD1: u32 = (3 << 1) | (3 << 3) | (8 << 16);
pub(super) const DCI3_DWORD4: u32 = 8;
pub(super) const TRB_CYCLE: u64 = 1;
pub(super) const DEVICE_DESCRIPTOR: [u8; 18] = [
    18, 1, 0x00, 0x02, 0, 0, 0, 64, 0x09, 0x12, 0x01, 0x00, 0x00, 0x01, 0, 0, 0, 1,
];

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
    write_command_trb_with_parameter(mem, 0, command_control);
}

pub(super) fn write_command_trb_with_parameter(
    mem: &mut FlatGuestRam,
    parameter: u64,
    command_control: u32,
) {
    assert!(mem.write_bytes(COMMAND_RING, &parameter.to_le_bytes()));
    assert!(mem.write_bytes(COMMAND_RING + 8, &0u32.to_le_bytes()));
    assert!(mem.write_bytes(COMMAND_RING + 12, &command_control.to_le_bytes()));
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

pub(super) fn write_ep0_input_context(mem: &mut FlatGuestRam, ep0_dequeue: u64) {
    assert!(mem.write_bytes(INPUT_CONTEXT + 0x40 + 8, &ep0_dequeue.to_le_bytes()));
}

pub(super) fn write_get_descriptor_device_transfer(mem: &mut FlatGuestRam) {
    write_u64(mem, EP0_RING, 0x0012_0000_0100_0680);
    write_u32(mem, EP0_RING + 8, 8);
    write_u32(mem, EP0_RING + 12, transfer_control(2));

    write_u64(mem, EP0_RING + 0x10, DATA_STAGE_BUFFER);
    write_u32(
        mem,
        EP0_RING + 0x18,
        u32::try_from(DEVICE_DESCRIPTOR.len()).unwrap(),
    );
    write_u32(
        mem,
        EP0_RING + 0x1c,
        transfer_control(3) | TRB_DATA_STAGE_DIRECTION_IN,
    );

    write_u32(mem, EP0_RING + 0x2c, transfer_control(7));
    write_u32(mem, EP0_RING + 0x3c, transfer_control(4));
}

pub(super) fn write_set_configuration_transfer(mem: &mut FlatGuestRam) {
    write_u64(mem, EP0_RING, 0x0000_0000_0001_0900);
    write_u32(mem, EP0_RING + 8, 8);
    write_u32(mem, EP0_RING + 12, transfer_control(2));

    write_u32(mem, EP0_RING + 0x1c, transfer_control(4));
}

pub(super) fn assert_success_transfer_event_for_trb(
    mem: &FlatGuestRam,
    event_gpa: u64,
    trb_gpa: u64,
) {
    assert_eq!(read_u64(mem, event_gpa), trb_gpa);
    assert_eq!(read_u32(mem, event_gpa + 8) >> 24, COMPLETION_CODE_SUCCESS);
    let control = read_u32(mem, event_gpa + 12);
    assert_eq!((control >> 10) & 0x3f, TRB_TYPE_TRANSFER_EVENT);
    assert_eq!((control >> 16) & 0x1f, 1);
    assert_eq!(control >> 24, ENABLE_SLOT_ID);
    assert_eq!(control & 1, 1);
}

fn pcie_cfg_gpa(device: u8, function: u8, reg: u16) -> u64 {
    machine::PCIE_ECAM.base
        + (u64::from(device) << 15)
        + (u64::from(function) << 12)
        + u64::from(reg)
}

pub(super) fn read_u32(mem: &FlatGuestRam, gpa: u64) -> u32 {
    u32::from_le_bytes(mem.read_bytes(gpa, 4).unwrap().try_into().unwrap())
}

pub(super) fn read_bytes(mem: &FlatGuestRam, gpa: u64, len: usize) -> Vec<u8> {
    mem.read_bytes(gpa, len).unwrap()
}

pub(super) fn read_u64(mem: &FlatGuestRam, gpa: u64) -> u64 {
    u64::from_le_bytes(mem.read_bytes(gpa, 8).unwrap().try_into().unwrap())
}

fn write_u32(mem: &mut FlatGuestRam, gpa: u64, value: u32) {
    assert!(mem.write_bytes(gpa, &value.to_le_bytes()));
}

fn write_u64(mem: &mut FlatGuestRam, gpa: u64, value: u64) {
    assert!(mem.write_bytes(gpa, &value.to_le_bytes()));
}

fn transfer_control(trb_type: u32) -> u32 {
    (trb_type << 10) | 1
}

fn low_u32(value: u64) -> u32 {
    u32::try_from(value & 0xffff_ffff).unwrap()
}

fn high_u32(value: u64) -> u32 {
    u32::try_from(value >> 32).unwrap()
}
