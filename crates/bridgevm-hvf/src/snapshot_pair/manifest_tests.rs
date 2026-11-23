use super::managed::LockedPair;
use super::tests::Scratch;
use super::*;
use std::fs::OpenOptions;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{symlink, OpenOptionsExt};
use std::sync::mpsc;
use std::time::Duration;

#[test]
fn a_manifest_round_trips_through_json() {
    let m = SnapshotManifest {
        format_version: SNAPSHOT_FORMAT_VERSION,
        vm_id: "windows-11".into(),
        disk_bytes: 68_719_476_736,
        disk_sha256: "a".repeat(64),
        vars_bytes: 67_108_864,
        vars_sha256: "b".repeat(64),
    };
    assert_eq!(SnapshotManifest::from_json(&m.to_json()).unwrap(), m);
}

#[test]
fn a_vm_id_with_quotes_does_not_break_the_manifest() {
    let m = SnapshotManifest {
        format_version: SNAPSHOT_FORMAT_VERSION,
        vm_id: r#"vm "quoted" \ backslash"#.into(),
        disk_bytes: 1,
        disk_sha256: "c".repeat(64),
        vars_bytes: 2,
        vars_sha256: "d".repeat(64),
    };
    assert_eq!(SnapshotManifest::from_json(&m.to_json()).unwrap(), m);
}

#[test]
fn malformed_manifests_cannot_authorize_replacement_or_restore() {
    for case in [
        "version-exponent",
        "missing-punctuation",
        "duplicate-field",
        "trailing-document",
    ] {
        let scratch = Scratch::new(&format!("manifest-admission-{case}"));
        let disk = scratch.write("disk", b"snapshot disk");
        let vars = scratch.write("vars", b"snapshot vars");
        let destination = scratch.path("snapshot");
        create_snapshot(&disk, &vars, &destination, "vm").unwrap();
        let text = fs::read_to_string(destination.join(MANIFEST_NAME)).unwrap();
        let malformed = match case {
            "version-exponent" => text.replace("\"format_version\": 1", "\"format_version\": 1e2"),
            "missing-punctuation" => text
                .replace("{\n", "")
                .replace("\n}\n", "\n")
                .replace(",\n", "\n"),
            "duplicate-field" => text.replacen('{', "{\"vm_id\":\"other\",", 1),
            "trailing-document" => text + "{}",
            _ => unreachable!(),
        };
        fs::write(destination.join(MANIFEST_NAME), &malformed).unwrap();
        let before = [DISK_NAME, VARS_NAME, MANIFEST_NAME]
            .map(|name| fs::read(destination.join(name)).unwrap());
        fs::write(&disk, b"keep live disk").unwrap();
        fs::write(&vars, b"keep live vars").unwrap();

        assert!(matches!(
            verify_snapshot(&destination),
            Err(SnapshotError::BadManifest(_))
        ));
        let replacement = create_snapshot(&disk, &vars, &destination, "new");
        let Err(SnapshotError::Io(error)) = replacement else {
            panic!("{case}: replacement accepted malformed snapshot: {replacement:?}");
        };
        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        assert!(error.to_string().contains("left intact"));
        assert!(!staging_path(&destination).exists());
        assert!(matches!(
            restore_snapshot(&destination, &disk, &vars),
            Err(SnapshotError::BadManifest(_))
        ));
        let after = [DISK_NAME, VARS_NAME, MANIFEST_NAME]
            .map(|name| fs::read(destination.join(name)).unwrap());
        assert_eq!(after, before, "{case}: malformed destination was changed");
        assert_eq!(fs::read(&disk).unwrap(), b"keep live disk");
        assert_eq!(fs::read(&vars).unwrap(), b"keep live vars");
        assert!(!fs::read_dir(&scratch.0).unwrap().any(|entry| entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".bridgevm-pair-")));
    }
}

fn manifest() -> SnapshotManifest {
    SnapshotManifest {
        format_version: SNAPSHOT_FORMAT_VERSION,
        vm_id: "vm".into(),
        disk_bytes: 10,
        disk_sha256: "a".repeat(64),
        vars_bytes: 20,
        vars_sha256: "b".repeat(64),
    }
}

fn assert_bad(text: &str) {
    assert!(
        matches!(
            SnapshotManifest::from_json(text),
            Err(SnapshotError::BadManifest(_))
        ),
        "accepted ambiguous or malformed manifest: {text:?}"
    );
}

#[test]
fn every_control_character_and_unicode_vm_id_round_trips() {
    let mut expected = manifest();
    expected.vm_id = (0u8..=31).map(char::from).collect::<String>() + "\"\\ 한글 🦀";
    assert_eq!(
        SnapshotManifest::from_json(&expected.to_json()).unwrap(),
        expected
    );
}

#[test]
fn standard_json_string_escapes_are_decoded() {
    let mut expected = manifest();
    let text = expected.to_json().replace(
        "\"vm_id\": \"vm\"",
        r#""vm_id": "\b\f\n\r\t\/\u0041\ud83e\udd80""#,
    );
    expected.vm_id = "\u{8}\u{c}\n\r\t/A🦀".into();
    assert_eq!(SnapshotManifest::from_json(&text).unwrap(), expected);
}

#[test]
fn malformed_and_trailing_documents_are_refused() {
    let text = manifest().to_json();
    for bad in [
        text.trim().trim_start_matches('{').to_string(),
        text.trim().trim_end_matches('}').to_string(),
        text.replace(",\n", "\n"),
        text.clone() + "garbage",
        text.clone() + "{}",
        format!("[{text}]"),
        format!(
            "[1,\"vm\",10,\"{}\",20,\"{}\"]",
            "a".repeat(64),
            "b".repeat(64)
        ),
    ] {
        assert_bad(&bad);
    }
}

#[test]
fn integer_fields_refuse_suffixes_and_non_integer_types() {
    let text = manifest().to_json();
    for (field, original) in [
        ("format_version", 1),
        ("disk_bytes", 10),
        ("vars_bytes", 20),
    ] {
        for value in [
            "1e2",
            "1.0",
            "1suffix",
            "-1",
            "+1",
            "01",
            "null",
            "\"1\"",
            "true",
            "18446744073709551616",
        ] {
            assert_bad(&text.replace(
                &format!("\"{field}\": {original}"),
                &format!("\"{field}\": {value}"),
            ));
        }
    }
    let mut maximum = manifest();
    maximum.disk_bytes = u64::MAX;
    maximum.vars_bytes = u64::MAX;
    assert_eq!(
        SnapshotManifest::from_json(&maximum.to_json()).unwrap(),
        maximum
    );
}

#[test]
fn every_duplicate_known_field_is_refused() {
    let text = manifest().to_json();
    let fields: serde_json::Value = serde_json::from_str(&text).unwrap();
    for (key, value) in fields.as_object().unwrap() {
        assert_bad(&text.replacen('{', &format!("{{\"{key}\":{value},"), 1));
    }
    assert_bad(&text.replacen('{', r#"{"\u0076m_id":"vm","#, 1));
}

#[test]
fn fields_must_be_top_level_with_the_declared_types() {
    let fields: serde_json::Value = serde_json::from_str(&manifest().to_json()).unwrap();
    for (key, value) in fields.as_object().unwrap() {
        let mut nested = fields.clone();
        nested.as_object_mut().unwrap().remove(key);
        nested["extension"] = serde_json::json!({key: value});
        assert_bad(&nested.to_string());
        let mut wrong_type = fields.clone();
        wrong_type[key] = if value.is_string() {
            serde_json::json!(1)
        } else {
            serde_json::json!("1")
        };
        assert_bad(&wrong_type.to_string());
    }
}

#[test]
fn invalid_json_string_escapes_and_raw_controls_are_refused() {
    let text = manifest().to_json();
    for value in [
        r#""\q""#,
        r#""\u000x""#,
        r#""\ud800""#,
        r#""\udc00""#,
        "\"raw\nnewline\"",
        "\"raw\tcontrol\"",
    ] {
        assert_bad(&text.replace("\"vm_id\": \"vm\"", &format!("\"vm_id\": {value}")));
    }
}

#[test]
fn additional_fields_cannot_shadow_the_known_fields() {
    let expected = manifest();
    let text = expected.to_json().replacen(
        '{',
        r#"{"extension":{"format_version":999,"vm_id":"other","disk_bytes":0},"#,
        1,
    );
    assert_eq!(SnapshotManifest::from_json(&text).unwrap(), expected);
}

#[test]
fn a_regular_manifest_at_the_limit_verifies_but_larger_files_do_not() {
    let scratch = Scratch::new("manifest-read-size");
    let disk = scratch.write("disk", b"disk");
    let vars = scratch.write("vars", b"vars");
    let snapshot = scratch.path("snapshot");
    let manifest = create_snapshot(&disk, &vars, &snapshot, "vm").unwrap();
    let path = snapshot.join(MANIFEST_NAME);
    let mut text = manifest.to_json();
    text.extend(std::iter::repeat_n(
        ' ',
        MANIFEST_LIMIT as usize - text.len(),
    ));
    fs::write(&path, &text).unwrap();
    assert_eq!(verify_snapshot(&snapshot).unwrap(), manifest);
    text.push(' ');
    fs::write(&path, text).unwrap();
    assert!(matches!(
        verify_snapshot(&snapshot),
        Err(SnapshotError::BadManifest(_))
    ));
    OpenOptions::new()
        .write(true)
        .open(&path)
        .unwrap()
        .set_len(1 << 30)
        .unwrap();
    assert!(matches!(
        verify_snapshot(&snapshot),
        Err(SnapshotError::BadManifest(_))
    ));
}

#[test]
fn unsafe_manifest_files_cannot_change_the_selected_generation_or_original_media() {
    for case in ["symlink", "fifo", "directory", "oversized"] {
        let scratch = Scratch::new(&format!("manifest-read-{case}"));
        let disk = scratch.write("disk", b"snapshot disk");
        let vars = scratch.write("vars", b"snapshot vars");
        let snapshot = scratch.path("snapshot");
        let manifest = create_snapshot(&disk, &vars, &snapshot, "vm").unwrap();
        fs::write(&disk, b"original disk remains").unwrap();
        fs::write(&vars, b"original vars remain").unwrap();
        restore_snapshot(&snapshot, &disk, &vars).unwrap();
        let pair = managed::LockedPair::open(&disk, &vars).unwrap();
        let selected = pair.paths().unwrap();
        fs::write(&selected.0, b"selected live disk").unwrap();
        fs::write(&selected.1, b"selected live vars").unwrap();
        drop(pair);
        let path = snapshot.join(MANIFEST_NAME);
        fs::remove_file(&path).unwrap();
        match case {
            "symlink" => {
                let target = scratch.write("outside", manifest.to_json().as_bytes());
                symlink(target, &path).unwrap();
            }
            "fifo" => {
                let path = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
                // SAFETY: the CString owns a NUL-terminated scratch path for the entire call.
                assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
            }
            "directory" => fs::create_dir(&path).unwrap(),
            "oversized" => {
                fs::write(&path, manifest.to_json()).unwrap();
                OpenOptions::new()
                    .write(true)
                    .open(&path)
                    .unwrap()
                    .set_len(MANIFEST_LIMIT + 1)
                    .unwrap();
            }
            _ => unreachable!(),
        }
        // Keep a FIFO writer open so a regressed blocking reader can be released after the bounded wait.
        let anchor = (case == "fifo").then(|| {
            OpenOptions::new()
                .read(true)
                .write(true)
                .custom_flags(libc::O_NONBLOCK)
                .open(&path)
                .unwrap()
        });
        let (send, receive) = mpsc::channel();
        let (thread_disk, thread_vars, thread_snapshot) =
            (disk.clone(), vars.clone(), snapshot.clone());
        let worker = std::thread::spawn(move || {
            send.send((
                verify_snapshot(&thread_snapshot),
                restore_snapshot(&thread_snapshot, &thread_disk, &thread_vars),
                create_snapshot(&thread_disk, &thread_vars, &thread_snapshot, "new"),
            ))
            .unwrap();
        });
        let (verified, restored, replacement) = receive
            .recv_timeout(Duration::from_secs(3))
            .unwrap_or_else(|error| panic!("{case}: manifest refusal blocked: {error}"));
        drop(anchor);
        worker.join().unwrap();
        assert!(
            matches!(verified, Err(SnapshotError::BadManifest(_))),
            "{case}: verify accepted unsafe manifest"
        );
        assert!(
            matches!(restored, Err(SnapshotError::BadManifest(_))),
            "{case}: restore accepted unsafe manifest"
        );
        assert!(
            matches!(replacement, Err(SnapshotError::Io(_))),
            "{case}: replacement accepted unsafe manifest"
        );
        let pair = managed::LockedPair::open(&disk, &vars).unwrap();
        assert_eq!(
            pair.paths().unwrap(),
            selected,
            "{case}: selected generation changed"
        );
        assert_eq!(fs::read(&selected.0).unwrap(), b"selected live disk");
        assert_eq!(fs::read(&selected.1).unwrap(), b"selected live vars");
        assert_eq!(fs::read(&disk).unwrap(), b"original disk remains");
        assert_eq!(fs::read(&vars).unwrap(), b"original vars remain");
    }
}

#[test]
fn unsafe_disk_and_vars_members_refuse_without_blocking_or_changing_live_media() {
    for name in [DISK_NAME, VARS_NAME] {
        for kind in ["fifo", "symlink", "directory"] {
            let scratch = Scratch::new(&format!("verify-member-{name}-{kind}"));
            let disk = scratch.write("disk", b"snapshot disk");
            let vars = scratch.write("vars", b"snapshot vars");
            let snapshot = scratch.path("snapshot");
            create_snapshot(&disk, &vars, &snapshot, "vm").unwrap();
            fs::write(&disk, b"original live disk").unwrap();
            fs::write(&vars, b"original live vars").unwrap();
            restore_snapshot(&snapshot, &disk, &vars).unwrap();
            let pair = LockedPair::open(&disk, &vars).unwrap();
            let selected = pair.paths().unwrap();
            fs::write(&selected.0, b"selected live disk").unwrap();
            fs::write(&selected.1, b"selected live vars").unwrap();
            drop(pair);
            let path = snapshot.join(name);
            let bytes = fs::read(&path).unwrap();
            fs::remove_file(&path).unwrap();
            match kind {
                "fifo" => {
                    let path = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
                    // SAFETY: this owned CString is a valid scratch filename for the entire syscall.
                    assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
                }
                "symlink" => {
                    let outside = scratch.write("outside", &bytes);
                    symlink(outside, &path).unwrap();
                }
                "directory" => fs::create_dir(&path).unwrap(),
                _ => unreachable!(),
            }
            // A bounded receive detects a regressed FIFO reader; the writer is
            // held until refusal and released if the test unwinds.
            let anchor = (kind == "fifo").then(|| {
                OpenOptions::new()
                    .read(true)
                    .write(true)
                    .custom_flags(libc::O_NONBLOCK)
                    .open(&path)
                    .unwrap()
            });
            let (send, receive) = mpsc::channel();
            let (worker_disk, worker_vars, worker_snapshot) =
                (disk.clone(), vars.clone(), snapshot.clone());
            let worker = std::thread::spawn(move || {
                let verified = verify_snapshot(&worker_snapshot);
                let mut pair = LockedPair::open(&worker_disk, &worker_vars).unwrap();
                let restored = pair.restore_copying(
                    &worker_snapshot,
                    |_, _| panic!("unsafe source reached restore copy"),
                    |_, _| panic!("unsafe source reached publication"),
                );
                send.send((verified, restored)).unwrap();
            });
            let (verified, restored) = receive
                .recv_timeout(Duration::from_secs(3))
                .unwrap_or_else(|error| panic!("{name}/{kind}: verification blocked: {error}"));
            drop(anchor);
            worker.join().unwrap();
            assert!(
                matches!(verified, Err(SnapshotError::Io(_))),
                "{name}/{kind}: unsafe verify result {verified:?}"
            );
            assert!(
                matches!(restored, Err(SnapshotError::Io(_))),
                "{name}/{kind}: unsafe restore result {restored:?}"
            );
            let pair = LockedPair::open(&disk, &vars).unwrap();
            assert_eq!(pair.paths().unwrap(), selected);
            assert!(!selected
                .0
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .join("staging")
                .exists());
            assert_eq!(fs::read(&selected.0).unwrap(), b"selected live disk");
            assert_eq!(fs::read(&selected.1).unwrap(), b"selected live vars");
            assert_eq!(fs::read(&disk).unwrap(), b"original live disk");
            assert_eq!(fs::read(&vars).unwrap(), b"original live vars");
            if kind == "symlink" {
                assert_eq!(fs::read(scratch.path("outside")).unwrap(), bytes);
            }
        }
    }
}

#[test]
fn mismatched_sparse_member_size_is_refused_before_hashing_or_staging() {
    for name in [DISK_NAME, VARS_NAME] {
        let scratch = Scratch::new(&format!("verify-member-size-{name}"));
        let disk = scratch.write("disk", b"disk");
        let vars = scratch.write("vars", b"vars");
        let snapshot = scratch.path("snapshot");
        create_snapshot(&disk, &vars, &snapshot, "vm").unwrap();
        // One TiB is sparse; reading it to discover a declared-size mismatch
        // would take far beyond the bounded refusal checkpoint.
        OpenOptions::new()
            .write(true)
            .open(snapshot.join(name))
            .unwrap()
            .set_len(1 << 40)
            .unwrap();
        let (send, receive) = mpsc::channel();
        let mut pair = LockedPair::open(&disk, &vars).unwrap();
        let before = pair.paths().unwrap();
        let worker = std::thread::spawn(move || {
            let result = pair.restore_copying(
                &snapshot,
                |_, _| panic!("wrong-size source reached copy"),
                |_, _| panic!("wrong-size source reached publication"),
            );
            send.send((result, pair.paths().unwrap())).unwrap();
        });
        let (result, after) = receive
            .recv_timeout(Duration::from_secs(3))
            .unwrap_or_else(|error| panic!("{name}: size refusal hashed the sparse file: {error}"));
        worker.join().unwrap();
        assert!(
            matches!(result, Err(SnapshotError::BadManifest(_))),
            "{name}: {result:?}"
        );
        assert_eq!(after, before);
        assert_eq!(fs::read(&disk).unwrap(), b"disk");
        assert_eq!(fs::read(&vars).unwrap(), b"vars");
    }
}
