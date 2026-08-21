//! Per-emission DCI5 correlation line: the exact TRB a pointer report rode,
//! its completion code and length, the event TRB written for it, and the
//! interrupter state right after the post. This is the B4 instrument the
//! 2026-08-20 batch demanded (docs/windows-arm/evidence/
//! b4-pointer-reliability-batch-20260820.md): the host emitted every report
//! yet the guest input stack saw none, so the next separator is whether the
//! event/interrupt state for the button report differs from the move report
//! that precedes it. Gated by its own env so a live batch can carry it
//! without the full bringup firehose.

use super::event::{IMAN_INTERRUPT_ENABLE, IMAN_INTERRUPT_PENDING};
use super::pointer_input_report::{PointerInputReport, PointerInputReportKind};
use super::XhciController;

#[derive(Clone, Copy)]
pub(crate) struct Dci5EmissionTrace {
    pub(crate) kind: PointerInputReportKind,
    pub(crate) report: [u8; 6],
    pub(crate) trb_gpa: u64,
    pub(crate) transfer_length: u32,
    pub(crate) written_length: u32,
    pub(crate) completion_code: u32,
    pub(crate) interrupter: usize,
    pub(crate) event_gpa: u64,
    pub(crate) event_status: u32,
    pub(crate) event_control: u32,
    pub(crate) iman_ip: bool,
    pub(crate) iman_ie: bool,
    pub(crate) event_handler_busy: bool,
    pub(crate) erdp: u64,
}

pub(crate) fn enabled() -> bool {
    super::trace::bringup_enabled()
        || matches!(
            std::env::var("BRIDGEVM_TRACE_DCI5_EMISSION")
                .ok()
                .as_deref(),
            Some("1") | Some("true") | Some("TRUE") | Some("yes") | Some("YES")
        )
}

impl XhciController {
    // Captured after the post so the line shows the interrupter state the
    // guest will actually observe for this event.
    pub(super) fn trace_dci5_report_emitted(
        &self,
        report: PointerInputReport,
        trb_gpa: u64,
        transfer_length: u32,
        written_length: u32,
        completion_code: u32,
    ) {
        if !enabled() {
            return;
        }
        let stats = self.event_lifecycle_stats();
        let interrupter = stats.last_event_interrupter;
        let (iman_ip, iman_ie, event_handler_busy, erdp) = self
            .interrupters
            .get(interrupter)
            .map_or((false, false, false, 0), |it| {
                (
                    it.iman & IMAN_INTERRUPT_PENDING != 0,
                    it.iman & IMAN_INTERRUPT_ENABLE != 0,
                    it.event_handler_busy,
                    it.erdp,
                )
            });
        println!(
            "{}",
            format_dci5_report_emitted(Dci5EmissionTrace {
                kind: report.kind(),
                report: report.bytes(),
                trb_gpa,
                transfer_length,
                written_length,
                completion_code,
                interrupter,
                event_gpa: stats.last_event_gpa,
                event_status: stats.last_event_status,
                event_control: stats.last_event_control,
                iman_ip,
                iman_ie,
                event_handler_busy,
                erdp,
            })
        );
    }
}

fn kind_label(kind: PointerInputReportKind) -> &'static str {
    match kind {
        PointerInputReportKind::Move => "move",
        PointerInputReportKind::Button => "button",
        PointerInputReportKind::Release => "release",
        PointerInputReportKind::Wheel => "wheel",
    }
}

fn format_dci5_report_emitted(trace: Dci5EmissionTrace) -> String {
    let report = trace
        .report
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .join("");
    format!(
        "xHCI dci5 report emitted kind={kind} report={report} trb_gpa={trb_gpa:#x} transfer_length={transfer_length} written_length={written_length} completion_code={completion_code} interrupter={interrupter} event_gpa={event_gpa:#x} event_status={event_status:#010x} event_control={event_control:#010x} iman_ip={iman_ip} iman_ie={iman_ie} event_handler_busy={event_handler_busy} erdp={erdp:#x}",
        kind = kind_label(trace.kind),
        trb_gpa = trace.trb_gpa,
        transfer_length = trace.transfer_length,
        written_length = trace.written_length,
        completion_code = trace.completion_code,
        interrupter = trace.interrupter,
        event_gpa = trace.event_gpa,
        event_status = trace.event_status,
        event_control = trace.event_control,
        iman_ip = trace.iman_ip,
        iman_ie = trace.iman_ie,
        event_handler_busy = trace.event_handler_busy,
        erdp = trace.erdp,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dci5_emission_trace_format_includes_parseable_state() {
        let line = format_dci5_report_emitted(Dci5EmissionTrace {
            kind: PointerInputReportKind::Button,
            report: [0x01, 0x26, 0x57, 0x40, 0x51, 0x00],
            trb_gpa: 0x2400,
            transfer_length: 6,
            written_length: 6,
            completion_code: 1,
            interrupter: 1,
            event_gpa: 0x5010,
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
            "transfer_length=6",
            "written_length=6",
            "completion_code=1",
            "interrupter=1",
            "event_gpa=0x5010",
            "event_status=0x01000006",
            "event_control=0x01008401",
            "iman_ip=true",
            "iman_ie=true",
            "event_handler_busy=true",
            "erdp=0x5000",
        ] {
            assert!(
                line.split_ascii_whitespace().any(|part| part == token),
                "missing token {token} in: {line}"
            );
        }
    }
}
