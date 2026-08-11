use super::*;
use std::ffi::OsString;

#[test]
fn configuration_requires_an_absolute_path() {
    assert_eq!(configured_path(None), None);
    assert_eq!(configured_path(Some(OsString::from("relative"))), None);
    assert_eq!(
        configured_path(Some(OsString::from("/tmp/bridgevm-stop"))),
        Some(PathBuf::from("/tmp/bridgevm-stop"))
    );
}

#[test]
fn only_a_direct_regular_file_is_a_one_shot_request() {
    let dir = std::env::temp_dir().join(format!(
        "bridgevm-host-stop-{}-{}",
        std::process::id(),
        std::thread::current().name().unwrap_or("test")
    ));
    let request = dir.join("request");
    std::fs::create_dir_all(&dir).unwrap();
    let stop = HostDiagnosticStop {
        vcpu: 0,
        request_path: request.clone(),
        fired: Arc::new(AtomicBool::new(false)),
    };
    assert!(!stop.claim_request());
    std::fs::write(&request, b"stop\n").unwrap();
    assert!(stop.claim_request());
    assert!(stop.fired.load(Ordering::SeqCst));
    assert!(!stop.claim_request());
    std::fs::remove_file(&request).unwrap();
    std::fs::create_dir(&request).unwrap();
    assert!(!request_is_regular_file(&request));
    std::fs::remove_dir_all(&dir).unwrap();
}
