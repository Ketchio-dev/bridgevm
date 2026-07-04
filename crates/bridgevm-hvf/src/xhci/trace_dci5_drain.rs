#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Dci5DrainBlockedTrace<'a> {
    pub(crate) reason: &'a str,
    pub(crate) policy: &'a str,
    pub(crate) dequeue: u64,
    pub(crate) ring_base: u64,
    pub(crate) dcs: bool,
    pub(crate) trb_gpa: Option<u64>,
    pub(crate) trb_type: Option<u32>,
    pub(crate) trb_cycle: Option<bool>,
    pub(crate) trb_parameter: Option<u64>,
    pub(crate) trb_status: Option<u32>,
    pub(crate) trb_control: Option<u32>,
    pub(crate) queued_reports: u64,
    pub(crate) emitted_move_reports: u64,
    pub(crate) emitted_button_reports: u64,
    pub(crate) emitted_release_reports: u64,
}

pub(crate) fn dci5_drain_blocked(trace: Dci5DrainBlockedTrace<'_>) {
    if super::trace::bringup_enabled() {
        println!("{}", format_dci5_drain_blocked(trace));
    }
}

fn format_dci5_drain_blocked(trace: Dci5DrainBlockedTrace<'_>) -> String {
    format!(
        "xHCI pointer-input DCI5 drain blocked reason={reason} policy={policy} dequeue={dequeue:#x} ring_base={ring_base:#x} dcs={dcs} trb_gpa={trb_gpa} trb_type={trb_type} trb_cycle={trb_cycle} trb_parameter={trb_parameter} trb_status={trb_status} trb_control={trb_control} queued_reports={queued_reports} emitted_move_reports={emitted_move_reports} emitted_button_reports={emitted_button_reports} emitted_release_reports={emitted_release_reports}",
        reason = trace.reason,
        policy = trace.policy,
        dequeue = trace.dequeue,
        ring_base = trace.ring_base,
        dcs = trace.dcs,
        trb_gpa = format_optional_u64(trace.trb_gpa),
        trb_type = format_optional_u32(trace.trb_type),
        trb_cycle = format_optional_bool(trace.trb_cycle),
        trb_parameter = format_optional_u64(trace.trb_parameter),
        trb_status = format_optional_u32(trace.trb_status),
        trb_control = format_optional_u32(trace.trb_control),
        queued_reports = trace.queued_reports,
        emitted_move_reports = trace.emitted_move_reports,
        emitted_button_reports = trace.emitted_button_reports,
        emitted_release_reports = trace.emitted_release_reports
    )
}

fn format_optional_u64(value: Option<u64>) -> String {
    value.map_or_else(|| "none".to_string(), |value| format!("{value:#x}"))
}

fn format_optional_u32(value: Option<u32>) -> String {
    value.map_or_else(|| "none".to_string(), |value| format!("{value:#x}"))
}

fn format_optional_bool(value: Option<bool>) -> &'static str {
    match value {
        Some(true) => "true",
        Some(false) => "false",
        None => "none",
    }
}

#[cfg(test)]
pub(crate) fn assert_dci5_drain_blocked_trace_format_includes_parseable_state() {
    let trace = Dci5DrainBlockedTrace {
        reason: "no_dci5_endpoint",
        policy: "queued_pointer_drain",
        dequeue: 0,
        ring_base: 0x2400,
        dcs: true,
        trb_gpa: Some(0x2400),
        trb_type: Some(1),
        trb_cycle: Some(false),
        trb_parameter: Some(0x8800),
        trb_status: Some(0x50000),
        trb_control: Some(0x405),
        queued_reports: 7,
        emitted_move_reports: 2,
        emitted_button_reports: 1,
        emitted_release_reports: 1,
    };

    let line = format_dci5_drain_blocked(trace);

    assert_eq!(
        line,
        "xHCI pointer-input DCI5 drain blocked reason=no_dci5_endpoint policy=queued_pointer_drain dequeue=0x0 ring_base=0x2400 dcs=true trb_gpa=0x2400 trb_type=0x1 trb_cycle=false trb_parameter=0x8800 trb_status=0x50000 trb_control=0x405 queued_reports=7 emitted_move_reports=2 emitted_button_reports=1 emitted_release_reports=1"
    );
    for token in [
        "reason=no_dci5_endpoint",
        "policy=queued_pointer_drain",
        "dequeue=0x0",
        "ring_base=0x2400",
        "dcs=true",
        "trb_gpa=0x2400",
        "trb_type=0x1",
        "trb_cycle=false",
        "trb_parameter=0x8800",
        "trb_status=0x50000",
        "trb_control=0x405",
        "queued_reports=7",
        "emitted_move_reports=2",
        "emitted_button_reports=1",
        "emitted_release_reports=1",
    ] {
        assert!(line.split_ascii_whitespace().any(|part| part == token));
    }
}
