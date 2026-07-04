use super::test_support::{command_control, xhci_target, TestMemory};
use super::*;

const COMMAND_RING: u64 = 0x1000;
const INPUT_CONTEXT: u64 = 0x2000;
const DCI3_RING: u64 = 0x4000;
const NEW_DCI3_RING: u64 = 0x7000;
const DCI3_BUFFER: u64 = 0x7800;
const DCI5_RING: u64 = 0x8200;
const NEW_DCI5_RING: u64 = 0x8400;
const DCI5_BUFFER: u64 = 0x8800;
const TYPE_SET_TR_DEQUEUE_POINTER: u32 = 16;
const COMMAND_ENDPOINT_ID_SHIFT: u32 = 16;
const EP_TR_DEQUEUE_OFFSET: u64 = context::EP_TR_DEQUEUE_OFFSET;
const DCI3: u32 = context::DCI3;
const DCI5: u32 = context::DCI5;
const INPUT_CONTROL_ADD_CONTEXT_OFFSET: u64 = context::INPUT_CONTROL_ADD_CONTEXT_OFFSET;
const DCI3_INPUT_CONTEXT_OFFSET: u64 = context::DCI3_INPUT_CONTEXT_OFFSET;
const DCI5_INPUT_CONTEXT_OFFSET: u64 = context::DCI5_INPUT_CONTEXT_OFFSET;
const TRB_TYPE_NORMAL: u32 = 1;

#[test]
fn dci3_lifecycle_summary_survives_recent_trace_truncation() {
    let mut mem = TestMemory::new(0x9000);
    let mut trace = XhciBringupTrace::new(2);
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
    trace.record_mmio(
        Some(xhci_target(ERDP0)),
        &MmioOp::Write {
            size: 8,
            value: 0x9000,
        },
        &mem,
    );
    trace.record_mmio(
        Some(xhci_target(CONFIG)),
        &MmioOp::Write { size: 4, value: 1 },
        &mem,
    );
    trace.record_mmio(
        Some(xhci_target(CONFIG)),
        &MmioOp::Write { size: 4, value: 2 },
        &mem,
    );

    let event_stats = bridgevm_hvf::xhci::XhciEventLifecycleStats {
        event_post_attempts: 4,
        event_post_successes: 3,
        event_post_failures: 1,
        command_completion_event_posts: 2,
        transfer_event_posts: 1,
        erdp_updates: 2,
        erdp_ehb_consumed: 1,
        last_erdp: 0x9000,
        last_event_interrupter: 1,
        last_event_gpa: 0x9100,
        last_event_parameter: NEW_DCI3_RING,
        last_event_status: 0x0100_0000,
        last_event_control: (32 << trb::TYPE_SHIFT) | trb::CYCLE,
        ..Default::default()
    };
    let summary = trace.summary_lines(event_stats).join("\n");

    assert!(!trace
        .events
        .iter()
        .any(|event| event.contains("set_tr_dequeue_pointer")));
    assert!(summary.contains(
        "dci3_set_tr_dequeue_pointer count=1 last_slot=1 last_endpoint=3 last_raw_dequeue=0x7001 last_dequeue=0x7000 last_dcs=true"
    ));
    assert!(summary.contains("dci3_doorbell count=1 last_slot=1 last_target=0x3 last_value=0x3"));
    assert!(summary.contains(
        "dci3_transfer_ring_snapshot count=1 last_slot=1 last_target=0x3 last_dequeue=0x7000 trbs_read=4 nonzero_trbs=1 zero_type0_trbs=3 first_nonzero_index=0 first_zero_type0_index=1"
    ));
    assert!(summary.contains("guest_erdp0_writes count=1 last_erdp0=0x9000"));
    assert!(summary.contains(
        "xhci_event_posts attempts=4 successes=3 failures=1 command_completion=2 transfer=1"
    ));
    assert!(summary.contains("model_erdp_updates=2 model_erdp_ehb_consumed=1"));
}

#[test]
fn dci5_lifecycle_summary_survives_recent_trace_truncation() {
    let mut mem = TestMemory::new(0xa000);
    let mut trace = XhciBringupTrace::new(2);
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
    let set_dequeue_command = COMMAND_RING + trb::BYTES_U64;
    mem.write_u64(set_dequeue_command, NEW_DCI5_RING | trb::CYCLE as u64);
    mem.write_u32(
        set_dequeue_command + 12,
        command_control(TYPE_SET_TR_DEQUEUE_POINTER, 1) | (DCI5 << COMMAND_ENDPOINT_ID_SHIFT),
    );
    mem.write_u64(NEW_DCI5_RING, DCI5_BUFFER);
    mem.write_u32(NEW_DCI5_RING + 8, 8);
    mem.write_u32(
        NEW_DCI5_RING + 12,
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
    trace.record_mmio(
        Some(xhci_target(CONFIG)),
        &MmioOp::Write { size: 4, value: 1 },
        &mem,
    );
    trace.record_mmio(
        Some(xhci_target(CONFIG)),
        &MmioOp::Write { size: 4, value: 2 },
        &mem,
    );
    let summary = trace
        .summary_lines(bridgevm_hvf::xhci::XhciEventLifecycleStats::default())
        .join("\n");

    assert!(!trace
        .events
        .iter()
        .any(|event| event.contains("set_tr_dequeue_pointer")));
    assert!(summary.contains(
        "dci5_set_tr_dequeue_pointer count=1 last_slot=1 last_endpoint=5 last_raw_dequeue=0x8401 last_dequeue=0x8400 last_dcs=true"
    ));
    assert!(summary.contains("dci5_doorbell count=1 last_slot=1 last_target=0x5 last_value=0x5"));
    assert!(summary.contains(
        "dci5_transfer_ring_snapshot count=1 last_slot=1 last_target=0x5 last_dequeue=0x8400 trbs_read=4 nonzero_trbs=1 zero_type0_trbs=3 first_nonzero_index=0 first_zero_type0_index=1"
    ));
}
