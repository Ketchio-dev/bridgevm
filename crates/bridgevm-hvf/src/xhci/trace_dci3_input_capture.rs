#[derive(Clone, Copy)]
pub(crate) enum Dci3InputContextField {
    Value(u64),
    Unreadable,
    Overflow,
}

#[derive(Clone, Copy)]
pub(crate) struct Dci3InputCaptureTrace {
    pub(crate) input_context: u64,
    pub(crate) add_context: Dci3InputContextField,
    pub(crate) dci3_context: Dci3InputContextField,
    pub(crate) raw_dequeue: Option<u64>,
    pub(crate) output_context: Option<u64>,
    pub(crate) dci3_output: Option<u64>,
    pub(crate) published: bool,
    pub(crate) reason: &'static str,
}

pub(crate) fn dci3_input_capture(trace: Dci3InputCaptureTrace) {
    if super::trace::bringup_enabled() {
        println!("{}", format_dci3_input_capture(trace));
    }
}

fn format_dci3_input_capture(trace: Dci3InputCaptureTrace) -> String {
    let dequeue = trace.raw_dequeue.map(|raw_dequeue| raw_dequeue & !0xf);
    let dcs = trace.raw_dequeue.map_or_else(
        || "unavailable".to_string(),
        |raw_dequeue| (raw_dequeue & 1 != 0).to_string(),
    );
    format!(
        "xHCI diagnostic source=xhci.dci3 action=input_context_capture input_context={input_context:#x} add_context={add_context} dci3_added={dci3_added} dci3_context={dci3_context} raw_dequeue={raw_dequeue} dequeue={dequeue} dcs={dcs} output_context={output_context} dci3_output={dci3_output} published={published} reason={reason}",
        input_context = trace.input_context,
        add_context = format_dci3_input_context_field(trace.add_context),
        dci3_added = dci3_added(trace.add_context),
        dci3_context = format_dci3_input_context_field(trace.dci3_context),
        raw_dequeue = format_optional_hex(trace.raw_dequeue),
        dequeue = format_optional_hex(dequeue),
        output_context = format_optional_hex(trace.output_context),
        dci3_output = format_optional_hex(trace.dci3_output),
        published = trace.published,
        reason = trace.reason
    )
}

fn dci3_added(add_context: Dci3InputContextField) -> bool {
    matches!(add_context, Dci3InputContextField::Value(add_context) if add_context & (1 << 3) != 0)
}

fn format_dci3_input_context_field(field: Dci3InputContextField) -> String {
    match field {
        Dci3InputContextField::Value(value) => format!("{value:#x}"),
        Dci3InputContextField::Unreadable => "unreadable".to_string(),
        Dci3InputContextField::Overflow => "overflow".to_string(),
    }
}

fn format_optional_hex(value: Option<u64>) -> String {
    value.map_or_else(|| "unavailable".to_string(), |value| format!("{value:#x}"))
}

#[cfg(test)]
pub(super) fn assert_dci3_input_capture_trace_format_includes_parseable_context_state() {
    let line = format_dci3_input_capture(Dci3InputCaptureTrace {
        input_context: 0x1000,
        add_context: Dci3InputContextField::Value(0x8),
        dci3_context: Dci3InputContextField::Value(0x1080),
        raw_dequeue: Some(0x2201),
        output_context: Some(0x3000),
        dci3_output: Some(0x3060),
        published: true,
        reason: "published",
    });

    for token in [
        "source=xhci.dci3",
        "action=input_context_capture",
        "input_context=0x1000",
        "add_context=0x8",
        "dci3_added=true",
        "dci3_context=0x1080",
        "raw_dequeue=0x2201",
        "dequeue=0x2200",
        "dcs=true",
        "output_context=0x3000",
        "dci3_output=0x3060",
        "published=true",
        "reason=published",
    ] {
        assert!(line.split_ascii_whitespace().any(|part| part == token));
    }
}
