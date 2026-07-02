use crate::fwcfg::GuestMemoryMut;

use super::XhciController;

pub(super) const DOORBELL_BASE: u64 = 0x2000;
pub(super) const TRB_TYPE_ENABLE_SLOT: u32 = 9;
pub(super) const TRB_TYPE_ADDRESS_DEVICE: u32 = 11;
pub(super) const TRB_TYPE_TRANSFER_EVENT: u32 = 32;
const TRB_TYPE_COMMAND_COMPLETION_EVENT: u32 = 33;
pub(super) const COMPLETION_CODE_SUCCESS: u32 = 1;
pub(super) const ENABLE_SLOT_ID: u32 = 1;
pub(super) const TRB_SIZE: u64 = 16;
pub(super) const CMD_RING: u64 = 0x1000;
const TRB_EVENT_DATA: u32 = 1 << 2;
const ERST: u64 = 0x2000;
const ERST1: u64 = 0x2040;
pub(super) const EVENT_RING: u64 = 0x3000;
pub(super) const EVENT_RING1: u64 = 0x3800;
const DCBAA: u64 = 0x4000;

#[derive(Clone, Copy)]
pub(super) struct SetupPacketFields {
    pub(super) bm_request_type: u8,
    pub(super) request: u8,
    pub(super) value: u16,
    pub(super) index: u16,
    pub(super) length: u16,
}

#[derive(Debug)]
pub(super) struct TestRam {
    bytes: Vec<u8>,
}

impl TestRam {
    pub(super) fn new(len: usize) -> Self {
        Self {
            bytes: vec![0; len],
        }
    }

    pub(super) fn write_u32(&mut self, gpa: u64, value: u32) {
        assert!(self.write_bytes(gpa, &value.to_le_bytes()));
    }

    pub(super) fn write_u64(&mut self, gpa: u64, value: u64) {
        assert!(self.write_bytes(gpa, &value.to_le_bytes()));
    }

    pub(super) fn read_u32(&self, gpa: u64) -> u32 {
        let bytes = self.read_bytes(gpa, 4).unwrap();
        u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
    }

    pub(super) fn read_u64(&self, gpa: u64) -> u64 {
        let bytes = self.read_bytes(gpa, 8).unwrap();
        u64::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ])
    }
}

impl GuestMemoryMut for TestRam {
    fn write_bytes(&mut self, gpa: u64, data: &[u8]) -> bool {
        let Ok(start) = usize::try_from(gpa) else {
            return false;
        };
        let Some(end) = start.checked_add(data.len()) else {
            return false;
        };
        if end > self.bytes.len() {
            return false;
        }
        self.bytes[start..end].copy_from_slice(data);
        true
    }

    fn read_bytes(&self, gpa: u64, len: usize) -> Option<Vec<u8>> {
        let start = usize::try_from(gpa).ok()?;
        let end = start.checked_add(len)?;
        if end > self.bytes.len() {
            return None;
        }
        Some(self.bytes[start..end].to_vec())
    }
}

pub(super) fn setup_command_rings(
    xhci: &mut XhciController,
    mem: &mut TestRam,
    command_control: u32,
) {
    setup_command_rings_with_parameter(xhci, mem, 0, command_control);
}

pub(super) fn setup_command_rings_with_parameter(
    xhci: &mut XhciController,
    mem: &mut TestRam,
    parameter: u64,
    command_control: u32,
) {
    mem.write_u64(CMD_RING, parameter);
    mem.write_u32(CMD_RING + 12, command_control);
    setup_event_ring(xhci, mem);
}

pub(super) fn setup_event_ring(xhci: &mut XhciController, mem: &mut TestRam) {
    mem.write_u64(ERST, EVENT_RING);
    mem.write_u32(ERST + 8, 16);
    xhci.mmio_write(0x58, 8, CMD_RING | 1);
    xhci.mmio_write(0x70, 8, DCBAA);
    xhci.mmio_write(0x78, 4, 64);
    xhci.mmio_write(0x1028, 4, 1);
    xhci.mmio_write(0x1030, 8, ERST);
    xhci.mmio_write(0x1038, 8, EVENT_RING | 0x8);
    xhci.mmio_write(0x1020, 4, 0x2);
}

/// Windows bootmgr programs interrupter 1 for transfer events (its transfer
/// TRBs carry interrupter target 1) while commands stay on interrupter 0.
pub(super) fn setup_secondary_event_ring(xhci: &mut XhciController, mem: &mut TestRam) {
    mem.write_u64(ERST1, EVENT_RING1);
    mem.write_u32(ERST1 + 8, 16);
    xhci.mmio_write(0x1048, 4, 1);
    xhci.mmio_write(0x1050, 8, ERST1);
    xhci.mmio_write(0x1058, 8, EVENT_RING1 | 0x8);
    xhci.mmio_write(0x1040, 4, 0x2);
}

pub(super) fn command_control(trb_type: u32, slot_id: u32) -> u32 {
    command_control_with_cycle(trb_type, slot_id, true)
}

pub(super) fn command_control_with_cycle(trb_type: u32, slot_id: u32, cycle: bool) -> u32 {
    let cycle_bit = u32::from(cycle);
    (slot_id << 24) | (trb_type << 10) | cycle_bit
}

pub(super) fn setup_packet_parameter(fields: SetupPacketFields) -> u64 {
    u64::from(fields.bm_request_type)
        | (u64::from(fields.request) << 8)
        | (u64::from(fields.value) << 16)
        | (u64::from(fields.index) << 32)
        | (u64::from(fields.length) << 48)
}

pub(super) fn assert_success_completion(
    mem: &TestRam,
    event_gpa: u64,
    command_gpa: u64,
    expected_slot_id: u32,
) {
    assert_eq!(mem.read_u64(event_gpa), command_gpa);
    assert_eq!(mem.read_u32(event_gpa + 8) >> 24, COMPLETION_CODE_SUCCESS);
    let control = mem.read_u32(event_gpa + 12);
    assert_eq!((control >> 10) & 0x3f, TRB_TYPE_COMMAND_COMPLETION_EVENT);
    assert_eq!((control >> 24) & 0xff, expected_slot_id);
    assert_eq!(control & 1, 1);
}

pub(super) fn assert_success_transfer_event_for_trb(mem: &TestRam, event_gpa: u64, trb_gpa: u64) {
    assert_success_transfer_event(mem, event_gpa, trb_gpa, 0, 0);
}

pub(super) fn assert_success_event_data_transfer_event(
    mem: &TestRam,
    event_gpa: u64,
    event_parameter: u64,
    edtla: u32,
) {
    assert_success_transfer_event(mem, event_gpa, event_parameter, TRB_EVENT_DATA, edtla);
}

fn assert_success_transfer_event(
    mem: &TestRam,
    event_gpa: u64,
    parameter: u64,
    event_data: u32,
    transfer_length: u32,
) {
    assert_eq!(mem.read_u64(event_gpa), parameter);
    assert_eq!(mem.read_u32(event_gpa + 8) & 0x00ff_ffff, transfer_length);
    assert_eq!(mem.read_u32(event_gpa + 8) >> 24, COMPLETION_CODE_SUCCESS);
    let control = mem.read_u32(event_gpa + 12);
    assert_eq!((control >> 10) & 0x3f, TRB_TYPE_TRANSFER_EVENT);
    assert_eq!((control >> 16) & 0x1f, 1);
    assert_eq!((control >> 24) & 0xff, 1);
    assert_eq!(control & TRB_EVENT_DATA, event_data);
    assert_eq!(control & 1, 1);
    assert_ne!(mem.read_u64(event_gpa), CMD_RING);
}
