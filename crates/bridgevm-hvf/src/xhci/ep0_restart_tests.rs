//! A stopped EP0 resumes on its doorbell, before publishing transfer events.

use super::*;
use crate::fwcfg::GuestMemoryMut;
use crate::xhci::test_support::{assert_success_event_data_transfer_event, EVENT_RING1};

const EP0_CONTEXT: u64 = OUTPUT_CONTEXT + EP0_OUTPUT_CONTEXT_OFFSET;
const EP_STATE_RUNNING: u32 = 1;

struct ObservedMemory {
    ram: TestRam,
    event_states: Vec<u32>,
    reject_state_write: bool,
}

impl GuestMemoryMut for ObservedMemory {
    fn read_bytes(&self, gpa: u64, len: usize) -> Option<Vec<u8>> {
        self.ram.read_bytes(gpa, len)
    }

    fn read_into(&self, gpa: u64, dst: &mut [u8]) -> bool {
        self.ram.read_into(gpa, dst)
    }

    fn write_bytes(&mut self, gpa: u64, data: &[u8]) -> bool {
        if self.reject_state_write && gpa == EP0_CONTEXT {
            return false;
        }
        if gpa == EVENT_RING1 + TRB_SIZE && data.len() == TRB_SIZE as usize {
            self.event_states
                .push(self.ram.read_u32(EP0_CONTEXT) & EP_STATE_MASK);
        }
        self.ram.write_bytes(gpa, data)
    }
}

fn stopped_ep0(restart_dequeue: u64) -> (XhciController, ObservedMemory) {
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x9000);
    write_ep0_input_context(&mut mem, EP0_RING | 1);
    setup_command_rings_with_parameter(
        &mut xhci,
        &mut mem,
        INPUT_CONTEXT,
        command_control(TRB_TYPE_ADDRESS_DEVICE, ENABLE_SLOT_ID),
    );
    mem.write_u32(CMD_RING + TRB_SIZE + 12, stop_endpoint_control());
    mem.write_u64(CMD_RING + TRB_SIZE * 2, restart_dequeue | 1);
    mem.write_u32(
        CMD_RING + TRB_SIZE * 2 + 12,
        set_tr_dequeue_pointer_control(),
    );
    mem.write_u64(DCBAA + u64::from(ENABLE_SLOT_ID) * 8, OUTPUT_CONTEXT);
    setup_secondary_event_ring(&mut xhci, &mut mem);
    write_get_descriptor_device_transfer(&mut mem);
    xhci.mmio_write(0x40, 4, 1);
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, 1, &mut mem));
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));
    assert_success_completion(
        &mem,
        EVENT_RING + TRB_SIZE * 2,
        CMD_RING + TRB_SIZE * 2,
        ENABLE_SLOT_ID,
    );
    assert_eq!(mem.read_u32(EP0_CONTEXT) & EP_STATE_MASK, EP_STATE_STOPPED);
    assert_eq!(
        mem.read_u64(EP0_CONTEXT + EP_TR_DEQUEUE_OFFSET),
        restart_dequeue | 1
    );
    (
        xhci,
        ObservedMemory {
            ram: mem,
            event_states: Vec::new(),
            reject_state_write: false,
        },
    )
}

#[test]
fn ep0_restart_publishes_running_before_transfer_completion() {
    let (mut xhci, mut mem) = stopped_ep0(EP0_RING);
    let descriptor = mem.ram.read_bytes(DATA_STAGE_BUFFER, 18).unwrap();
    assert!(mem.ram.write_bytes(DATA_STAGE_BUFFER, &[0xa5; 18]));
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, 1, &mut mem));
    assert_eq!(
        mem.ram.read_bytes(DATA_STAGE_BUFFER, 18).unwrap(),
        descriptor
    );
    assert_success_event_data_transfer_event(
        &mem.ram,
        EVENT_RING1 + TRB_SIZE,
        EVENT_DATA_PARAMETER,
        18,
    );
    assert_eq!(mem.event_states, vec![EP_STATE_RUNNING]);
    assert_eq!(
        mem.ram.read_u32(EP0_CONTEXT) & EP_STATE_MASK,
        EP_STATE_RUNNING
    );
    assert_eq!(
        mem.ram.read_u64(EP0_CONTEXT + EP_TR_DEQUEUE_OFFSET),
        (EP0_RING + TRB_SIZE * 4) | 1
    );
}

#[test]
fn ep0_restart_with_empty_ring_still_publishes_running() {
    let (mut xhci, mut mem) = stopped_ep0(EP0_RING + TRB_SIZE * 4);
    let mut expected = mem.ram.read_bytes(EP0_CONTEXT, 32).unwrap();
    expected[0] = (expected[0] & !EP_STATE_MASK as u8) | EP_STATE_RUNNING as u8;
    assert!(!xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, 1, &mut mem));
    assert!(mem.event_states.is_empty());
    assert_eq!(mem.ram.read_bytes(EP0_CONTEXT, 32).unwrap(), expected);
}

#[test]
fn ep0_restart_does_not_complete_if_running_state_cannot_be_published() {
    let (mut xhci, mut mem) = stopped_ep0(EP0_RING);
    mem.reject_state_write = true;
    assert!(mem.ram.write_bytes(DATA_STAGE_BUFFER, &[0xa5; 18]));
    assert!(!xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, 1, &mut mem));
    assert!(mem.event_states.is_empty());
    assert_eq!(
        mem.ram.read_bytes(DATA_STAGE_BUFFER, 18).unwrap(),
        [0xa5; 18]
    );
    assert_eq!(
        mem.ram.read_u32(EP0_CONTEXT) & EP_STATE_MASK,
        EP_STATE_STOPPED
    );
    assert_eq!(
        mem.ram.read_u64(EP0_CONTEXT + EP_TR_DEQUEUE_OFFSET),
        EP0_RING | 1
    );
}
