//! Decode both bounded firmware rings kept in the shared result page.

#[path = "report_bs_trace.rs"]
mod bs_trace;
#[path = "report_start_failure.rs"]
mod start_failure;

pub(super) fn write(ram: &[u8]) {
    start_failure::write(ram);
    bs_trace::write(ram);
}
