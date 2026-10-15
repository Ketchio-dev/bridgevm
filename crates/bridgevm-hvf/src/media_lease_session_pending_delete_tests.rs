use super::*;
use crate::media_lease::MediaLease;
use crate::media_lease_session::pending_delete;
use std::os::unix::fs::symlink;
#[path = "media_lease_session_process_test_support.rs"]
mod process;
use process::{absent_fixture, assert_blocked, assert_empty, assert_refused, Session};

#[test]
fn subprocess_pending_session_owns_both_absent_paths_until_release() {
    let fixture = absent_fixture();
    let owner = Session::start("pending", &fixture.disk(), &fixture.vars());
    assert_empty(&fixture);
    assert_blocked(&fixture.disk());
    assert_blocked(&fixture.vars());
    assert_refused("pending", &fixture.disk(), &fixture.vars());
    owner.finish(RELEASE, true);
    assert_empty(&fixture);
    MediaLease::acquire([fixture.disk().as_path(), fixture.vars().as_path()]).unwrap();
    Session::start("pending", &fixture.disk(), &fixture.vars()).finish(RELEASE, true);
    assert_empty(&fixture);
}

#[test]
fn subprocess_later_created_leaves_still_conflict_with_pending_owner() {
    let fixture = absent_fixture();
    let owner = Session::start("pending", &fixture.disk(), &fixture.vars());
    fs::write(fixture.disk(), b"created disk").unwrap();
    assert_blocked(&fixture.disk());
    fs::write(fixture.vars(), b"created vars").unwrap();
    assert_blocked(&fixture.vars());
    assert_refused("lease", &fixture.disk(), &fixture.vars());
    owner.finish(RELEASE, true);
    Session::start("lease", &fixture.disk(), &fixture.vars()).finish(RELEASE, true);
    assert_eq!(fs::read(fixture.disk()).unwrap(), b"created disk");
    assert_eq!(fs::read(fixture.vars()).unwrap(), b"created vars");
}

#[test]
fn subprocess_eof_and_invalid_release_free_pending_paths_without_media() {
    for release in [b"".as_slice(), b"invalid\n".as_slice()] {
        let fixture = absent_fixture();
        let owner = Session::start("pending", &fixture.disk(), &fixture.vars());
        assert_blocked(&fixture.disk());
        assert_blocked(&fixture.vars());
        owner.finish(release, false);
        MediaLease::acquire([fixture.disk().as_path(), fixture.vars().as_path()]).unwrap();
        assert_empty(&fixture);
        Session::start("pending", &fixture.disk(), &fixture.vars()).finish(RELEASE, true);
    }
}

#[test]
fn pending_session_refuses_existing_partial_pairs_and_leaf_symlinks() {
    for name in ["disk", "vars"] {
        for kind in ["file", "directory", "dangling", "symlink"] {
            let fixture = absent_fixture();
            let leaf = fixture.disk().parent().unwrap().join(name);
            match kind {
                "file" => fs::write(&leaf, b"existing").unwrap(),
                "directory" => fs::create_dir(&leaf).unwrap(),
                "dangling" => symlink("missing", &leaf).unwrap(),
                _ => {
                    fs::write(fixture.disk().parent().unwrap().join("target"), b"target").unwrap();
                    symlink("target", &leaf).unwrap();
                }
            }
            assert_refused("pending", &fixture.disk(), &fixture.vars());
            assert!(fs::symlink_metadata(leaf).is_ok());
        }
    }
    let fixture = Fixture::new();
    assert_refused("pending", &fixture.disk(), &fixture.vars());
    fixture.assert_available();
    assert_eq!(fs::read(fixture.disk()).unwrap(), b"disk");
    assert_eq!(fs::read(fixture.vars()).unwrap(), b"vars");
}

#[test]
fn pending_session_refuses_same_paths_missing_or_non_directory_parents() {
    let fixture = absent_fixture();
    assert_refused("pending", &fixture.disk(), &fixture.disk());
    let root = fixture.disk().parent().unwrap().to_path_buf();
    let alias = root.join("alias");
    symlink(&root, &alias).unwrap();
    assert_refused("pending", &fixture.disk(), &alias.join("disk"));
    fs::remove_file(alias).unwrap();
    for bad_parent in [root.join("missing"), fixture.disk()] {
        if bad_parent == fixture.disk() {
            fs::write(&bad_parent, b"not a directory").unwrap();
        }
        assert_refused("pending", &bad_parent.join("leaf"), &fixture.vars());
        assert_refused("pending", &fixture.vars(), &bad_parent.join("leaf"));
    }
    fs::remove_file(fixture.disk()).unwrap();
    assert_empty(&fixture);
}

#[test]
fn canonical_parent_aliases_share_pending_ownership() {
    let fixture = absent_fixture();
    let root = fixture.disk().parent().unwrap().to_path_buf();
    let alias = root.join("alias");
    symlink(&root, &alias).unwrap();
    let owner = Session::start("pending", &alias.join("disk"), &fixture.vars());
    assert_blocked(&fixture.disk());
    assert_blocked(&alias.join("vars"));
    owner.finish(RELEASE, true);
    fs::remove_file(alias).unwrap();
    assert_empty(&fixture);
}

#[test]
fn pending_session_can_own_leaves_in_distinct_existing_parents() {
    let disk_fixture = absent_fixture();
    let vars_fixture = absent_fixture();
    let owner = Session::start("pending", &disk_fixture.disk(), &vars_fixture.vars());
    assert_blocked(&disk_fixture.disk());
    assert_blocked(&vars_fixture.vars());
    owner.finish(RELEASE, true);
    assert_empty(&disk_fixture);
    assert_empty(&vars_fixture);
}

#[test]
fn pending_session_checks_absence_under_ownership_before_ready() {
    let fixture = Fixture::new();
    let owner = MediaLease::acquire([fixture.disk().as_path(), fixture.vars().as_path()]).unwrap();
    let mut output = Vec::new();
    assert_eq!(
        pending_delete::hold(
            &fixture.disk(),
            &fixture.vars(),
            &mut Cursor::new(RELEASE),
            &mut output
        )
        .unwrap_err()
        .kind(),
        io::ErrorKind::WouldBlock
    );
    assert!(output.is_empty());
    drop(owner);
    assert_eq!(
        pending_delete::hold(
            &fixture.disk(),
            &fixture.vars(),
            &mut Cursor::new(RELEASE),
            &mut output
        )
        .unwrap_err()
        .kind(),
        io::ErrorKind::AlreadyExists
    );
    assert!(output.is_empty());
    fixture.assert_available();
}

#[test]
#[ignore = "invoked only by process-owned lease protocol tests"]
fn subprocess() {
    let arguments =
        ["BV_TEST_SESSION_DISK", "BV_TEST_SESSION_VARS"].map(|name| std::env::var(name).unwrap());
    let result = match std::env::var("BV_TEST_SESSION_MODE").unwrap().as_str() {
        "pending" => pending_delete::command(&arguments),
        "lease" => command(&arguments),
        _ => panic!("unknown child mode"),
    };
    std::process::exit(if result == std::process::ExitCode::SUCCESS {
        0
    } else {
        1
    });
}
