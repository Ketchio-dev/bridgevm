#[derive(Clone, Copy)]
pub(crate) struct EventRingTrace {
    pub(crate) segment_base: u64,
    pub(crate) segment_trbs: u32,
    pub(crate) enqueue: u32,
    pub(crate) cycle: bool,
    pub(crate) interrupter: usize,
}

#[derive(Clone, Copy)]
pub(crate) struct EventPostTrace {
    pub(crate) ring: EventRingTrace,
    pub(crate) parameter: u64,
    pub(crate) status: u32,
    pub(crate) control: u32,
    pub(crate) event_gpa: u64,
}

#[derive(Clone, Copy)]
pub(crate) struct EventPostStateTrace {
    pub(crate) event_handler_busy: bool,
    pub(crate) iman_interrupt_pending: bool,
    pub(crate) usb_sts_eint: bool,
}

pub(crate) struct SetupInputReportEmittedTrace<'a> {
    pub(crate) action: &'a str,
    pub(crate) usage: u8,
    pub(crate) report_kind: &'a str,
    pub(crate) report: [u8; 8],
    pub(crate) dci3_trb_gpa: u64,
    pub(crate) buffer_gpa: u64,
    pub(crate) emitted_key_reports: u64,
    pub(crate) emitted_release_reports: u64,
}

pub(crate) use super::trace_dci3_drain::{dci3_drain_blocked, Dci3DrainBlockedTrace};
pub(crate) use super::trace_dci3_input_capture::{
    dci3_input_capture, Dci3InputCaptureTrace, Dci3InputContextField,
};
#[cfg(test)]
pub(crate) use super::trace_dci5_drain::assert_dci5_drain_blocked_trace_format_includes_parseable_state;
pub(crate) use super::trace_dci5_drain::{dci5_drain_blocked, Dci5DrainBlockedTrace};
pub(crate) use super::trace_dci5_input_capture::{
    dci5_input_capture, Dci5InputCaptureTrace, Dci5InputContextField,
};
pub(crate) use super::trace_host_controller_reset::host_controller_reset;

pub(crate) fn bringup_enabled() -> bool {
    matches!(
        std::env::var("BRIDGEVM_TRACE_XHCI_BRINGUP").ok().as_deref(),
        Some("1") | Some("true") | Some("TRUE") | Some("yes") | Some("YES")
    )
}

pub(crate) fn event_post_reject(reason: &str) {
    if bringup_enabled() {
        println!("XHCI event post result posted=false reason={reason}");
    }
}

pub(crate) fn event_post_reject_with_gpa(reason: &str, gpa: u64) {
    if bringup_enabled() {
        println!("XHCI event post result posted=false reason={reason} gpa={gpa:#x}");
    }
}

pub(crate) fn event_post_reject_with_ring(reason: &str, trace: EventRingTrace) {
    if bringup_enabled() {
        println!(
            "XHCI event post result posted=false reason={reason} segment_base={segment_base:#x} segment_trbs={segment_trbs} enqueue={enqueue} cycle={cycle} interrupter={interrupter}",
            segment_base = trace.segment_base,
            segment_trbs = trace.segment_trbs,
            enqueue = trace.enqueue,
            cycle = u32::from(trace.cycle),
            interrupter = trace.interrupter
        );
    }
}

pub(crate) fn event_post_reject_with_event(reason: &str, trace: EventPostTrace) {
    if bringup_enabled() {
        println!(
            "XHCI event post result posted=false reason={reason} parameter={parameter:#x} status={status:#010x} control={control:#010x} event_gpa={event_gpa:#x} segment_base={segment_base:#x} segment_trbs={segment_trbs} enqueue={enqueue} cycle={cycle} interrupter={interrupter}",
            parameter = trace.parameter,
            status = trace.status,
            control = trace.control,
            event_gpa = trace.event_gpa,
            segment_base = trace.ring.segment_base,
            segment_trbs = trace.ring.segment_trbs,
            enqueue = trace.ring.enqueue,
            cycle = u32::from(trace.ring.cycle),
            interrupter = trace.ring.interrupter
        );
    }
}

pub(crate) fn event_post_success(trace: EventPostTrace, state: EventPostStateTrace) {
    if bringup_enabled() {
        println!("{}", format_event_post_success(trace, state));
    }
}

pub(crate) fn erdp_ehb_consumed(erdp: u64, interrupter: usize, state: EventPostStateTrace) {
    if bringup_enabled() {
        println!("{}", format_erdp_ehb_consumed(erdp, interrupter, state));
    }
}

pub(crate) fn setup_input_action_queued(
    action: &str,
    usage: u8,
    key_report: [u8; 8],
    release_report: [u8; 8],
    queued_actions: u64,
    queued_reports: u64,
) {
    if bringup_enabled() {
        println!(
            "xHCI setup-input action queued action={action} usage=0x{usage:02x} key_report={} release_report={} queued_actions={queued_actions} queued_reports={queued_reports}",
            format_report(key_report),
            format_report(release_report)
        );
    }
}

pub(crate) fn setup_input_report_emitted(trace: SetupInputReportEmittedTrace<'_>) {
    if bringup_enabled() {
        println!(
            "xHCI setup-input report emitted action={action} usage=0x{usage:02x} report_kind={report_kind} report={} dci3_trb_gpa={dci3_trb_gpa:#x} buffer_gpa={buffer_gpa:#x} emitted_key_reports={emitted_key_reports} emitted_release_reports={emitted_release_reports}",
            format_report(trace.report),
            action = trace.action,
            usage = trace.usage,
            report_kind = trace.report_kind,
            dci3_trb_gpa = trace.dci3_trb_gpa,
            buffer_gpa = trace.buffer_gpa,
            emitted_key_reports = trace.emitted_key_reports,
            emitted_release_reports = trace.emitted_release_reports
        );
    }
}

pub(crate) fn ep0_handler_entered(transfer_ring: u64) {
    if bringup_enabled() {
        println!("XHCI EP0 handler entered transfer_ring={transfer_ring:#x}");
    }
}

pub(crate) fn ep0_trb(label: &str, gpa: u64, parameter: u64, status: u32, control: u32, ty: u32) {
    if bringup_enabled() {
        println!(
            "XHCI EP0 {label}_trb gpa={gpa:#x} parameter={parameter:#x} status={status:#010x} control={control:#010x} type={ty}"
        );
    }
}

pub(crate) fn ep0_setup_packet(
    bm_request_type: u8,
    request: u8,
    value: u16,
    index: u16,
    length: u16,
) {
    if bringup_enabled() {
        println!(
            "XHCI EP0 setup_packet bm_request_type={bm_request_type:#04x} request={request:#04x} value={value:#06x} index={index} length={length}"
        );
    }
}

pub(crate) fn ep0_descriptor_write_success(target_gpa: u64, len: usize) {
    if bringup_enabled() {
        println!("XHCI EP0 descriptor_write success target_gpa={target_gpa:#x} len={len}");
    }
}

pub(crate) fn ep0_post_event_request(parameter: u64, status: u32, control: u32) {
    if bringup_enabled() {
        println!(
            "XHCI EP0 post_event request parameter={parameter:#x} status={status:#010x} control={control:#010x}"
        );
    }
}

pub(crate) fn ep0_post_event_result(posted: bool) {
    if bringup_enabled() {
        println!("XHCI EP0 post_event result posted={posted}");
    }
}

pub(crate) fn ep0_reject(reason: &str) {
    if bringup_enabled() {
        println!("XHCI EP0 outcome posted=false reason={reason}");
    }
}

pub(crate) fn ep0_reject_with_gpa(reason: &str, gpa: u64) {
    if bringup_enabled() {
        println!("XHCI EP0 outcome posted=false reason={reason} gpa={gpa:#x}");
    }
}

pub(crate) fn ep0_reject_with_value(reason: &str, value: u32) {
    if bringup_enabled() {
        println!("XHCI EP0 outcome posted=false reason={reason} value={value:#x}");
    }
}

fn format_report(report: [u8; 8]) -> String {
    format!(
        "{:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x}",
        report[0], report[1], report[2], report[3], report[4], report[5], report[6], report[7]
    )
}

pub(super) fn format_event_post_success(
    trace: EventPostTrace,
    state: EventPostStateTrace,
) -> String {
    format!(
        "XHCI event post result posted=true parameter={parameter:#x} status={status:#010x} control={control:#010x} event_gpa={event_gpa:#x} segment_base={segment_base:#x} segment_trbs={segment_trbs} enqueue={enqueue} cycle={cycle} interrupter={interrupter} event_handler_busy={event_handler_busy} iman_interrupt_pending={iman_interrupt_pending} usb_sts_eint={usb_sts_eint}",
        parameter = trace.parameter,
        status = trace.status,
        control = trace.control,
        event_gpa = trace.event_gpa,
        segment_base = trace.ring.segment_base,
        segment_trbs = trace.ring.segment_trbs,
        enqueue = trace.ring.enqueue,
        cycle = u32::from(trace.ring.cycle),
        interrupter = trace.ring.interrupter,
        event_handler_busy = state.event_handler_busy,
        iman_interrupt_pending = state.iman_interrupt_pending,
        usb_sts_eint = state.usb_sts_eint
    )
}

pub(super) fn format_erdp_ehb_consumed(
    erdp: u64,
    interrupter: usize,
    state: EventPostStateTrace,
) -> String {
    format!(
        "XHCI ERDP EHB consumed erdp0={erdp:#x} interrupter={interrupter} event_handler_busy={event_handler_busy} iman_interrupt_pending={iman_interrupt_pending} usb_sts_eint={usb_sts_eint}",
        event_handler_busy = state.event_handler_busy,
        iman_interrupt_pending = state.iman_interrupt_pending,
        usb_sts_eint = state.usb_sts_eint
    )
}
