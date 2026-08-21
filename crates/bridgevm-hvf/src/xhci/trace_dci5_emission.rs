//! Per-emission DCI5 correlation line for the report buffer, TD completion,
//! event TRB and interrupter state. Gated separately from bringup tracing.

use super::event::{IMAN_INTERRUPT_ENABLE, IMAN_INTERRUPT_PENDING};
use super::interrupt_trb::InterruptTransferTrb;
use super::pointer_input_report::PointerInputReport;
use super::trace_dci5_emission_format::{format, Dci5EmissionTrace};
use super::XhciController;
use std::sync::OnceLock;
use std::time::Instant;

pub(crate) fn enabled() -> bool {
    super::trace::bringup_enabled()
        || matches!(
            std::env::var("BRIDGEVM_TRACE_DCI5_EMISSION")
                .ok()
                .as_deref(),
            Some("1") | Some("true") | Some("TRUE") | Some("yes") | Some("YES")
        )
}

pub(super) fn trace_context(
    report: PointerInputReport,
    trb: &InterruptTransferTrb,
    td_end_gpa: u64,
    transfer_length: u32,
    written_length: u32,
    completion_code: u32,
) -> Dci5EmissionTrace {
    Dci5EmissionTrace {
        kind: report.kind(),
        report: report.bytes(),
        trb_gpa: trb.gpa,
        report_buffer_gpa: trb.parameter,
        td_end_gpa,
        transfer_length,
        written_length,
        completion_code,
        interrupter: 0,
        event_gpa: 0,
        event_parameter: 0,
        event_status: 0,
        event_control: 0,
        iman_ip: false,
        iman_ie: false,
        event_handler_busy: false,
        erdp: 0,
    }
}

impl XhciController {
    pub(super) fn trace_dci5_report_emitted(&self, mut trace: Dci5EmissionTrace) {
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
        trace.interrupter = interrupter;
        trace.event_gpa = stats.last_event_gpa;
        trace.event_parameter = stats.last_event_parameter;
        trace.event_status = stats.last_event_status;
        trace.event_control = stats.last_event_control;
        trace.iman_ip = iman_ip;
        trace.iman_ie = iman_ie;
        trace.event_handler_busy = event_handler_busy;
        trace.erdp = erdp;
        static START: OnceLock<Instant> = OnceLock::new();
        let elapsed_ms = START.get_or_init(Instant::now).elapsed().as_millis();
        println!("{} host_elapsed_ms={elapsed_ms}", format(trace));
    }
}
