//! A guest file fetched to the Mac keeps the modification time the guest listed.

use super::*;
use crate::agent_console::agent_console_tests::tests::harness;
use crate::agent_console::share::share_host_write::get_test_fixture::fetched;

#[test]
fn a_fetched_guest_file_keeps_its_listed_modification_time() {
    let dir = std::env::temp_dir().join(format!("bvagent-get-mtime-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut h = harness();
    h.share = Some(ShareState::new(ShareSync::new(1024), dir.clone(), "C:\\share".into(), Duration::from_secs(1)));
    let entry = |name: &str, mtime: &str| LsEntry { name: name.into(), size: 3, is_dir: false, mtime: mtime.into() };
    let engine = &mut h.share.as_mut().unwrap().engine;
    let actions = engine.on_guest_listing_normalized([
        entry("dated.txt", "2026-11-08T00:05:12.1234567Z"),
        entry("undated.txt", "not a time"),
    ]);
    assert_eq!(actions.len(), 2);
    let ms = |time: std::time::SystemTime| time.duration_since(std::time::UNIX_EPOCH).unwrap().as_millis();
    let before = ms(std::time::SystemTime::now());
    fetched(&mut h, "dated.txt", b"abc");
    fetched(&mut h, "undated.txt", b"abc");
    let modified = |name: &str| ms(std::fs::metadata(dir.join(name)).unwrap().modified().unwrap());
    assert_eq!(modified("dated.txt"), 1_794_096_312_123);
    // A time the host cannot read leaves the write's own time.
    assert!(modified("undated.txt") + 1_000 >= before);
    let _ = std::fs::remove_dir_all(&dir);
}
