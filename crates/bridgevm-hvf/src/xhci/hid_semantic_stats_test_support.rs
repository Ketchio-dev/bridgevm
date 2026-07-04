use super::test_support::{
    command_control, setup_command_rings_with_parameter, setup_packet_parameter, SetupPacketFields,
    TestRam, DOORBELL_BASE, ENABLE_SLOT_ID, TRB_SIZE, TRB_TYPE_ADDRESS_DEVICE,
};
use super::XhciController;
use crate::fwcfg::GuestMemoryMut;

pub(super) const DATA_STAGE_BUFFER: u64 = 0x7000;
pub(super) const EP0_RING: u64 = 0x6000;
pub(super) const HID_CLASS_DESCRIPTOR_LENGTH: u16 = 9;
pub(super) const HID_POINTER_REPORT_DESCRIPTOR_LENGTH: u16 = 51;
pub(super) const HID_REPORT_DESCRIPTOR_LENGTH: u16 = 63;

const INPUT_CONTEXT: u64 = 0x5000;
const TRB_CYCLE: u32 = 1;
const TRB_TYPE_SETUP_STAGE: u32 = 2;
const TRB_TYPE_DATA_STAGE: u32 = 3;
const TRB_TYPE_STATUS_STAGE: u32 = 4;
const TRB_DATA_STAGE_DIRECTION_IN: u32 = 1 << 16;

#[derive(Clone, Copy)]
pub(super) enum ControlTransferShape {
    NoData,
    DataIn { buffer: u64, length: u16 },
    DataOut { buffer: u64, payload: u8 },
}

pub(super) fn prepare_addressed_control_transfer(
    xhci: &mut XhciController,
    mem: &mut TestRam,
    packet: SetupPacketFields,
    shape: ControlTransferShape,
) {
    mem.write_u64(INPUT_CONTEXT + 0x40 + 8, EP0_RING | 1);
    setup_command_rings_with_parameter(
        xhci,
        mem,
        INPUT_CONTEXT,
        command_control(TRB_TYPE_ADDRESS_DEVICE, ENABLE_SLOT_ID),
    );
    write_control_transfer_at(mem, EP0_RING, packet, shape);
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, mem));
}

pub(super) fn write_control_transfer_at(
    mem: &mut TestRam,
    ring: u64,
    packet: SetupPacketFields,
    shape: ControlTransferShape,
) {
    mem.write_u64(ring, setup_packet_parameter(packet));
    mem.write_u32(ring + 8, 8);
    mem.write_u32(ring + 12, transfer_control(TRB_TYPE_SETUP_STAGE));

    match shape {
        ControlTransferShape::NoData => {
            mem.write_u32(
                ring + TRB_SIZE + 12,
                transfer_control(TRB_TYPE_STATUS_STAGE),
            );
        }
        ControlTransferShape::DataIn { buffer, length } => {
            mem.write_u64(ring + TRB_SIZE, buffer);
            mem.write_u32(ring + TRB_SIZE + 8, u32::from(length));
            mem.write_u32(
                ring + TRB_SIZE + 12,
                transfer_control(TRB_TYPE_DATA_STAGE) | TRB_DATA_STAGE_DIRECTION_IN,
            );
            mem.write_u32(
                ring + (TRB_SIZE * 2) + 12,
                transfer_control(TRB_TYPE_STATUS_STAGE),
            );
        }
        ControlTransferShape::DataOut { buffer, payload } => {
            assert!(mem.write_bytes(buffer, &[payload]));
            mem.write_u64(ring + TRB_SIZE, buffer);
            mem.write_u32(ring + TRB_SIZE + 8, 1);
            mem.write_u32(ring + TRB_SIZE + 12, transfer_control(TRB_TYPE_DATA_STAGE));
            mem.write_u32(
                ring + (TRB_SIZE * 2) + 12,
                transfer_control(TRB_TYPE_STATUS_STAGE),
            );
        }
    }
}

fn transfer_control(trb_type: u32) -> u32 {
    (trb_type << 10) | TRB_CYCLE
}
