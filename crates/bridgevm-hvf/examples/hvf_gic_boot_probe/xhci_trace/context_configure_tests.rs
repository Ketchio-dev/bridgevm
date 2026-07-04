use super::test_support::{command_control, xhci_target, TestMemory};
use super::*;

const COMMAND_RING: u64 = 0x1000;
const INPUT_CONTEXT: u64 = 0x2000;
const DCI3_RING: u64 = 0x4000;
const DCI3_BUFFER: u64 = 0x4800;
const DCI5_RING: u64 = 0x5200;
const DCI5_BUFFER: u64 = 0x5800;
const EP_TR_DEQUEUE_OFFSET: u64 = context::EP_TR_DEQUEUE_OFFSET;
const DCI3: u32 = context::DCI3;
const DCI5: u32 = context::DCI5;
const INPUT_CONTROL_ADD_CONTEXT_OFFSET: u64 = context::INPUT_CONTROL_ADD_CONTEXT_OFFSET;
const DCI3_INPUT_CONTEXT_OFFSET: u64 = context::DCI3_INPUT_CONTEXT_OFFSET;
const DCI5_INPUT_CONTEXT_OFFSET: u64 = context::DCI5_INPUT_CONTEXT_OFFSET;
const TRB_TYPE_NORMAL: u32 = 1;

#[test]
fn configure_endpoint_command_records_dci3_context_for_interrupt_in_trace() {
    let mut mem = TestMemory::new(0x7000);
    let mut trace = XhciBringupTrace::new(32);
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
    mem.write_u32(DCI3_RING + 8, 8);
    mem.write_u32(
        DCI3_RING + 12,
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

    assert!(trace
        .events
        .iter()
        .any(|event| event.contains("configure_endpoint slot=1")
            && event.contains("add_context=0x8")
            && event.contains("dci3_dequeue=0x4000")
            && event.contains("dci3_dcs=true")));
    assert!(
        trace
            .events
            .iter()
            .any(|event| event
                .contains("transfer_trb slot=1 target=0x3 ring=dci3 index=0 gpa=0x4000"))
    );
}

#[test]
fn configure_endpoint_command_records_dci5_context_for_pointer_interrupt_in_trace() {
    let mut mem = TestMemory::new(0x8000);
    let mut trace = XhciBringupTrace::new(32);
    mem.write_u64(COMMAND_RING, INPUT_CONTEXT);
    mem.write_u32(
        COMMAND_RING + 12,
        command_control(trb::TYPE_CONFIGURE_ENDPOINT, 1),
    );
    mem.write_u32(INPUT_CONTEXT + INPUT_CONTROL_ADD_CONTEXT_OFFSET, 1 << DCI5);
    mem.write_u64(
        INPUT_CONTEXT + DCI5_INPUT_CONTEXT_OFFSET + EP_TR_DEQUEUE_OFFSET,
        DCI5_RING | trb::CYCLE as u64,
    );
    mem.write_u64(DCI5_RING, DCI5_BUFFER);
    mem.write_u32(DCI5_RING + 8, 8);
    mem.write_u32(
        DCI5_RING + 12,
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
    trace.record_mmio(
        Some(xhci_target(DOORBELL_BASE)),
        &MmioOp::Write { size: 4, value: 0 },
        &mem,
    );
    trace.record_mmio(
        Some(xhci_target(DOORBELL_BASE + DOORBELL_STRIDE)),
        &MmioOp::Write {
            size: 4,
            value: u64::from(DCI5),
        },
        &mem,
    );

    assert!(trace
        .events
        .iter()
        .any(|event| event.contains("configure_endpoint slot=1")
            && event.contains("add_context=0x20")
            && event.contains("dci3=not-added")
            && event.contains("dci5_dequeue=0x5200")
            && event.contains("dci5_dcs=true")));
    assert!(
        trace
            .events
            .iter()
            .any(|event| event
                .contains("transfer_trb slot=1 target=0x5 ring=dci5 index=0 gpa=0x5200"))
    );
}
