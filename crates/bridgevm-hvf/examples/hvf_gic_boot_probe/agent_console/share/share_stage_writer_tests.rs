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
    writer.publish(None, None, false);
    assert!(matches!(answer(&mut writer), FromWriter::Published(Ok(Some(_)), true)));
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

#[test]
fn a_preferred_copy_is_not_published_over_a_mac_file_changed_since_the_first_scan() {
    // A large preferred guest copy (share_synced.rs): the Mac file is checked right before the
    // rename, after the copy was staged and synced.
    let dir = folder("mac-changed");
    let mac = dir.join("doc.bin");
    std::fs::write(&mac, b"synced").unwrap();
    let meta = std::fs::metadata(&mac).unwrap();
    let seen = (meta.len(), super::super::file_mtime_ms_from_meta(&meta).unwrap());
    let mut writer = StageWriter::start(Destination::open(&dir, "doc.bin", true).unwrap(), 5, None).unwrap();
    writer.chunk(base64_encode(b"guest"), 5);
    assert!(matches!(answer(&mut writer), FromWriter::Hashed(_)));
    std::fs::write(&mac, b"edited on the Mac").unwrap();
    writer.publish(None, Some(Some(seen)), false);
    assert!(matches!(answer(&mut writer), FromWriter::Published(Err(e), false) if super::super::host_destination::is_mac_changed(&e)));
    assert_eq!(std::fs::read(&mac).unwrap(), b"edited on the Mac");
    let conflicts: Vec<_> = std::fs::read_dir(&dir).unwrap().filter_map(Result::ok).filter(|e| e.file_name().to_string_lossy().contains("Windows copy")).collect();
    assert_eq!(conflicts.len(), 1);
    assert_eq!(std::fs::read(conflicts[0].path()).unwrap(), b"guest");
    // Unchanged, the same copy goes.
    let meta = std::fs::metadata(&mac).unwrap();
    let seen = (meta.len(), super::super::file_mtime_ms_from_meta(&meta).unwrap());
    let mut writer = StageWriter::start(Destination::open(&dir, "doc.bin", true).unwrap(), 5, None).unwrap();
    writer.chunk(base64_encode(b"guest"), 5);
    assert!(matches!(answer(&mut writer), FromWriter::Hashed(_)));
    writer.publish(None, Some(Some(seen)), false);
    assert!(matches!(answer(&mut writer), FromWriter::Published(Ok(_), true)));
    assert_eq!(std::fs::read(&mac).unwrap(), b"guest");
    let _ = std::fs::remove_dir_all(&dir);
}
