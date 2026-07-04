use super::test_support::{command_control, xhci_target, TestMemory};
use super::*;

const COMMAND_RING: u64 = 0x1000;
const INPUT_CONTEXT: u64 = 0x2000;
const DCI3_RING: u64 = 0x4000;
const NEW_DCI3_RING: u64 = 0x7000;
const DCI3_BUFFER: u64 = 0x7800;
const TYPE_SET_TR_DEQUEUE_POINTER: u32 = 16;
const COMMAND_ENDPOINT_ID_SHIFT: u32 = 16;
const EP_TR_DEQUEUE_OFFSET: u64 = context::EP_TR_DEQUEUE_OFFSET;
const DCI3: u32 = context::DCI3;
const INPUT_CONTROL_ADD_CONTEXT_OFFSET: u64 = context::INPUT_CONTROL_ADD_CONTEXT_OFFSET;
const DCI3_INPUT_CONTEXT_OFFSET: u64 = context::DCI3_INPUT_CONTEXT_OFFSET;
const TRB_TYPE_NORMAL: u32 = 1;

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

#[test]
fn set_tr_dequeue_pointer_updates_dci3_trace_dequeue() {
    // Given: DCI3 was configured on one transfer ring, then Windows moves it.
    let mut mem = TestMemory::new(0x8000);
    let mut trace = XhciBringupTrace::new(64);
    mem.write_u64(COMMAND_RING, INPUT_CONTEXT);
    mem.write_u32(
        COMMAND_RING + 12,
        command_control(trb::TYPE_CONFIGURE_ENDPOINT, 1),
    );
    mem.write_u32(INPUT_CONTEXT + INPUT_CONTROL_ADD_CONTEXT_OFFSET, 1 << DCI3);
    mem.write_u64(
        INPUT_CONTEXT + DCI3_INPUT_CONTEXT_OFFSET + EP_TR_DEQUEUE_OFFSET,
        DCI3_RING | trb::CYCLE as u64,
    );
    let set_dequeue_command = COMMAND_RING + trb::BYTES_U64;
    mem.write_u64(set_dequeue_command, NEW_DCI3_RING | trb::CYCLE as u64);
    mem.write_u32(
        set_dequeue_command + 12,
        command_control(TYPE_SET_TR_DEQUEUE_POINTER, 1) | (DCI3 << COMMAND_ENDPOINT_ID_SHIFT),
    );
    mem.write_u64(NEW_DCI3_RING, DCI3_BUFFER);
    mem.write_u32(NEW_DCI3_RING + 8, 8);
    mem.write_u32(
        NEW_DCI3_RING + 12,
        (TRB_TYPE_NORMAL << trb::TYPE_SHIFT) | trb::CYCLE,
    );

    trace.record_mmio(
        Some(xhci_target(CRCR)),
        &MmioOp::Write {
            size: 8,
            value: COMMAND_RING | trb::CYCLE as u64,
        },
        &mem,
    );

    // When: the command ring is processed and the DCI3 doorbell is traced.
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
    trace.record_mmio(
        Some(xhci_target(DOORBELL_BASE + DOORBELL_STRIDE)),
        &MmioOp::Write {
            size: 4,
            value: u64::from(DCI3),
        },
        &mem,
    );

    // Then: the trace shadow uses the Set TR Dequeue Pointer value for DCI3.
    assert!(
        trace.events.iter().any(|event| event.contains(
            "set_tr_dequeue_pointer slot=1 endpoint=3 raw_dequeue=0x7001 dci3_dequeue=0x7000"
        )),
        "{:#?}",
        trace.events
    );
    assert!(
        trace.events.iter().any(|event| event.contains(
            "doorbell[1] slot=1 target=0x3 value=0x3 ep0_dequeue=0x0 dci3_dequeue=0x7000"
        )),
        "{:#?}",
        trace.events
    );
    assert!(
        trace
            .events
            .iter()
            .any(|event| event
                .contains("transfer_trb slot=1 target=0x3 ring=dci3 index=0 gpa=0x7000")),
        "{:#?}",
        trace.events
    );
}
