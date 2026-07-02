use super::trace::{
    format_erdp_ehb_consumed, format_event_post_success, EventPostStateTrace, EventPostTrace,
    EventRingTrace,
};

fn posted_event() -> EventPostTrace {
    EventPostTrace {
        ring: EventRingTrace {
            segment_base: 0x5000,
            segment_trbs: 2,
            enqueue: 1,
            cycle: true,
            interrupter: 1,
        },
        parameter: 0x1000,
        status: 0x0100_0000,
        control: 0x0100_8401,
        event_gpa: 0x5010,
    }
}

#[test]
fn event_post_success_trace_includes_live_interrupt_state() {
    let state = EventPostStateTrace {
        event_handler_busy: true,
        iman_interrupt_pending: true,
        usb_sts_eint: true,
    };
    let line = format_event_post_success(posted_event(), state);

    assert!(line.contains("posted=true"));
    assert!(line.contains("interrupter=1"));
    assert!(line.contains("segment_base=0x5000"));
    assert!(line.contains("enqueue=1"));
    assert!(line.contains("cycle=1"));
    assert!(line.contains("iman_interrupt_pending=true"));
    assert!(line.contains("usb_sts_eint=true"));
}

#[test]
fn erdp_ehb_trace_includes_cleared_interrupt_state() {
    let state = EventPostStateTrace {
        event_handler_busy: false,
        iman_interrupt_pending: false,
        usb_sts_eint: false,
    };
    let line = format_erdp_ehb_consumed(0x5010, 0, state);

    assert!(line.contains("XHCI ERDP EHB consumed"));
    assert!(line.contains("erdp0=0x5010"));
    assert!(line.contains("event_handler_busy=false"));
    assert!(line.contains("iman_interrupt_pending=false"));
    assert!(line.contains("usb_sts_eint=false"));
}

#[test]
fn dci3_drain_blocked_trace_format_includes_parseable_state() {
    super::trace_dci3_drain::assert_dci3_drain_blocked_trace_format_includes_parseable_state();
}

#[test]
fn dci3_input_capture_trace_format_includes_parseable_context_state() {
    super::trace_dci3_input_capture::assert_dci3_input_capture_trace_format_includes_parseable_context_state();
}

#[test]
fn mmio_trace_format_includes_parseable_access() {
    super::trace_mmio::assert_mmio_trace_format_includes_parseable_access();
}

#[test]
fn mmio_read_repeat_flush_summarizes_extra_repeats() {
    super::trace_mmio::assert_mmio_read_repeat_flush_summarizes_extra_repeats();
}
