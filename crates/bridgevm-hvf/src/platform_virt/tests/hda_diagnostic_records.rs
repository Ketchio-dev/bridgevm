//! The platform exposes the HDA diagnostic records and its host sink's.

use super::super::*;
use super::helpers::platform_with_devices;

#[test]
fn platform_reports_hda_diagnostics_then_the_host_sinks_records_across_resets() {
    let mut absent = platform_with_devices(VirtPlatformDeviceConfig::default());
    absent.set_hda_diagnostic_host_records(Box::new(|| vec!["unreachable".to_string()]));
    assert!(absent.hda_diagnostic_records().is_empty());

    let mut platform = platform_with_devices(VirtPlatformDeviceConfig {
        hda_present: true,
        ..VirtPlatformDeviceConfig::default()
    });
    let prefixes = |records: Vec<String>| -> Vec<String> {
        let prefix = |record: &String| record.split(": ").next().unwrap_or_default().to_string();
        records.iter().map(prefix).collect()
    };
    assert_eq!(
        prefixes(platform.hda_diagnostic_records()),
        ["hda poll", "hda stream", "hda repeat"]
    );
    let timing = "hda CoreAudio timing: callbacks=0".to_string();
    let host = timing.clone();
    platform.set_hda_diagnostic_host_records(Box::new(move || vec![host.clone()]));
    platform.reset().expect("a platform without a TPM backend resets");
    let records = platform.hda_diagnostic_records();
    assert_eq!(records.len(), 4);
    assert_eq!(records[3], timing);
}
