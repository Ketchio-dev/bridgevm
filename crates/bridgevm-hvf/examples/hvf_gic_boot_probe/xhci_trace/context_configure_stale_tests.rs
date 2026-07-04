use super::test_support::{command_control, xhci_target, TestMemory};
use super::*;

const COMMAND_RING: u64 = 0x1000;
const INPUT_CONTEXT: u64 = 0x2000;
const INPUT_CONTEXT_WITHOUT_DCI3: u64 = 0x6000;
const INPUT_CONTEXT_DROP_DCI3_WITH_UNREADABLE_ADD: u64 = 0x7ff0;
const DCI3_RING: u64 = 0x4000;
const DCI3_BUFFER: u64 = 0x4800;
const IGNORED_DCI3_RING: u64 = 0x5000;
const EP_TR_DEQUEUE_OFFSET: u64 = context::EP_TR_DEQUEUE_OFFSET;
const DCI3: u32 = context::DCI3;
const INPUT_CONTROL_ADD_CONTEXT_OFFSET: u64 = context::INPUT_CONTROL_ADD_CONTEXT_OFFSET;
const DCI3_INPUT_CONTEXT_OFFSET: u64 = context::DCI3_INPUT_CONTEXT_OFFSET;
const TRB_TYPE_NORMAL: u32 = 1;

#[test]
fn configure_endpoint_without_dci3_add_context_does_not_capture_stale_dci3_context() {
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
    let second_command = COMMAND_RING + trb::BYTES_U64;
    mem.write_u64(second_command, INPUT_CONTEXT_WITHOUT_DCI3);
    mem.write_u32(
        second_command + 12,
        command_control(trb::TYPE_CONFIGURE_ENDPOINT, 1),
    );
    mem.write_u32(
        INPUT_CONTEXT_WITHOUT_DCI3 + INPUT_CONTROL_ADD_CONTEXT_OFFSET,
        0,
    );
    mem.write_u64(
        INPUT_CONTEXT_WITHOUT_DCI3 + DCI3_INPUT_CONTEXT_OFFSET + EP_TR_DEQUEUE_OFFSET,
        IGNORED_DCI3_RING | trb::CYCLE as u64,
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
        Some(xhci_target(DOORBELL_BASE)),
        &MmioOp::Write { size: 4, value: 0 },
        &mem,
    );

    assert!(trace.events.iter().any(|event| event.contains(
        "configure_endpoint slot=1 input_context=0x6000 drop_context=0x0 add_context=0x0 dci3=not-added"
    )));
    assert!(!trace
        .events
        .iter()
        .any(|event| event.contains("dci3_dequeue=0x5000")));
}

#[test]
fn configure_endpoint_drop_dci3_clears_stale_ring_before_unreadable_add_context() {
    let mut mem = TestMemory::new((INPUT_CONTEXT_DROP_DCI3_WITH_UNREADABLE_ADD + 4) as usize);
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
    mem.write_u64(DCI3_RING, DCI3_BUFFER);
    mem.write_u32(DCI3_RING + 12, TRB_TYPE_NORMAL << trb::TYPE_SHIFT);
    let second_command = COMMAND_RING + trb::BYTES_U64;
    mem.write_u64(second_command, INPUT_CONTEXT_DROP_DCI3_WITH_UNREADABLE_ADD);
    mem.write_u32(
        second_command + 12,
        command_control(trb::TYPE_CONFIGURE_ENDPOINT, 1),
    );
    mem.write_u32(INPUT_CONTEXT_DROP_DCI3_WITH_UNREADABLE_ADD, 1 << DCI3);

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

    assert!(trace.events.iter().any(|event| event.contains(
        "configure_endpoint slot=1 input_context=0x7ff0 drop_context=0x8 add_context=unreadable"
    )));
    assert!(!trace
        .events
        .iter()
        .any(|event| event.contains("transfer_trb slot=1 target=0x3 ring=dci3")));
}
