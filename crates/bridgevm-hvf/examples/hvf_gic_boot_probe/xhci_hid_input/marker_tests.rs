use super::marker::{MarkerEnvError, ProbeMarker, MARKER_MAX_BYTES};

#[test]
fn marker_rejects_overlong_custom_value() {
    let marker = vec![b'a'; MARKER_MAX_BYTES + 1];

    let error = ProbeMarker::custom_for_test(&marker).unwrap_err();

    assert_eq!(
        error,
        MarkerEnvError::TooLong {
            len: MARKER_MAX_BYTES + 1,
            max: MARKER_MAX_BYTES,
        }
    );
}

#[test]
fn marker_rejects_empty_custom_value() {
    let error = ProbeMarker::custom_for_test(b"").unwrap_err();

    assert_eq!(error, MarkerEnvError::Empty);
}

#[test]
fn marker_uses_default_when_env_is_absent() {
    let marker = ProbeMarker::default_bytes(b"BdsDxe: starting Boot0001");

    assert_eq!(marker.as_bytes(), b"BdsDxe: starting Boot0001");
    assert_eq!(marker.source_name(), "default");
}

#[test]
fn marker_log_summary_redacts_custom_marker_content() {
    let marker = ProbeMarker::custom_for_test(b"secret-marker").unwrap();

    let summary = marker.log_summary().to_string();

    assert!(summary.contains("marker_source=custom"));
    assert!(summary.contains("marker_bytes=13"));
    assert!(summary.contains("marker_hash="));
    assert!(!summary.contains("secret-marker"));
}

#[test]
fn marker_rejection_summary_redacts_custom_marker_content() {
    let error = MarkerEnvError::TooLong {
        len: MARKER_MAX_BYTES + 1,
        max: MARKER_MAX_BYTES,
    };

    let summary = error.rejection_summary().to_string();

    assert!(summary.contains("marker_source=custom"));
    assert!(summary.contains("marker_bytes=97"));
    assert!(summary.contains("marker_hash=redacted"));
}

#[cfg(unix)]
#[test]
fn marker_env_rejects_non_unicode_value() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());
    let _guard = ENV_LOCK.lock().unwrap();
    let env_name = "BRIDGEVM_TEST_XHCI_MARKER_NONUNICODE";
    std::env::set_var(env_name, OsString::from_vec(vec![0xff]));

    let result = ProbeMarker::custom_from_env(env_name);

    std::env::remove_var(env_name);
    assert_eq!(result, Err(MarkerEnvError::NotUnicode { env_name }));
}
