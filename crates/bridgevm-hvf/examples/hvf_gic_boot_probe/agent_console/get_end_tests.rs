//! Harness-mode GETEND completes only the GET command whose path its GETBEG
//! named, and the file reaches BRIDGEVM_VIRTIO_CONSOLE_GET_DIR through the
//! share's descriptor-relative Destination: never through a symlink at the leaf
//! name, never by writing into an existing file. The directory is a
//! process-global environment variable that every completed GET reads, so each
//! test holds GET_DIR_LOCK while it runs.

use super::*;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};
#[path = "get_dir_policy_tests.rs"]
mod dir_policy;

static GET_DIR_LOCK: Mutex<()> = Mutex::new(());
const GET_DIR_ENV: &str = "BRIDGEVM_VIRTIO_CONSOLE_GET_DIR";

/// Holds GET_DIR_LOCK with the GET directory unset, or set to the `get`
/// directory of a fresh scratch root; unsets it and removes the root on drop.
struct GetDir {
    root: Option<PathBuf>,
    _lock: MutexGuard<'static, ()>,
}

impl GetDir {
    fn unset() -> Self {
        let lock = GET_DIR_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        std::env::remove_var(GET_DIR_ENV);
        Self {
            root: None,
            _lock: lock,
        }
    }
    fn scratch(label: &str) -> Self {
        let mut guard = Self::unset();
        let id = std::process::id();
        let root = std::env::temp_dir().join(format!("bvagent-get-end-{label}-{id}"));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("get")).unwrap();
        std::env::set_var(GET_DIR_ENV, root.join("get"));
        guard.root = Some(root);
        guard
    }
    fn root(&self) -> &Path {
        self.root.as_deref().unwrap()
    }
    fn dir(&self) -> PathBuf {
        self.root().join("get")
    }
}

impl Drop for GetDir {
    fn drop(&mut self) {
        std::env::remove_var(GET_DIR_ENV);
        if let Some(root) = &self.root {
            let _ = std::fs::remove_dir_all(root);
        }
    }
}

fn get_command(path: &str) -> String {
    format!("GET {}", base64_encode(path.as_bytes()))
}

/// Feed GETBEG and one GETCHUNK for `path` holding `bytes`, as the agent sends them.
fn begin(h: &mut AgentConsoleHarness, path: &str, bytes: &[u8]) {
    let path_b64 = base64_encode(path.as_bytes());
    let fragments = [
        format!("GETBEG {path_b64} {} 1", bytes.len()),
        format!("GETCHUNK 0 {}", base64_encode(bytes)),
    ];
    for fragment in fragments {
        assert!(h.handle_unlabelled_get_fragment(&fragment).is_some());
    }
}

/// One whole GET of `path` returning `bytes`, ended under its own command.
fn get(h: &mut AgentConsoleHarness, path: &str, bytes: &[u8]) -> ReplyProgress {
    begin(h, path, bytes);
    h.handle_reply_line("GETEND 1", &get_command(path))
}

#[test]
fn a_symlink_at_the_get_dir_leaf_is_never_followed() {
    let get_dir = GetDir::scratch("symlink");
    let victim = get_dir.root().join("victim.txt");
    std::fs::write(&victim, b"victim bytes").unwrap();
    std::os::unix::fs::symlink(&victim, get_dir.dir().join("leaf.bin")).unwrap();
    let mut h = harness();
    let progress = get(&mut h, "C:\\guest\\leaf.bin", b"guest");
    assert!(matches!(progress, ReplyProgress::Complete));
    let victim_bytes = std::fs::read(&victim).unwrap();
    assert_eq!(
        victim_bytes, b"victim bytes",
        "the GET wrote through the leaf symlink"
    );
    let leaf = std::fs::symlink_metadata(get_dir.dir().join("leaf.bin")).unwrap();
    assert!(leaf.file_type().is_symlink());
}

#[test]
fn an_existing_file_is_replaced_and_never_written_in_place() {
    let get_dir = GetDir::scratch("replace");
    // A hard link shares the inode that an in-place write would change.
    let outside = get_dir.root().join("outside.bin");
    std::fs::write(&outside, b"outside bytes").unwrap();
    std::fs::hard_link(&outside, get_dir.dir().join("linked.bin")).unwrap();
    std::fs::write(get_dir.dir().join("old.bin"), b"previous").unwrap();
    let mut h = harness();
    let gets = [
        ("C:\\guest\\new.bin", &b"fresh"[..]),
        ("C:\\guest\\old.bin", &b"replaced"[..]),
        ("C:\\guest\\linked.bin", &b"guest"[..]),
    ];
    for (path, bytes) in gets {
        assert!(matches!(get(&mut h, path, bytes), ReplyProgress::Complete));
        assert!(h.get_accum.is_none());
    }
    let read = |name: &str| std::fs::read(get_dir.dir().join(name)).unwrap();
    assert_eq!(read("new.bin"), b"fresh");
    assert_eq!(read("old.bin"), b"replaced");
    assert_eq!(read("linked.bin"), b"guest");
    let outside_bytes = std::fs::read(&outside).unwrap();
    assert_eq!(
        outside_bytes, b"outside bytes",
        "the GET wrote into an existing inode"
    );
    let mut names: Vec<String> = std::fs::read_dir(get_dir.dir())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    let expected = ["linked.bin", "new.bin", "old.bin"];
    assert_eq!(names, expected, "a temporary file was left behind");
}

#[test]
fn a_getend_without_its_getbeg_is_ignored_like_any_unmatched_line() {
    let _get_dir = GetDir::unset();
    let mut h = harness();
    // No GETBEG at all, while a GET or another command is in flight.
    for command in [get_command("C:\\guest\\a.txt"), "whoami".to_string()] {
        let progress = h.handle_reply_line("GETEND 0", &command);
        assert!(matches!(progress, ReplyProgress::Ignored));
    }
    // A GETBEG arrived, but it names no path the command in flight asked for.
    begin(&mut h, "C:\\guest\\a.txt", b"bytes");
    for command in ["whoami".to_string(), get_command("C:\\guest\\b.txt")] {
        let progress = h.handle_reply_line("GETEND 1", &command);
        assert!(matches!(progress, ReplyProgress::Ignored));
    }
    assert!(
        h.get_accum.is_some(),
        "an ignored GETEND consumed the reassembly"
    );
    // The GET it began still completes. The agent decodes the argument with
    // .NET's FromBase64String, which skips whitespace, so a doubled space in
    // the command names the same path.
    let own = format!("GET  {}", base64_encode(b"C:\\guest\\a.txt"));
    let progress = h.handle_reply_line("GETEND 1", &own);
    assert!(matches!(progress, ReplyProgress::Complete));
}

#[test]
fn a_getbeg_does_not_outlive_the_command_it_answered() {
    let _get_dir = GetDir::unset();
    let mut h = harness();
    let command = get_command("C:\\guest\\a.txt");
    // The agent began the GET, then failed it: ERR completes the command.
    begin(&mut h, "C:\\guest\\a.txt", b"partial");
    let progress = h.handle_reply_line("ERR GET read failed", &command);
    assert!(matches!(progress, ReplyProgress::Complete));
    assert!(
        h.get_accum.is_none(),
        "the failed GET's reassembly outlived its command"
    );
    // A later GET of the same file gets a stray GETEND before any GETBEG.
    let progress = h.handle_reply_line("GETEND 1", &command);
    assert!(matches!(progress, ReplyProgress::Ignored));
}

#[test]
fn a_stray_getend_keeps_a_service_command_in_flight() {
    let _get_dir = GetDir::unset();
    let mut h = harness();
    h.state = AgentConsoleState::Service;
    let get = get_command("C:\\guest\\a.txt");
    for command in ["whoami".to_string(), get.clone()] {
        h.in_flight = Some((ServiceReq::Ctl(command), Instant::now()));
        h.handle_service_reply("GETEND 0", None, None, Instant::now());
        let kept = h.in_flight.is_some();
        assert!(kept, "a GETEND without its GETBEG completed the command");
    }
    let lines = [
        format!("GETBEG {} 1 1", base64_encode(b"C:\\guest\\a.txt")),
        format!("GETCHUNK 0 {}", base64_encode(b"x")),
        "GETEND 1".to_string(),
    ];
    for line in lines {
        h.handle_service_reply(&line, None, None, Instant::now());
    }
    let completed = h.in_flight.is_none();
    assert!(completed, "the GET that its GETBEG began did not complete");
}
