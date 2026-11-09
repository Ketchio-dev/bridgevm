use super::*;

fn folder(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("bvagent-stage-{label}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn answer(writer: &mut StageWriter) -> FromWriter {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(answer) = writer.poll() {
            return answer;
        }
        assert!(Instant::now() < deadline, "no answer");
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn chunks_are_hashed_written_and_published_whole() {
    let dir = folder("publish");
    let destination = Destination::open(&dir, "sub/out.bin", true).unwrap();
    let mut writer = StageWriter::start(destination, 7, None).unwrap();
    writer.chunk(base64_encode(b"abcd"), 4);
    writer.chunk(base64_encode(b"efg"), 3);
    assert!(matches!(answer(&mut writer), FromWriter::Hashed(hash) if hash == share_sync::fnv1a64(b"abcdefg")));
    assert_eq!(writer.backlog, 0);
    assert!(!dir.join("sub/out.bin").exists(), "visible before the publish");
    writer.publish(None);
    assert!(matches!(answer(&mut writer), FromWriter::Published(Ok(Some(_)))));
    assert_eq!(std::fs::read(dir.join("sub/out.bin")).unwrap(), b"abcdefg");
    assert_eq!(std::fs::read_dir(dir.join("sub")).unwrap().count(), 1, "a temporary is left");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_dropped_or_failed_writer_leaves_no_temporary() {
    let dir = folder("drop");
    let writer = StageWriter::start(Destination::open(&dir, "a.bin", true).unwrap(), 9, None).unwrap();
    drop(writer);
    let mut writer = StageWriter::start(Destination::open(&dir, "b.bin", true).unwrap(), 2, None).unwrap();
    writer.chunk("not base64!".into(), 2);
    assert!(matches!(answer(&mut writer), FromWriter::Failed(e) if e.kind() == std::io::ErrorKind::InvalidData));
    let deadline = Instant::now() + Duration::from_secs(10);
    while std::fs::read_dir(&dir).unwrap().count() > 0 {
        assert!(Instant::now() < deadline, "a temporary was left");
        std::thread::sleep(Duration::from_millis(1));
    }
    let _ = std::fs::remove_dir_all(&dir);
}
