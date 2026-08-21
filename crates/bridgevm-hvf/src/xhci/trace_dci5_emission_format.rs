use super::pointer_input_report::PointerInputReportKind;

#[derive(Clone, Copy)]
pub(crate) struct Dci5EmissionTrace {
    pub(crate) kind: PointerInputReportKind,
    pub(crate) report: [u8; 6],
    pub(crate) trb_gpa: u64,
    pub(crate) report_buffer_gpa: u64,
    pub(crate) td_end_gpa: u64,
    pub(crate) transfer_length: u32,
    pub(crate) written_length: u32,
    pub(crate) completion_code: u32,
    pub(crate) interrupter: usize,
    pub(crate) event_gpa: u64,
    pub(crate) event_parameter: u64,
    pub(crate) event_status: u32,
    pub(crate) event_control: u32,
    pub(crate) iman_ip: bool,
    pub(crate) iman_ie: bool,
    pub(crate) event_handler_busy: bool,
    pub(crate) erdp: u64,
}

fn kind_label(kind: PointerInputReportKind) -> &'static str {
    match kind {
        PointerInputReportKind::Move => "move",
        PointerInputReportKind::Button => "button",
        PointerInputReportKind::Release => "release",
        PointerInputReportKind::Wheel => "wheel",
    }
}

pub(crate) fn format(trace: Dci5EmissionTrace) -> String {
    let report = trace
        .report
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!(
        "xHCI dci5 report emitted kind={kind} report={report} trb_gpa={trb_gpa:#x} report_buffer_gpa={report_buffer_gpa:#x} td_end_gpa={td_end_gpa:#x} transfer_length={transfer_length} written_length={written_length} completion_code={completion_code} interrupter={interrupter} event_gpa={event_gpa:#x} event_parameter={event_parameter:#x} event_status={event_status:#010x} event_control={event_control:#010x} iman_ip={iman_ip} iman_ie={iman_ie} event_handler_busy={event_handler_busy} erdp={erdp:#x}",
        kind = kind_label(trace.kind), trb_gpa = trace.trb_gpa,
        report_buffer_gpa = trace.report_buffer_gpa, td_end_gpa = trace.td_end_gpa,
        transfer_length = trace.transfer_length, written_length = trace.written_length,
        completion_code = trace.completion_code, interrupter = trace.interrupter,
        event_gpa = trace.event_gpa, event_parameter = trace.event_parameter,
        event_status = trace.event_status, event_control = trace.event_control,
        iman_ip = trace.iman_ip, iman_ie = trace.iman_ie,
        event_handler_busy = trace.event_handler_busy, erdp = trace.erdp,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn includes_parseable_buffer_and_event_state() {
        let line = format(Dci5EmissionTrace {
            kind: PointerInputReportKind::Button,
            report: [1, 0x26, 0x57, 0x40, 0x51, 0],
            trb_gpa: 0x2400,
            report_buffer_gpa: 0x2800,
            td_end_gpa: 0x2410,
            transfer_length: 6,
            written_length: 6,
            completion_code: 1,
            interrupter: 1,
            event_gpa: 0x5010,
            event_parameter: 0xdead_beef,
            event_status: 0x0100_0006,
            event_control: 0x0100_8401,
            iman_ip: true,
            iman_ie: true,
            event_handler_busy: true,
            erdp: 0x5000,
        });
        for token in [
            "kind=button",
            "report=012657405100",
            "trb_gpa=0x2400",
            "report_buffer_gpa=0x2800",
            "td_end_gpa=0x2410",
            "transfer_length=6",
            "written_length=6",
            "completion_code=1",
            "interrupter=1",
            "event_gpa=0x5010",
            "event_parameter=0xdeadbeef",
            "event_status=0x01000006",
            "event_control=0x01008401",
            "iman_ip=true",
            "iman_ie=true",
            "event_handler_busy=true",
            "erdp=0x5000",
        ] {
            assert!(
                line.split_ascii_whitespace().any(|part| part == token),
                "missing {token}: {line}"
            );
        }
    }
}
