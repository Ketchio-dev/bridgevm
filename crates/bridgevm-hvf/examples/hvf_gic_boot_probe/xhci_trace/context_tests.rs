use super::test_support::{command_control, xhci_target, TestMemory};
use super::*;

const COMMAND_RING: u64 = 0x1000;
const INPUT_CONTEXT: u64 = 0x2000;
const EP0_RING: u64 = 0x3000;
const EP0_CONTEXT_OFFSET: u64 = context::EP0_CONTEXT_OFFSET;
const EP_TR_DEQUEUE_OFFSET: u64 = context::EP_TR_DEQUEUE_OFFSET;

#[test]
fn address_device_command_records_ep0_dequeue_from_input_context() {
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

    assert!(trace.events.iter().any(
        |event| event.contains("address_device slot=1") && event.contains("ep0_dequeue=0x3000")
    ));
}

#[test]
fn address_device_trace_reports_overflowing_input_context_without_panic() {
    let mut mem = TestMemory::new(0x2000);
    let mut trace = XhciBringupTrace::new(16);
    mem.write_u64(COMMAND_RING, u64::MAX);
    mem.write_u32(
        COMMAND_RING + 12,
        command_control(trb::TYPE_ADDRESS_DEVICE, 1),
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

    assert!(trace
        .events
        .iter()
        .any(|event| event.contains("address_device slot=1")
            && event.contains("ep0_context=overflow")
            && event.contains("ep0_dequeue=unreadable")));
}
