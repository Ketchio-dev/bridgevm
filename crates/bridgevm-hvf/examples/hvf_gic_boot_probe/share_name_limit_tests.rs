//! Guest listing names are untrusted. A key no macOS path can hold is refused
//! when the listing is parsed, so the unseen-name check, which hashes every
//! parent of a key while the platform lock is held, never sees one deeper
//! than 512 components. Without the limit one 200,000-component name kept that
//! check running past the 10 s bound below.

use super::*;
use std::sync::mpsc;
use std::time::Duration;

/// `work` on its own thread; None when it has not finished within `limit`.
fn finished_within<T: Send + 'static>(
    limit: Duration,
    work: impl FnOnce() -> T + Send + 'static,
) -> Option<T> {
    let (done, finished) = mpsc::channel();
    std::thread::spawn(move || done.send(work()));
    finished.recv_timeout(limit).ok()
}

#[test]
fn listing_with_a_200k_component_name_is_refused_within_a_bound() {
    let mut sync = ShareSync::new(512);
    sync.set_host_unseen(["big.bin".to_string()]);
    let deep = "d\\".repeat(200_000);
    let listing = format!("{deep}x.txt|5|0|m1\nok.txt|2|0|m1\n");
    let actions = finished_within(Duration::from_secs(10), move || {
        sync.on_guest_listing_normalized(parse_ls(&listing))
    })
    .expect("a listing with one 200,000-component name took over 10 s");
    assert_eq!(
        actions,
        [SyncAction::Get {
            name: "ok.txt".into()
        }]
    );
}

#[test]
fn guest_keys_follow_the_macos_path_and_name_limits() {
    // Ten 99-byte folders, then a file name making the key `bytes` long.
    let key = |bytes: usize| format!("{}/", "d".repeat(99)).repeat(10) + &"f".repeat(bytes - 1000);
    assert_eq!(key(1023).len(), 1023);
    assert!(from_guest_rel(&key(1023)).is_some());
    assert!(from_guest_rel(&key(1024)).is_none());
    // NAME_MAX counts UTF-16 code units, as APFS and NTFS do, not bytes.
    for (name, fits) in [
        ("가".repeat(255), true),
        ("가".repeat(256), false),
        ("😀".repeat(127) + "a", true),
        ("😀".repeat(128), false),
    ] {
        let listed = format!("sub\\{name}");
        assert_eq!(
            from_guest_rel(&listed).is_some(),
            fits,
            "{} bytes",
            name.len()
        );
    }
}
