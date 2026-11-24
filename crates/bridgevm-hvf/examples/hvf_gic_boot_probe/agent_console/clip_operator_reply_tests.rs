//! Operator clipboard commands from the control file or a scripted command
//! keep clipboard text out of run.log. Each reply is
//! one bounded line with the decoded byte length and the first 16 hex digits of
//! the SHA-256 of those bytes, so a verifier that knows the expected text can
//! still compare it. The line goes through println!, which the test harness
//! captures, so the reply is handled in a child of this test binary and the
//! child's real stdout is read.
#![cfg(test)]

use super::agent_console_tests::tests::harness;
use super::*;
use std::process::Command;

const CHILD_ENV: &str = "BRIDGEVM_TEST_OPERATOR_CLIPBOARD_CHILD";
const TEST_NAME: &str = "operator_clipboard_replies_print_only_fingerprints";
/// A stand-in guest text: Korean, two lines, and a nonce no other output has.
const TEXT: &str = "브리지VM CLIPGET 지문 v1\nnonce-7f3a9c\n";
/// The first 16 hex digits of `shasum -a 256` of TEXT (43 bytes).
const TEXT_SHA256: &str = "a9817db2a8752f60";
/// One byte over the 8 MiB guest-to-host limit, all `x`.
const LARGE: usize = 8 * 1024 * 1024 + 1;
/// The first 16 hex digits of the SHA-256 of LARGE bytes of `x`.
const LARGE_SHA256: &str = "942d6013edf5b8bf";
/// The longest line any reply may print.
const MAX_LINE: usize = 160;

/// The child handles each reply through the relay path that writes run.log.
fn answer_operator_commands() {
    let replies = [
        format!("CLIP {}", base64_encode(TEXT.as_bytes())),
        format!("CLIP {}", base64_encode(&vec![b'x'; LARGE])),
        "CLIP @@@@".to_string(),
        "CLIP ".to_string(),
    ];
    let mut h = harness();
    for reply in replies {
        let progress = h.handle_reply_line(&reply, "CLIPGET");
        assert!(matches!(progress, ReplyProgress::Complete));
    }
    let payload = base64_encode(TEXT.as_bytes());
    let command = format!("CLIPSET {payload}");
    assert_eq!(command_wire_line(&command), format!("{command}\n"));
    for reply in ["OK CLIPSET".to_string(), format!("ERR CLIPSET {payload}")] {
        assert!(matches!(
            h.handle_reply_line(&reply, &command),
            ReplyProgress::Complete
        ));
    }
}

#[test]
fn operator_clipboard_replies_print_only_fingerprints() {
    if std::env::var_os(CHILD_ENV).is_some() {
        answer_operator_commands();
        return;
    }
    let module = module_path!().split_once("::").unwrap().1;
    let child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", &format!("{module}::{TEST_NAME}"), "--nocapture"])
        .env(CHILD_ENV, "1")
        .output()
        .unwrap();
    assert!(child.status.success(), "child {}", child.status);
    let stdout = String::from_utf8_lossy(&child.stdout);
    // Byte counts only: a failure message must not carry the text either.
    let leaked = [
        "nonce-7f3a9c",
        "브리지VM",
        "xxxxxxxx",
        "BVAGENT END CLIPGET",
    ]
    .iter()
    .filter(|marker| stdout.contains(*marker))
    .count();
    assert_eq!(leaked, 0, "guest text in a {} byte stdout", stdout.len());
    assert!(
        !stdout.contains(&base64_encode(TEXT.as_bytes())),
        "encoded clipboard in run.log"
    );
    let longest = stdout.lines().map(str::len).max().unwrap_or(0);
    assert!(longest <= MAX_LINE, "a {longest} byte line");
    let replies: Vec<&str> = stdout
        .lines()
        .filter(|line| line.starts_with("BVAGENT CLIP"))
        .collect();
    let expected = [
        format!(
            "BVAGENT CLIP CLIPGET bytes={} sha256={TEXT_SHA256}",
            TEXT.len()
        ),
        format!("BVAGENT CLIP CLIPGET bytes={LARGE} sha256={LARGE_SHA256}"),
        "BVAGENT CLIP CLIPGET base64-error=Byte payload-bytes=4".to_string(),
        "BVAGENT CLIP CLIPGET bytes=0 sha256=e3b0c44298fc1c14".to_string(),
        format!(
            "BVAGENT CLIPSET bytes={} sha256={TEXT_SHA256} -> OK CLIPSET",
            TEXT.len()
        ),
        format!(
            "BVAGENT CLIPSET bytes={} sha256={TEXT_SHA256} -> ERR CLIPSET",
            TEXT.len()
        ),
    ];
    assert_eq!(replies, expected);
}
