use super::{InputControlFile, LiveInputCommand, LiveInputController, MAX_PENDING_COMMANDS};
use std::collections::VecDeque;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use std::sync::atomic::{AtomicU64, Ordering};
struct Fixture {
    input: LiveInputController,
    path: PathBuf,
}

impl Fixture {
    fn new(contents: &str) -> Self {
        let nonce = { static NEXT: AtomicU64 = AtomicU64::new(0); (SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos(), NEXT.fetch_add(1, Ordering::Relaxed)) };
        let path = std::env::temp_dir().join(format!("bridgevm-input-backpressure-{}-{}-{}", std::process::id(), nonce.0, nonce.1));
        let mut file = OpenOptions::new().write(true).create_new(true).open(&path).unwrap();
        file.write_all(contents.as_bytes()).unwrap();
        Self {
            input: LiveInputController {
                source: Some(InputControlFile::from_path(path.clone())),
                offset: 0,
                lines: Default::default(),
                pending: VecDeque::new(),
                accepted_pointer_moves: 0,
                next_poll: Instant::now(),
            },
            path,
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

#[test]
fn queued_capacity_preserves_every_later_key_and_crlf_offset() {
    let contents: String = (0..130).map(|i| format!("KEY text:k{i}\r\n")).collect();
    let mut fixture = Fixture::new(&contents);
    let mut observed = Vec::new();
    for _ in 0..3 {
        fixture.input.read_new_commands();
        assert!(fixture.input.pending.len() <= MAX_PENDING_COMMANDS);
        while let Some(command) = fixture.input.pending.pop_front() {
            match command {
                LiveInputCommand::Key(value) => observed.push(value),
                _ => panic!("unexpected command"),
            }
        }
    }
    assert_eq!(observed, (0..130).map(|i| format!("text:k{i}")).collect::<Vec<_>>());
    assert_eq!(fixture.input.offset, contents.len() as u64);
}

#[test]
fn full_busy_queue_does_not_advance_file_cursor_or_drop_later_input() {
    let contents = "KEY enter\n".repeat(65);
    let mut fixture = Fixture::new(&contents);
    fixture.input.read_new_commands();
    assert_eq!(fixture.input.pending.len(), MAX_PENDING_COMMANDS);
    let offset = fixture.input.offset;
    assert_eq!(offset, ("KEY enter\n".len() * 64) as u64);
    fixture.input.read_new_commands();
    assert_eq!(fixture.input.offset, offset);
    assert_eq!(fs::read(&fixture.path).unwrap(), contents.as_bytes());
    fixture.input.pending.pop_front();
    fixture.input.read_new_commands();
    assert_eq!(fixture.input.pending.len(), MAX_PENDING_COMMANDS);
    assert_eq!(fixture.input.offset, contents.len() as u64);
}

#[test]
fn pointer_release_behind_full_keyboard_queue_remains_deliverable() {
    let contents = "KEY enter\n".repeat(64) + "POINTER release:10x20\n";
    let mut fixture = Fixture::new(&contents);
    fixture.input.read_new_commands();
    assert_eq!(fixture.input.pending.len(), MAX_PENDING_COMMANDS);
    assert!(fixture.input.pending.iter().all(|command| matches!(command, LiveInputCommand::Key(_))));
    fixture.input.pending.pop_front();
    fixture.input.read_new_commands();
    assert!(matches!(fixture.input.pending.back(), Some(LiveInputCommand::Pointer(value)) if value == "release:10x20"));
    assert_eq!(fixture.input.pending.len(), MAX_PENDING_COMMANDS);
    assert_eq!(fixture.input.offset, contents.len() as u64);
}
