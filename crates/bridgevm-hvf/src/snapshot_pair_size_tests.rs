use super::*;
use crate::media_lease::MediaLease;
use crate::snapshot_pair::snapshot_pair_tests::{Scratch, QUOTA};
use crate::snapshot_pair::{create_snapshot, restore_snapshot, SnapshotError};
use std::fs::{self, OpenOptions};
use std::os::unix::fs::symlink;
use std::sync::mpsc;
use std::time::Duration;

#[test]
fn original_pair_sizes_and_logical_aliases_are_supported() {
    let s = Scratch::new("size-original");
    let disk = s.write("disk", &[0; 144]);
    let vars = s.write("vars", &[0; 17]);
    let alias = s.path("disk-alias");
    symlink(&disk, &alias).unwrap();
    for path in [&disk, &alias] {
        assert_eq!(
            selected_size(path, &vars).unwrap(),
            SelectedSize {
                disk_bytes: 144,
                vars_bytes: 17,
            }
        );
    }
}

#[test]
fn restored_selected_sizes_not_preserved_originals_supply_the_explicit_quota() {
    let s = Scratch::new("size-restored");
    let disk = s.write("disk", &[1; 144]);
    let vars = s.write("vars", &[2; 17]);
    let snapshot = s.path("snapshot");
    let original = create_snapshot(&disk, &vars, &snapshot, "vm", false, QUOTA).unwrap();
    fs::write(&disk, [3; 72]).unwrap();
    fs::write(&vars, [4; 9]).unwrap();
    restore_snapshot(&snapshot, &disk, &vars, false).unwrap();

    let size = selected_size(&disk, &vars).unwrap();
    assert_eq!((size.disk_bytes, size.vars_bytes), (144, 17));
    assert_eq!(fs::read(&disk).unwrap(), [3; 72]);
    assert_eq!(fs::read(&vars).unwrap(), [4; 9]);
    let refused = s.path("refused");
    assert!(matches!(
        create_snapshot(&disk, &vars, &refused, "vm", false, 72 + 9),
        Err(SnapshotError::QuotaExceeded {
            bytes: 161,
            quota: 81
        })
    ));
    assert!(!refused.exists());
    let exported = create_snapshot(
        &disk,
        &vars,
        &s.path("exported"),
        "vm",
        false,
        size.disk_bytes + size.vars_bytes,
    )
    .unwrap();
    assert_eq!(exported, original);
}

#[test]
fn stale_size_quota_refuses_after_selection_grows_and_preserves_prior_snapshot() {
    let s = Scratch::new("size-stale-quota");
    let disk = s.write("disk", &[1; 72]);
    let vars = s.write("vars", &[2; 8]);
    let prior = s.path("prior");
    let previous = create_snapshot(&disk, &vars, &prior, "vm", false, QUOTA).unwrap();
    let larger_disk = s.write("larger-disk", &[3; 144]);
    let larger_vars = s.write("larger-vars", &[4; 16]);
    let larger = s.path("larger");
    create_snapshot(&larger_disk, &larger_vars, &larger, "vm", false, QUOTA).unwrap();

    let stale = selected_size(&disk, &vars).unwrap();
    assert_eq!((stale.disk_bytes, stale.vars_bytes), (72, 8));
    restore_snapshot(&larger, &disk, &vars, false).unwrap();
    assert!(matches!(
        create_snapshot(
            &disk,
            &vars,
            &prior,
            "vm",
            false,
            stale.disk_bytes + stale.vars_bytes,
        ),
        Err(SnapshotError::QuotaExceeded {
            bytes: 160,
            quota: 80
        })
    ));
    assert_eq!(
        crate::snapshot_pair::verify_snapshot(&prior).unwrap(),
        previous
    );
    assert_eq!(fs::read(&disk).unwrap(), [1; 72]);
    assert_eq!(fs::read(&vars).unwrap(), [2; 8]);
    let selected = selected_size(&disk, &vars).unwrap();
    assert_eq!((selected.disk_bytes, selected.vars_bytes), (144, 16));
}

#[test]
fn logical_and_selected_member_leases_refuse_size_until_released() {
    let s = Scratch::new("size-leased");
    let disk = s.write("disk", b"disk");
    let vars = s.write("vars", b"vars");
    let snapshot = s.path("snapshot");
    create_snapshot(&disk, &vars, &snapshot, "vm", false, QUOTA).unwrap();
    restore_snapshot(&snapshot, &disk, &vars, false).unwrap();
    let pair = LockedPair::open(&disk, &vars).unwrap();
    assert_eq!(
        selected_size(&disk, &vars).unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
    let (selected_disk, selected_vars) = pair.paths().unwrap();
    drop(pair);
    for member in [&disk, &vars, &selected_disk, &selected_vars] {
        let lease = MediaLease::acquire([member.as_path()]).unwrap();
        assert_eq!(
            selected_size(&disk, &vars).unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
        drop(lease);
        assert!(selected_size(&disk, &vars).is_ok());
    }
}

#[test]
fn invalid_original_or_selected_member_type_fails_closed() {
    for restored in [false, true] {
        for member in ["disk", "vars"] {
            let s = Scratch::new(&format!("size-invalid-{restored}-{member}"));
            let disk = s.write("disk", b"disk");
            let vars = s.write("vars", b"vars");
            if restored {
                let snapshot = s.path("snapshot");
                create_snapshot(&disk, &vars, &snapshot, "vm", false, QUOTA).unwrap();
                restore_snapshot(&snapshot, &disk, &vars, false).unwrap();
            }
            let pair = LockedPair::open(&disk, &vars).unwrap();
            let (selected_disk, selected_vars) = pair.paths().unwrap();
            drop(pair);
            let path = if member == "disk" {
                selected_disk
            } else {
                selected_vars
            };
            fs::remove_file(&path).unwrap();
            fs::create_dir(&path).unwrap();
            assert!(selected_size(&disk, &vars).is_err());
            fs::remove_dir(&path).unwrap();
            assert!(selected_size(&disk, &vars).is_err());
            if restored {
                let outside = s.write("outside", b"not selected media");
                symlink(&outside, &path).unwrap();
                assert!(selected_size(&disk, &vars).is_err());
            }
        }
    }
}

#[test]
fn huge_sparse_disk_is_sized_without_reading_or_hashing_it() {
    let s = Scratch::new("size-sparse");
    let disk = s.write("disk", b"");
    let vars = s.write("vars", b"");
    OpenOptions::new()
        .write(true)
        .open(&disk)
        .unwrap()
        .set_len(1 << 40)
        .unwrap();
    let (send, receive) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        send.send(selected_size(&disk, &vars)).unwrap();
    });
    let size = receive
        .recv_timeout(Duration::from_secs(3))
        .expect("size read file content")
        .unwrap();
    worker.join().unwrap();
    assert_eq!((size.disk_bytes, size.vars_bytes), (1 << 40, 0));
}

#[test]
fn report_has_exactly_two_decimal_lines_and_propagates_write_errors() {
    let size = SelectedSize {
        disk_bytes: 0,
        vars_bytes: u64::MAX,
    };
    let mut output = Vec::new();
    write_report(&size, &mut output).unwrap();
    assert_eq!(output, b"disk_bytes 0\nvars_bytes 18446744073709551615\n");
    assert!(write_report(&size, &mut [0u8; 1][..]).is_err());
    assert_eq!(command(&[]), ExitCode::from(2));
    assert_eq!(command(&["disk".into()]), ExitCode::from(2));
    assert_eq!(
        command(&["disk".into(), "vars".into(), "extra".into()]),
        ExitCode::from(2)
    );
}
