use super::test_support::{command_control, xhci_target, TestMemory};
use super::*;

const COMMAND_RING: u64 = 0x1000;
const INPUT_CONTEXT: u64 = 0x2000;
const EP0_RING: u64 = 0x3000;
const OVERFLOWING_EP0_RING: u64 = u64::MAX - 0xf;
const EP0_CONTEXT_OFFSET: u64 = context::EP0_CONTEXT_OFFSET;
const EP_TR_DEQUEUE_OFFSET: u64 = context::EP_TR_DEQUEUE_OFFSET;

#[derive(Debug)]
struct FirstOverflowingTransferTrbReadableMemory {
    base: TestMemory,
}

impl FirstOverflowingTransferTrbReadableMemory {
    fn new(len: usize) -> Self {
        Self {
            base: TestMemory::new(len),
        }
    }

    fn write_u32(&mut self, gpa: u64, value: u32) {
        self.base.write_u32(gpa, value);
    }

    fn write_u64(&mut self, gpa: u64, value: u64) {
        self.base.write_u64(gpa, value);
    }
}

impl GuestMemoryMut for FirstOverflowingTransferTrbReadableMemory {
    fn write_bytes(&mut self, gpa: u64, data: &[u8]) -> bool {
        self.base.write_bytes(gpa, data)
    }

    fn read_bytes(&self, gpa: u64, len: usize) -> Option<Vec<u8>> {
        if gpa == OVERFLOWING_EP0_RING && len == trb::BYTES {
            return Some(vec![0; trb::BYTES]);
        }
        self.base.read_bytes(gpa, len)
    }
}

#[test]
fn split_crcr_write_preserves_cycle_bit_for_command_trace() {
    let mut mem = TestMemory::new(0x2_0000);
    let mut trace = XhciBringupTrace::new(16);
    mem.write_u32(COMMAND_RING + 12, command_control(trb::TYPE_ENABLE_SLOT, 0));

    trace.record_mmio(
        Some(xhci_target(CRCR)),
        &MmioOp::Write {
            size: 4,
            value: COMMAND_RING | trb::CYCLE as u64,
        },
        &mem,
    );
    trace.record_mmio(
        Some(xhci_target(CRCR_HI)),
        &MmioOp::Write { size: 4, value: 0 },
        &mem,
    );
    trace.record_mmio(
        Some(xhci_target(DOORBELL_BASE)),
        &MmioOp::Write { size: 4, value: 0 },
        &mem,
    );

    assert!(trace
        .events
        .iter()
        .any(|event| event.contains("dequeue=0x1000 cycle=true")));
    assert!(trace
        .events
        .iter()
        .any(|event| event.contains("type=enable_slot")));
    assert!(!trace
        .events
        .iter()
        .any(|event| event.contains("cycle_mismatch")));
}

#[test]
fn slot_doorbell_reports_transfer_ring_gpa_overflow_without_panic() {
    let mut mem = FirstOverflowingTransferTrbReadableMemory::new(0x5000);
    let mut trace = XhciBringupTrace::new(16);
    mem.write_u64(COMMAND_RING, INPUT_CONTEXT);
    mem.write_u32(
        COMMAND_RING + 12,
        command_control(trb::TYPE_ADDRESS_DEVICE, 1),
    );
    mem.write_u64(
        INPUT_CONTEXT + EP0_CONTEXT_OFFSET + EP_TR_DEQUEUE_OFFSET,
        OVERFLOWING_EP0_RING | trb::CYCLE as u64,
    );

    trace.record_mmio(
        Some(xhci_target(CRCR)),
        &MmioOp::Write {
            size: 8,
            value: COMMAND_RING | trb::CYCLE as u64,
        },
        &mem,
    );
    trace.record_mmio(
        Some(xhci_target(DOORBELL_BASE)),
        &MmioOp::Write { size: 4, value: 0 },
        &mem,
    );
    trace.record_mmio(
        Some(xhci_target(DOORBELL_BASE + DOORBELL_STRIDE)),
        &MmioOp::Write { size: 4, value: 1 },
        &mem,
    );

    assert!(trace.events.iter().any(
        |event| event.contains("transfer_trb slot=1 target=0x1 ring=ep0 index=1 gpa=overflow")
    ));
}

#[test]
fn slot_doorbell_records_setup_stage_trb_details() {
    let mut mem = TestMemory::new(0x5000);
    let mut trace = XhciBringupTrace::new(16);
    mem.write_u64(COMMAND_RING, INPUT_CONTEXT);
    mem.write_u32(
        COMMAND_RING + 12,
        command_control(trb::TYPE_ADDRESS_DEVICE, 1),
    );
    mem.write_u64(
        INPUT_CONTEXT + EP0_CONTEXT_OFFSET + EP_TR_DEQUEUE_OFFSET,
        EP0_RING | trb::CYCLE as u64,
    );
    let get_descriptor = u64::from_le_bytes([0x80, 0x06, 0x00, 0x01, 0x00, 0x00, 0x12, 0x00]);
    mem.write_u64(EP0_RING, get_descriptor);
    mem.write_u32(EP0_RING + 8, 8);
    mem.write_u32(
        EP0_RING + 12,
        (trb::TYPE_SETUP_STAGE << trb::TYPE_SHIFT) | trb::CYCLE,
    );

    trace.record_mmio(
        Some(xhci_target(CRCR)),
        &MmioOp::Write {
            size: 8,
            value: COMMAND_RING | trb::CYCLE as u64,
        },
        &mem,
    );
    trace.record_mmio(
        Some(xhci_target(DOORBELL_BASE)),
        &MmioOp::Write { size: 4, value: 0 },
        &mem,
    );
    trace.record_mmio(
        Some(xhci_target(DOORBELL_BASE + DOORBELL_STRIDE)),
        &MmioOp::Write { size: 4, value: 1 },
        &mem,
    );

    assert!(trace
        .events
        .iter()
        .any(|event| event.contains("setup_stage")
            && event.contains("bm=0x80")
            && event.contains("req=0x06")
            && event.contains("value=0x0100")
            && event.contains("len=18")));
}
