use super::test_support::{command_control, xhci_target, TestMemory};
use super::*;

const COMMAND_RING: u64 = 0x1000;
const TYPE_SET_TR_DEQUEUE_POINTER: u32 = 16;

#[test]
fn evaluate_context_command_is_named_and_advances_trace_dequeue() {
    // Given: a command ring contains Evaluate Context followed by Enable Slot.
    let mut mem = TestMemory::new(0x5000);
    let mut trace = XhciBringupTrace::new(16);
    mem.write_u32(
        COMMAND_RING + 12,
        command_control(trb::TYPE_EVALUATE_CONTEXT, 1),
    );
    mem.write_u32(
        COMMAND_RING + trb::BYTES_U64 + 12,
        command_control(trb::TYPE_ENABLE_SLOT, 0),
    );

    trace.record_mmio(
        Some(xhci_target(CRCR)),
        &MmioOp::Write {
            size: 8,
            value: COMMAND_RING | trb::CYCLE as u64,
        },
        &mem,
    );

    // When: the guest rings command doorbell 0 twice.
    trace.record_mmio(
        Some(xhci_target(DOORBELL_BASE)),
        &MmioOp::Write { size: 4, value: 0 },
        &mem,
    );
    trace.record_mmio(
        Some(xhci_target(DOORBELL_BASE)),
        &MmioOp::Write { size: 4, value: 0 },
        &mem,
    );

    // Then: the trace names Evaluate Context and advances to the next command.
    assert!(trace
        .events
        .iter()
        .any(|event| event.contains("command_trb gpa=0x1000 type=evaluate_context")));
    assert!(trace
        .events
        .iter()
        .any(|event| event.contains("command_trb gpa=0x1010 type=enable_slot")));
}

#[test]
fn stop_endpoint_command_is_named_and_advances_trace_dequeue() {
    // Given: a command ring contains Stop Endpoint followed by Enable Slot.
    let mut mem = TestMemory::new(0x5000);
    let mut trace = XhciBringupTrace::new(16);
    mem.write_u32(
        COMMAND_RING + 12,
        command_control(trb::TYPE_STOP_ENDPOINT, 1),
    );
    mem.write_u32(
        COMMAND_RING + trb::BYTES_U64 + 12,
        command_control(trb::TYPE_ENABLE_SLOT, 0),
    );

    trace.record_mmio(
        Some(xhci_target(CRCR)),
        &MmioOp::Write {
            size: 8,
            value: COMMAND_RING | trb::CYCLE as u64,
        },
        &mem,
    );

    // When: the guest rings command doorbell 0 twice.
    trace.record_mmio(
        Some(xhci_target(DOORBELL_BASE)),
        &MmioOp::Write { size: 4, value: 0 },
        &mem,
    );
    trace.record_mmio(
        Some(xhci_target(DOORBELL_BASE)),
        &MmioOp::Write { size: 4, value: 0 },
        &mem,
    );

    // Then: the trace names Stop Endpoint and advances to the next command.
    assert!(
        trace
            .events
            .iter()
            .any(|event| event.contains("command_trb gpa=0x1000 type=stop_endpoint")),
        "{:#?}",
        trace.events
    );
    assert!(
        trace
            .events
            .iter()
            .any(|event| event.contains("command_trb gpa=0x1010 type=enable_slot")),
        "{:#?}",
        trace.events
    );
}

#[test]
fn set_tr_dequeue_pointer_command_is_named_and_advances_trace_dequeue() {
    // Given: a command ring contains Set TR Dequeue Pointer followed by Enable Slot.
    let mut mem = TestMemory::new(0x5000);
    let mut trace = XhciBringupTrace::new(16);
    mem.write_u32(
        COMMAND_RING + 12,
        command_control(TYPE_SET_TR_DEQUEUE_POINTER, 1),
    );
    mem.write_u32(
        COMMAND_RING + trb::BYTES_U64 + 12,
        command_control(trb::TYPE_ENABLE_SLOT, 0),
    );

    trace.record_mmio(
        Some(xhci_target(CRCR)),
        &MmioOp::Write {
            size: 8,
            value: COMMAND_RING | trb::CYCLE as u64,
        },
        &mem,
    );

    // When: the guest rings command doorbell 0 twice.
    trace.record_mmio(
        Some(xhci_target(DOORBELL_BASE)),
        &MmioOp::Write { size: 4, value: 0 },
        &mem,
    );
    trace.record_mmio(
        Some(xhci_target(DOORBELL_BASE)),
        &MmioOp::Write { size: 4, value: 0 },
        &mem,
    );

    // Then: the trace names Set TR Dequeue Pointer and advances to the next command.
    assert!(
        trace
            .events
            .iter()
            .any(|event| event.contains("command_trb gpa=0x1000 type=set_tr_dequeue_pointer")),
        "{:#?}",
        trace.events
    );
    assert!(
        trace
            .events
            .iter()
            .any(|event| event.contains("command_trb gpa=0x1010 type=enable_slot")),
        "{:#?}",
        trace.events
    );
}
