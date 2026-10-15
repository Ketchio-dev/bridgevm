//! Exit-42 process recreation builds a fresh controller from the same env.
use super::{LiveInputCommand, LiveInputController};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

static ENV_LOCK: Mutex<()> = Mutex::new(());
const INPUT_ENV: &str = "BRIDGEVM_INPUT_CONTROL";
const GENERATION_ENV: &str = "BRIDGEVM_RESET_GENERATION";

struct Fixture(PathBuf);

impl Fixture {
    fn new(contents: &[u8]) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "bvlr-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&dir).unwrap();
        fs::write(dir.join("input.ctl"), contents).unwrap();
        Self(dir)
    }

    fn input(&self) -> PathBuf {
        self.0.join("input.ctl")
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Build a controller exactly as a helper of `generation` does at startup.
fn helper(input: &Path, generation: Option<&str>) -> LiveInputController {
    std::env::set_var(INPUT_ENV, input);
    match generation {
        Some(value) => std::env::set_var(GENERATION_ENV, value),
        None => std::env::remove_var(GENERATION_ENV),
    }
    let controller = LiveInputController::from_env();
    std::env::remove_var(INPUT_ENV);
    std::env::remove_var(GENERATION_ENV);
    controller
}

fn append(path: &Path, bytes: &[u8]) {
    let mut file = OpenOptions::new().append(true).open(path).unwrap();
    file.write_all(bytes).unwrap();
}

fn keys(input: &LiveInputController) -> Vec<&str> {
    input
        .pending
        .iter()
        .map(|command| match command {
            LiveInputCommand::Key(value) | LiveInputCommand::Pointer(value) => value.as_str(),
            other => panic!("unexpected command {other:?}"),
        })
        .collect()
}

#[test]
fn restarted_helper_does_not_replay_lines_consumed_by_previous_generation() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new(b"KEY text:hunter2\nPOINTER click:10x20\n");
    let mut first = helper(&fixture.input(), Some("0"));
    first.read_new_commands();
    assert_eq!(keys(&first), ["text:hunter2", "click:10x20"]);
    drop(first); // guest SYSTEM_RESET: helper exits 42, runner spawns generation 1

    let mut second = helper(&fixture.input(), Some("1"));
    second.read_new_commands();
    assert!(
        second.pending.is_empty(),
        "restarted helper replayed consumed input: {:?}",
        second.pending
    );
    append(&fixture.input(), b"KEY text:after-reboot\n");
    second.read_new_commands();
    assert_eq!(keys(&second), ["text:after-reboot"]);
}

#[test]
fn restarted_helper_delivers_lines_written_after_it_started() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new(b"KEY text:old-boot\n");
    let mut input = helper(&fixture.input(), Some("2"));
    append(&fixture.input(), b"KEY text:firmware-boot\n");
    input.read_new_commands();
    assert_eq!(keys(&input), ["text:firmware-boot"]);
}

#[test]
fn restarted_helper_skips_old_lines_when_the_channel_only_becomes_usable_later() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new(b"");
    let alias = fixture.0.join("alias");
    fs::hard_link(fixture.input(), &alias).unwrap();
    append(&fixture.input(), b"KEY text:old-boot\n");
    // Two links: the consumer refuses this inode at startup.
    let mut input = helper(&fixture.input(), Some("1"));
    input.read_new_commands();
    assert!(input.pending.is_empty());
    fs::remove_file(&alias).unwrap();
    input.read_new_commands();
    assert!(
        input.pending.is_empty(),
        "deferred resume replayed: {:?}",
        input.pending
    );
    append(&fixture.input(), b"KEY text:new-boot\n");
    input.read_new_commands();
    assert_eq!(keys(&input), ["text:new-boot"]);
}

#[test]
fn first_generation_still_delivers_lines_written_before_the_helper_started() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    for generation in [None, Some("0")] {
        let fixture = Fixture::new(b"KEY text:early\nPOINTER click:1x2\n");
        let mut input = helper(&fixture.input(), generation);
        input.read_new_commands();
        assert_eq!(keys(&input), ["text:early", "click:1x2"], "{generation:?}");
    }
}

#[test]
fn empty_generation_is_first_boot_and_unparsable_never_replays() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new(b"KEY text:old-boot\n");
    for generation in ["", "x", "-1"] {
        let mut input = helper(&fixture.input(), Some(generation));
        input.read_new_commands();
        let expected: &[&str] = if generation.is_empty() {
            &["text:old-boot"]
        } else {
            &[]
        };
        assert_eq!(keys(&input), expected, "{generation:?}");
    }
}
