//! The platform prints its host's teardown records once, as it drops, after
//! the HDA controller's records.

use super::super::*;
use super::helpers::platform_with_devices;
use std::sync::{Arc, Mutex};

type Calls = Arc<Mutex<Vec<&'static str>>>;

fn noting(calls: &Calls, name: &'static str) -> Box<dyn Fn() -> Vec<String> + Send> {
    let calls = Arc::clone(calls);
    Box::new(move || {
        calls.lock().unwrap().push(name);
        Vec::new()
    })
}

#[test]
fn the_platform_reads_its_teardown_records_once_as_it_drops_after_the_hda_records() {
    let calls = Calls::default();
    let mut platform = platform_with_devices(VirtPlatformDeviceConfig {
        hda_present: true,
        ..VirtPlatformDeviceConfig::default()
    });
    platform.set_hda_diagnostic_host_records(noting(&calls, "hda"));
    platform.set_teardown_host_records(noting(&calls, "teardown"));
    platform.reset().expect("reset without a TPM");
    assert!(
        calls.lock().unwrap().is_empty(),
        "nothing is read before the drop"
    );
    drop(platform);
    assert_eq!(*calls.lock().unwrap(), ["hda", "teardown"]);
}

#[test]
fn the_records_are_what_the_host_returns_and_none_without_a_host() {
    let mut platform = platform_with_devices(VirtPlatformDeviceConfig::default());
    assert!(platform.teardown_host_records().is_empty());
    let record = "ramfb display timing: ticks=0".to_string();
    let host = record.clone();
    platform.set_teardown_host_records(Box::new(move || vec![host.clone()]));
    platform.reset().expect("reset without a TPM");
    assert_eq!(platform.teardown_host_records(), [record]);
}

#[test]
fn the_teardown_records_are_the_platforms_last_field_so_they_drop_last() {
    let source = include_str!("../machine_assembly.rs");
    let fields = &source[source.find("pub struct VirtPlatform {").unwrap()..];
    let fields = &fields[..fields.find("\n}").unwrap()];
    assert!(fields.ends_with("\n    pub(crate) teardown_records: TeardownRecords,"));
}
