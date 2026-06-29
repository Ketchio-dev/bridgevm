pub(crate) struct Dci3DrainBlockedTrace<'a> {
    pub(crate) reason: &'a str,
    pub(crate) policy: &'a str,
    pub(crate) dequeue: u64,
    pub(crate) ring_base: u64,
    pub(crate) dcs: bool,
    pub(crate) two_entry_rearm: bool,
    pub(crate) last_dequeue: u64,
    pub(crate) last_dcs: bool,
    pub(crate) last_ring_base: u64,
    pub(crate) last_ring_dcs: bool,
    pub(crate) queued_reports: u64,
    pub(crate) emitted_key_reports: u64,
    pub(crate) emitted_release_reports: u64,
}

pub(crate) fn dci3_drain_blocked(trace: Dci3DrainBlockedTrace<'_>) {
    if super::trace::bringup_enabled() {
        println!("{}", format_dci3_drain_blocked(trace));
    }
}

fn format_dci3_drain_blocked(trace: Dci3DrainBlockedTrace<'_>) -> String {
    format!(
        "xHCI setup-input DCI3 drain blocked reason={reason} policy={policy} dequeue={dequeue:#x} ring_base={ring_base:#x} dcs={dcs} two_entry_rearm={two_entry_rearm} last_dequeue={last_dequeue:#x} last_dcs={last_dcs} last_ring_base={last_ring_base:#x} last_ring_dcs={last_ring_dcs} queued_reports={queued_reports} emitted_key_reports={emitted_key_reports} emitted_release_reports={emitted_release_reports}",
        reason = trace.reason,
        policy = trace.policy,
        dequeue = trace.dequeue,
        ring_base = trace.ring_base,
        dcs = trace.dcs,
        two_entry_rearm = trace.two_entry_rearm,
        last_dequeue = trace.last_dequeue,
        last_dcs = trace.last_dcs,
        last_ring_base = trace.last_ring_base,
        last_ring_dcs = trace.last_ring_dcs,
        queued_reports = trace.queued_reports,
        emitted_key_reports = trace.emitted_key_reports,
        emitted_release_reports = trace.emitted_release_reports
    )
}

#[cfg(test)]
pub(super) fn assert_dci3_drain_blocked_trace_format_includes_parseable_state() {
    let trace = Dci3DrainBlockedTrace {
        reason: "no_dci3_endpoint",
        policy: "reusable_queue_drain",
        dequeue: 0,
        ring_base: 0,
        dcs: false,
        two_entry_rearm: false,
        last_dequeue: 0x13ef_b8ac0,
        last_dcs: true,
        last_ring_base: 0x13ef_b8a80,
        last_ring_dcs: true,
        queued_reports: 18,
        emitted_key_reports: 1,
        emitted_release_reports: 1,
    };

    let line = format_dci3_drain_blocked(trace);

    assert_eq!(
        line,
        "xHCI setup-input DCI3 drain blocked reason=no_dci3_endpoint policy=reusable_queue_drain dequeue=0x0 ring_base=0x0 dcs=false two_entry_rearm=false last_dequeue=0x13efb8ac0 last_dcs=true last_ring_base=0x13efb8a80 last_ring_dcs=true queued_reports=18 emitted_key_reports=1 emitted_release_reports=1"
    );
    for token in [
        "reason=no_dci3_endpoint",
        "policy=reusable_queue_drain",
        "dequeue=0x0",
        "ring_base=0x0",
        "dcs=false",
        "two_entry_rearm=false",
        "last_dequeue=0x13efb8ac0",
        "last_dcs=true",
        "last_ring_base=0x13efb8a80",
        "last_ring_dcs=true",
        "queued_reports=18",
        "emitted_key_reports=1",
        "emitted_release_reports=1",
    ] {
        assert!(line.split_ascii_whitespace().any(|part| part == token));
    }
}
