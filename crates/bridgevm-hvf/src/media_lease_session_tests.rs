use super::*;
use std::fs;
use std::io::Cursor;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
#[path = "media_lease_session_test_support.rs"]
mod support;
use support::{Contender, Fixture};
#[path = "media_lease_session_pending_delete_tests.rs"]
mod pending_delete;

#[test]
fn ordinary_lease_still_refuses_absent_or_partial_pairs() {
    for missing in [vec!["disk", "vars"], vec!["disk"], vec!["vars"]] {
        let fixture = Fixture::new();
        for name in missing {
            fs::remove_file(fixture.disk().parent().unwrap().join(name)).unwrap();
        }
        let mut output = Vec::new();
        assert_eq!(
            hold(
                &fixture.disk(),
                &fixture.vars(),
                &mut Cursor::new(RELEASE),
                &mut output
            )
            .unwrap_err()
            .kind(),
            io::ErrorKind::NotFound
        );
        assert!(output.is_empty());
    }
}

#[test]
fn ready_session_holds_ownership_until_exact_release() {
    let fixture = Fixture::new();
    let mut input = Contender {
        fixture: &fixture,
        command: RELEASE,
    };
    let mut output = Vec::new();
    hold(&fixture.disk(), &fixture.vars(), &mut input, &mut output).unwrap();
    assert_eq!(output, READY);
    fixture.assert_available();
}

#[test]
fn eof_and_invalid_command_fail_and_release_ownership() {
    for (command, expected) in [
        (b"".as_slice(), io::ErrorKind::UnexpectedEof),
        (b"invalid\n".as_slice(), io::ErrorKind::InvalidData),
    ] {
        let fixture = Fixture::new();
        let mut input = Contender {
            fixture: &fixture,
            command,
        };
        let mut output = Vec::new();
        assert_eq!(
            hold(&fixture.disk(), &fixture.vars(), &mut input, &mut output)
                .unwrap_err()
                .kind(),
            expected
        );
        assert_eq!(output, READY);
        fixture.assert_available();
    }
}

#[test]
fn competing_owner_never_receives_ready() {
    let fixture = Fixture::new();
    let owner = LockedPair::open(&fixture.disk(), &fixture.vars()).unwrap();
    let mut output = Vec::new();
    assert_eq!(
        hold(
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
    fixture.assert_available();
}

#[test]
fn broken_ready_transport_does_not_retain_ownership() {
    struct Broken;
    impl Write for Broken {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::ErrorKind::BrokenPipe.into())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let fixture = Fixture::new();
    assert_eq!(
        hold(
            &fixture.disk(),
            &fixture.vars(),
            &mut Cursor::new(RELEASE),
            &mut Broken
        )
        .unwrap_err()
        .kind(),
        io::ErrorKind::BrokenPipe
    );
    fixture.assert_available();
}
