//! Use the supported pinned-file RAII lock for the causal held-writer regression.

use super::*;
use std::sync::mpsc;

#[test]
fn a_held_writer_defers_preparation_without_losing_partial_or_queued_commands() {
    let mut file = InputFile::new("held-writer", b"KEY a\nKEY text:par");
    let now = Instant::now();
    let mut input = file.controller(now);
    assert!(input.prepare_tick(now, false));
    let before = input.offset;
    assert_eq!(input.pending.len(), 1);
    assert!(!input.lines.is_empty());
    let poll = now + POLL_INTERVAL;
    let (reader, completed_while_locked) = InputControlFile::from_path(file.path.clone())
        .with_exclusive(|_| {
            file.writer.write_all(b"tial\nKEY b\n").unwrap();
            let (finished, completion) = mpsc::channel();
            let reader = std::thread::spawn(move || {
                let ready = input.prepare_tick(poll, false);
                let _ = finished.send((input, ready));
            });
            let completed = completion.recv_timeout(Duration::from_secs(5));
            (reader, completed)
        })
        .expect("acquire the fixture writer lock");
    // The RAII lock drops before joining even a reader blocked by the old behavior.
    reader.join().unwrap();
    let (mut input, ready) =
        completed_while_locked.expect("prepare waited for the writer to unlock");
    assert!(ready, "already queued device work must remain eligible");
    assert_eq!(input.offset, before);
    assert_eq!(input.pending.len(), 1);
    assert!(!input.lines.is_empty());
    assert!(!input.prepare_tick(poll + POLL_INTERVAL / 2, false));
    assert!(input.prepare_tick(poll + POLL_INTERVAL, false));
    assert_eq!(input.pending.len(), 3);
    assert!(matches!(&input.pending[0], LiveInputCommand::Key(value) if value == "a"));
    assert!(matches!(&input.pending[1], LiveInputCommand::Key(value) if value == "text:partial"));
    assert!(matches!(&input.pending[2], LiveInputCommand::Key(value) if value == "b"));
    assert!(input.lines.is_empty());
    let consumed = input.offset;
    assert!(input.prepare_tick(poll + 2 * POLL_INTERVAL, false));
    assert_eq!(
        (input.offset, input.pending.len()),
        (consumed, 3),
        "a retry replayed input"
    );
}
