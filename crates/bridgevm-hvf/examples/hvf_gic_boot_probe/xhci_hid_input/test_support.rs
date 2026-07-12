use bridgevm_hvf::dtb::VirtFdtConfig;
use bridgevm_hvf::fwcfg::GuestMemoryMut;
use bridgevm_hvf::machine;
use bridgevm_hvf::pcie;
use bridgevm_hvf::platform_virt::{FlatGuestRam, MmioOp, MmioOutcome, VirtPlatform};

pub(crate) static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub(crate) const RAM_LEN: usize = 0x10_000;
pub(crate) const COMMAND_RING: u64 = machine::RAM_BASE + 0x1000;
pub(crate) const ERST: u64 = machine::RAM_BASE + 0x2000;
pub(crate) const EVENT_RING: u64 = machine::RAM_BASE + 0x3000;
pub(crate) const DCBAA: u64 = machine::RAM_BASE + 0x4000;
pub(crate) const INPUT_CONTEXT: u64 = machine::RAM_BASE + 0x5000;
pub(crate) const DCI3_RING: u64 = machine::RAM_BASE + 0x8000;
pub(crate) const DCI5_RING: u64 = machine::RAM_BASE + 0x8400;
pub(crate) const DCI3_KEY_BUFFER: u64 = machine::RAM_BASE + 0x8800;
pub(crate) const DCI3_RELEASE_BUFFER: u64 = machine::RAM_BASE + 0x8820;
pub(crate) const DCI5_POINTER_BUFFER: u64 = machine::RAM_BASE + 0x8860;
pub(crate) const OUTPUT_CONTEXT: u64 = machine::RAM_BASE + 0x9000;
pub(crate) const TRB_SIZE: u64 = 16;

const XHCI_BAR0: u64 = machine::PCIE_MMIO_32.base + 0x2_0000;
const ENABLE_SLOT_ID: u32 = 1;
const DCI3: u32 = 3;
const DCI5: u32 = 5;
const TRB_TYPE_CONFIGURE_ENDPOINT: u32 = 12;
const TRB_TYPE_COMMAND_COMPLETION_EVENT: u32 = 33;
const TRB_TYPE_PORT_STATUS_CHANGE_EVENT: u32 = 34;
const TRB_TYPE_TRANSFER_EVENT: u32 = 32;
const TRB_TYPE_NORMAL: u32 = 1;
const COMPLETION_CODE_SUCCESS: u32 = 1;
const PORT_STATUS_CHANGE_EVENT_PARAMETER: u64 = 1 << 24;
const DCI3_INPUT_CONTEXT_OFFSET: u64 = 0x80;
const DCI5_INPUT_CONTEXT_OFFSET: u64 = 0xc0;
const INPUT_CONTROL_ADD_CONTEXT_OFFSET: u64 = 0x04;
const EP_CONTEXT_DWORD1_OFFSET: u64 = 0x04;
const EP_TR_DEQUEUE_OFFSET: u64 = 0x08;
const EP_CONTEXT_DWORD4_OFFSET: u64 = 0x10;
const DCI3_ADD_CONTEXT_FLAG: u32 = 1 << DCI3;
const DCI5_ADD_CONTEXT_FLAG: u32 = 1 << DCI5;
const DCI3_DWORD1: u32 = (3 << 1) | (3 << 3) | (8 << 16);
const DCI3_DWORD4: u32 = 8;
const TRB_CYCLE: u64 = 1;
const USB_CMD_HCRST: u32 = 1 << 1;

pub(crate) fn new_platform() -> VirtPlatform {
    VirtPlatform::new(VirtFdtConfig::default())
}

pub(crate) fn new_platform_and_ram() -> (VirtPlatform, FlatGuestRam) {
    (
        new_platform(),
        FlatGuestRam::new(machine::RAM_BASE, RAM_LEN),
    )
}

pub(crate) fn emit_uart(platform: &mut VirtPlatform, bytes: &[u8]) {
    let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0);
    for byte in bytes {
        assert_eq!(
            platform.on_mmio(
                machine::UART.base,
                MmioOp::Write {
                    size: 1,
                    value: u64::from(*byte),
                },
                &mut mem,
            ),
            MmioOutcome::WriteAck
        );
    }
}

pub(crate) fn program_xhci_bar0(platform: &mut VirtPlatform, mem: &mut FlatGuestRam) {
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
            pcie_cfg_gpa(pcie::XHCI_BDF.1, pcie::XHCI_BDF.2, pcie::REG_COMMAND_STATUS,),
            MmioOp::Write {
                size: 2,
                value: u64::from(pcie::CMD_MEMORY_SPACE | pcie::CMD_BUS_MASTER),
            },
            mem,
        ),
        MmioOutcome::WriteAck
    );
}

pub(crate) fn configure_dci3_interrupt_in_over_bar0(
    platform: &mut VirtPlatform,
    mem: &mut FlatGuestRam,
) -> u64 {
    write_event_ring_table(mem);
    write_u64(mem, DCBAA + (u64::from(ENABLE_SLOT_ID) * 8), OUTPUT_CONTEXT);
    write_u32(
        mem,
        INPUT_CONTEXT + INPUT_CONTROL_ADD_CONTEXT_OFFSET,
        DCI3_ADD_CONTEXT_FLAG,
    );
    write_u32(
        mem,
        INPUT_CONTEXT + DCI3_INPUT_CONTEXT_OFFSET + EP_CONTEXT_DWORD1_OFFSET,
        DCI3_DWORD1,
    );
    write_u64(
        mem,
        INPUT_CONTEXT + DCI3_INPUT_CONTEXT_OFFSET + EP_TR_DEQUEUE_OFFSET,
        DCI3_RING | TRB_CYCLE,
    );
    write_u32(
        mem,
        INPUT_CONTEXT + DCI3_INPUT_CONTEXT_OFFSET + EP_CONTEXT_DWORD4_OFFSET,
        DCI3_DWORD4,
    );
    write_command_trb_with_parameter(
        mem,
        INPUT_CONTEXT,
        command_control(TRB_TYPE_CONFIGURE_ENDPOINT, ENABLE_SLOT_ID),
    );
    for (offset, size, value) in [
        (0x58, 8, COMMAND_RING | TRB_CYCLE),
        (0x70, 8, DCBAA),
        (0x78, 4, 1),
        (0x1020, 4, 2),
        (0x1028, 4, 1),
        (0x1030, 8, ERST),
        (0x1038, 8, EVENT_RING),
        (0x2000, 4, 0),
    ] {
        write_xhci_bar0(platform, mem, offset, size, value);
    }
    let first_event_control = read_u32(mem, EVENT_RING + 12);
    let command_completion_event_index =
        if ((first_event_control >> 10) & 0x3f) == TRB_TYPE_PORT_STATUS_CHANGE_EVENT {
            assert_eq!(
                read_u64(mem, EVENT_RING),
                PORT_STATUS_CHANGE_EVENT_PARAMETER
            );
            assert_eq!(read_u32(mem, EVENT_RING + 8), 0);
            assert_eq!(first_event_control & 1, 1);
            1
        } else {
            0
        };
    assert_success_completion(mem, command_completion_event_index, ENABLE_SLOT_ID);
    command_completion_event_index + 1
}

pub(crate) fn configure_dci3_and_dci5_interrupt_in_over_bar0(
    platform: &mut VirtPlatform,
    mem: &mut FlatGuestRam,
) -> u64 {
    write_event_ring_table(mem);
    write_u64(mem, DCBAA + (u64::from(ENABLE_SLOT_ID) * 8), OUTPUT_CONTEXT);
    write_u32(
        mem,
        INPUT_CONTEXT + INPUT_CONTROL_ADD_CONTEXT_OFFSET,
        DCI3_ADD_CONTEXT_FLAG | DCI5_ADD_CONTEXT_FLAG,
    );
    write_endpoint_context(mem, DCI3_INPUT_CONTEXT_OFFSET, DCI3_RING);
    write_endpoint_context(mem, DCI5_INPUT_CONTEXT_OFFSET, DCI5_RING);
    write_command_trb_with_parameter(
        mem,
        INPUT_CONTEXT,
        command_control(TRB_TYPE_CONFIGURE_ENDPOINT, ENABLE_SLOT_ID),
    );
    for (offset, size, value) in [
        (0x58, 8, COMMAND_RING | TRB_CYCLE),
        (0x70, 8, DCBAA),
        (0x78, 4, 1),
        (0x1020, 4, 2),
        (0x1028, 4, 1),
        (0x1030, 8, ERST),
        (0x1038, 8, EVENT_RING),
        (0x2000, 4, 0),
    ] {
        write_xhci_bar0(platform, mem, offset, size, value);
    }
    let first_event_control = read_u32(mem, EVENT_RING + 12);
    let command_completion_event_index =
        if ((first_event_control >> 10) & 0x3f) == TRB_TYPE_PORT_STATUS_CHANGE_EVENT {
            assert_eq!(
                read_u64(mem, EVENT_RING),
                PORT_STATUS_CHANGE_EVENT_PARAMETER
            );
            assert_eq!(read_u32(mem, EVENT_RING + 8), 0);
            assert_eq!(first_event_control & 1, 1);
            1
        } else {
            0
        };
    assert_success_completion(mem, command_completion_event_index, ENABLE_SLOT_ID);
    command_completion_event_index + 1
}

pub(crate) fn reset_xhci_host_controller_over_bar0(
    platform: &mut VirtPlatform,
    mem: &mut FlatGuestRam,
) {
    write_xhci_bar0(platform, mem, 0x40, 4, u64::from(USB_CMD_HCRST));
}

pub(crate) fn write_dci3_normal_trb(mem: &mut FlatGuestRam, trb_gpa: u64, buffer_gpa: u64) {
    write_u64(mem, trb_gpa, buffer_gpa);
    write_u32(mem, trb_gpa + 8, 8);
    write_u32(mem, trb_gpa + 12, (TRB_TYPE_NORMAL << 10) | 1);
}

pub(crate) fn write_dci5_normal_trb(mem: &mut FlatGuestRam, trb_gpa: u64, buffer_gpa: u64) {
    write_dci3_normal_trb(mem, trb_gpa, buffer_gpa);
}

pub(crate) fn ring_dci3_doorbell(platform: &mut VirtPlatform, mem: &mut FlatGuestRam) {
    write_xhci_bar0(platform, mem, 0x2004, 4, u64::from(DCI3));
}

pub(crate) fn acknowledge_event_ring_dequeue(
    platform: &mut VirtPlatform,
    mem: &mut FlatGuestRam,
    event_index: u64,
) {
    write_xhci_bar0(
        platform,
        mem,
        0x1038,
        4,
        (EVENT_RING + (TRB_SIZE * event_index)) | 0x8,
    );
}

pub(crate) fn assert_success_dci3_transfer_event_for_trb(
    mem: &FlatGuestRam,
    event_gpa: u64,
    trb_gpa: u64,
) {
    assert_eq!(read_u64(mem, event_gpa), trb_gpa);
    assert_eq!(read_u32(mem, event_gpa + 8) >> 24, COMPLETION_CODE_SUCCESS);
    let control = read_u32(mem, event_gpa + 12);
    assert_eq!((control >> 10) & 0x3f, TRB_TYPE_TRANSFER_EVENT);
    assert_eq!((control >> 16) & 0x1f, DCI3);
    assert_eq!(control >> 24, ENABLE_SLOT_ID);
    assert_eq!(control & 1, 1);
}

pub(crate) fn assert_success_dci5_transfer_event_for_trb(
    mem: &FlatGuestRam,
    event_gpa: u64,
    trb_gpa: u64,
) {
    assert_eq!(read_u64(mem, event_gpa), trb_gpa);
    assert_eq!(read_u32(mem, event_gpa + 8) & 0x00ff_ffff, 2);
    assert_eq!(read_u32(mem, event_gpa + 8) >> 24, 13);
    let control = read_u32(mem, event_gpa + 12);
    assert_eq!((control >> 10) & 0x3f, TRB_TYPE_TRANSFER_EVENT);
    assert_eq!((control >> 16) & 0x1f, DCI5);
    assert_eq!(control >> 24, ENABLE_SLOT_ID);
    assert_eq!(control & 1, 1);
}

pub(crate) fn read_bytes(mem: &FlatGuestRam, gpa: u64, len: usize) -> Vec<u8> {
    mem.read_bytes(gpa, len).unwrap()
}

fn write_xhci_bar0(
    platform: &mut VirtPlatform,
    mem: &mut FlatGuestRam,
    offset: u64,
    size: u8,
    value: u64,
) {
    assert_eq!(
        platform.on_mmio(XHCI_BAR0 + offset, MmioOp::Write { size, value }, mem),
        MmioOutcome::WriteAck
    );
}

fn write_command_trb_with_parameter(mem: &mut FlatGuestRam, parameter: u64, command_control: u32) {
    assert!(mem.write_bytes(COMMAND_RING, &parameter.to_le_bytes()));
    assert!(mem.write_bytes(COMMAND_RING + 8, &0u32.to_le_bytes()));
    assert!(mem.write_bytes(COMMAND_RING + 12, &command_control.to_le_bytes()));
}

fn write_event_ring_table(mem: &mut FlatGuestRam) {
    assert!(mem.write_bytes(ERST, &EVENT_RING.to_le_bytes()));
    assert!(mem.write_bytes(ERST + 8, &16u32.to_le_bytes()));
}

fn write_endpoint_context(mem: &mut FlatGuestRam, offset: u64, ring: u64) {
    write_u32(
        mem,
        INPUT_CONTEXT + offset + EP_CONTEXT_DWORD1_OFFSET,
        DCI3_DWORD1,
    );
    write_u64(
        mem,
        INPUT_CONTEXT + offset + EP_TR_DEQUEUE_OFFSET,
        ring | TRB_CYCLE,
    );
    write_u32(
        mem,
        INPUT_CONTEXT + offset + EP_CONTEXT_DWORD4_OFFSET,
        DCI3_DWORD4,
    );
}

fn assert_success_completion(mem: &FlatGuestRam, event_index: u64, expected_slot_id: u32) {
    let event_gpa = EVENT_RING + (TRB_SIZE * event_index);
    assert_eq!(read_u32(mem, event_gpa), low_u32(COMMAND_RING));
    assert_eq!(read_u32(mem, event_gpa + 4), high_u32(COMMAND_RING));
    assert_eq!(read_u32(mem, event_gpa + 8) >> 24, COMPLETION_CODE_SUCCESS);
    let control = read_u32(mem, event_gpa + 12);
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

fn read_u64(mem: &FlatGuestRam, gpa: u64) -> u64 {
    u64::from_le_bytes(mem.read_bytes(gpa, 8).unwrap().try_into().unwrap())
}

fn write_u32(mem: &mut FlatGuestRam, gpa: u64, value: u32) {
    assert!(mem.write_bytes(gpa, &value.to_le_bytes()));
}

fn write_u64(mem: &mut FlatGuestRam, gpa: u64, value: u64) {
    assert!(mem.write_bytes(gpa, &value.to_le_bytes()));
}

fn command_control(trb_type: u32, slot_id: u32) -> u32 {
    (slot_id << 24) | (trb_type << 10) | 1
}

fn low_u32(value: u64) -> u32 {
    u32::try_from(value & 0xffff_ffff).unwrap()
}

fn high_u32(value: u64) -> u32 {
    u32::try_from(value >> 32).unwrap()
}
