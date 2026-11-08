use super::*;
use std::io::Write;

#[test]
fn fsevents_reports_an_in_place_edit_below_the_root() {
    let dir = std::env::temp_dir().join(format!("bvagent-fsevents-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("sub")).unwrap();
    std::fs::write(dir.join("sub/a.txt"), b"one").unwrap();
    let (stream, changes) = FsEventStream::start(&dir).expect("FSEvents on a local temp folder");
    std::thread::sleep(Duration::from_millis(500));
    let before = changes.load(Ordering::Acquire);
    // The same size and the same folder entries: only the content changes.
    let mut file = std::fs::OpenOptions::new().write(true).open(dir.join("sub/a.txt")).unwrap();
    file.write_all(b"two").unwrap();
    drop(file);
    let deadline = Instant::now() + Duration::from_secs(10);
    while changes.load(Ordering::Acquire) == before && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(changes.load(Ordering::Acquire) > before, "no FSEvents delivery for a content edit");
    drop(stream);
    let _ = std::fs::remove_dir_all(&dir);
}
