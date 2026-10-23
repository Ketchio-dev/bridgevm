//! The controller prints its diagnostic records as it is dropped
//! (diagnostics_teardown.rs): only with host records installed, once, and
//! before its PCM sink drops and prints its own teardown records.

use super::super::*;
use std::path::Path;
use std::sync::{Arc, Mutex};

type Events = Arc<Mutex<Vec<&'static str>>>;

/// A sink that notes when it is dropped, where a real sink prints its records.
struct DropNotingSink(Events);

impl HdaPcmSink for DropNotingSink {
    fn write_pcm(&mut self, _samples: &[u8], _rate: u32, _channels: u8, _bits: u8) {}
}

impl Drop for DropNotingSink {
    fn drop(&mut self) {
        self.0.lock().unwrap().push("sink dropped");
    }
}

/// A controller with that sink and host records that note each read.
fn controller(events: &Events) -> HdaController {
    let mut ctrl = HdaController::with_pcm_output_path::<&Path>(None);
    ctrl.set_pcm_sink(Some(Box::new(DropNotingSink(Arc::clone(events)))));
    let read = Arc::clone(events);
    ctrl.set_diagnostic_host_records(Some(Box::new(move || {
        read.lock().unwrap().push("host records read");
        vec!["hda CoreAudio timing: callbacks=0".to_string()]
    })));
    ctrl
}

#[test]
fn without_host_records_the_controller_prints_nothing_as_it_is_dropped() {
    let mut ctrl = HdaController::with_pcm_output_path::<&Path>(None);
    assert!(ctrl.teardown_records().is_empty());
    let events = Events::default();
    ctrl.set_pcm_sink(Some(Box::new(DropNotingSink(Arc::clone(&events)))));
    assert!(ctrl.teardown_records().is_empty());
    drop(ctrl);
    assert_eq!(*events.lock().unwrap(), ["sink dropped"]);
}

#[test]
fn host_records_make_every_record_print_before_the_pcm_sink_drops() {
    let events = Events::default();
    let ctrl = controller(&events);
    let records = ctrl.teardown_records();
    let prefixes: Vec<&str> = records
        .iter()
        .map(|r| r.split(": ").next().unwrap())
        .collect();
    let printed = [
        "hda poll",
        "hda stream",
        "hda repeat",
        "hda CoreAudio timing",
    ];
    assert_eq!(prefixes, printed);
    assert_eq!(records, ctrl.diagnostic_records());
    events.lock().unwrap().clear();
    drop(ctrl);
    assert_eq!(
        *events.lock().unwrap(),
        ["host records read", "sink dropped"]
    );
}

#[test]
fn resets_keep_the_host_records_so_a_replaced_controller_prints_nothing() {
    let events = Events::default();
    let mut ctrl = controller(&events);
    ctrl.controller_reset();
    ctrl.reset_runtime_state();
    assert!(
        events.lock().unwrap().is_empty(),
        "{:?}",
        events.lock().unwrap()
    );
    drop(ctrl);
    assert_eq!(
        *events.lock().unwrap(),
        ["host records read", "sink dropped"]
    );
}
