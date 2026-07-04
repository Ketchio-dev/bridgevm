#[derive(Clone, Copy)]
pub(crate) enum Dci5InputContextField {
    Value(u64),
    Unreadable,
    Overflow,
}

#[derive(Clone, Copy)]
pub(crate) struct Dci5InputCaptureTrace {
    pub(crate) input_context: u64,
    pub(crate) add_context: Dci5InputContextField,
    pub(crate) dci5_context: Dci5InputContextField,
    pub(crate) raw_dequeue: Option<u64>,
    pub(crate) output_context: Option<u64>,
    pub(crate) dci5_output: Option<u64>,
    pub(crate) published: bool,
    pub(crate) reason: &'static str,
}

pub(crate) fn dci5_input_capture(trace: Dci5InputCaptureTrace) {
    if super::trace::bringup_enabled() {
        println!("{}", format_dci5_input_capture(trace));
    }
}

fn format_dci5_input_capture(trace: Dci5InputCaptureTrace) -> String {
    let dequeue = trace.raw_dequeue.map(|raw_dequeue| raw_dequeue & !0xf);
    let dcs = trace.raw_dequeue.map_or_else(
        || "unavailable".to_string(),
        |raw_dequeue| (raw_dequeue & 1 != 0).to_string(),
    );
    format!(
        "xHCI diagnostic source=xhci.dci5 action=input_context_capture input_context={input_context:#x} add_context={add_context} dci5_added={dci5_added} dci5_context={dci5_context} raw_dequeue={raw_dequeue} dequeue={dequeue} dcs={dcs} output_context={output_context} dci5_output={dci5_output} published={published} reason={reason}",
        input_context = trace.input_context,
        add_context = format_dci5_input_context_field(trace.add_context),
        dci5_added = dci5_added(trace.add_context),
        dci5_context = format_dci5_input_context_field(trace.dci5_context),
        raw_dequeue = format_optional_hex(trace.raw_dequeue),
        dequeue = format_optional_hex(dequeue),
        output_context = format_optional_hex(trace.output_context),
        dci5_output = format_optional_hex(trace.dci5_output),
        published = trace.published,
        reason = trace.reason
    )
}

fn dci5_added(add_context: Dci5InputContextField) -> bool {
    matches!(add_context, Dci5InputContextField::Value(add_context) if add_context & (1 << 5) != 0)
}

fn format_dci5_input_context_field(field: Dci5InputContextField) -> String {
    match field {
        Dci5InputContextField::Value(value) => format!("{value:#x}"),
        Dci5InputContextField::Unreadable => "unreadable".to_string(),
        Dci5InputContextField::Overflow => "overflow".to_string(),
    }
}

fn format_optional_hex(value: Option<u64>) -> String {
    value.map_or_else(|| "unavailable".to_string(), |value| format!("{value:#x}"))
}

#[cfg(test)]
pub(super) fn assert_dci5_input_capture_trace_format_includes_parseable_context_state() {
    let line = format_dci5_input_capture(Dci5InputCaptureTrace {
        input_context: 0x1000,
        add_context: Dci5InputContextField::Value(0x20),
        dci5_context: Dci5InputContextField::Value(0x10c0),
        raw_dequeue: Some(0x2401),
        output_context: Some(0x3000),
        dci5_output: Some(0x30a0),
        published: true,
        reason: "published",
    });

    for token in [
        "source=xhci.dci5",
        "action=input_context_capture",
        "input_context=0x1000",
        "add_context=0x20",
        "dci5_added=true",
        "dci5_context=0x10c0",
        "raw_dequeue=0x2401",
        "dequeue=0x2400",
        "dcs=true",
        "output_context=0x3000",
        "dci5_output=0x30a0",
        "published=true",
        "reason=published",
    ] {
        assert!(line.split_ascii_whitespace().any(|part| part == token));
    }
}
