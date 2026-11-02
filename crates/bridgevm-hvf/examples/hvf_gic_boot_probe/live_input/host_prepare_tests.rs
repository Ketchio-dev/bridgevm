//! Host preparation defers held writers and device application uses its fresh time.

use super::*;
use crate::xhci_hid_input::test_support::{
    configure_dci3_and_dci5_interrupt_in_over_bar0, new_platform_and_ram, program_xhci_bar0,
    write_dci5_normal_trb, DCI5_POINTER_BUFFER, DCI5_RING, TRB_SIZE,
};
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::mpsc;

struct InputFile {
    writer: File,
    path: PathBuf,
}
impl InputFile {
    fn new(label: &str, bytes: &[u8]) -> Self {
        for nonce in 0..128 {
            let path = std::env::temp_dir().join(format!(
                "bridgevm-input-host-{label}-{}-{nonce}.ctl",
                std::process::id()
            ));
            match OpenOptions::new()
                .read(true)
                .append(true)
                .create_new(true)
                .open(&path)
            {
                Ok(mut writer) => {
                    writer.write_all(bytes).unwrap();
                    return Self { writer, path };
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => panic!("create owned control file: {error}"),
            }
        }
        panic!("no fresh input control file available");
    }
    fn controller(&self, now: Instant) -> LiveInputController {
        LiveInputController {
            source: Some(InputControlFile::from_path(self.path.clone())),
            offset: 0,
            lines: Default::default(),
            pending: Default::default(),
            accepted_pointer_moves: 0,
            next_poll: now,
        }
    }
}
impl Drop for InputFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

#[test]
fn a_held_writer_defers_preparation_without_losing_partial_or_queued_commands() {
    let mut file = InputFile::new("held-writer", b"KEY a\nKEY text:par");
    let now = Instant::now();
    let mut input = file.controller(now);
    assert!(input.prepare_tick(now, false));
    let before = input.offset;
    assert_eq!(input.pending.len(), 1);
    assert!(!input.lines.is_empty());
    file.writer.lock().unwrap();
    file.writer.write_all(b"tial\nKEY b\n").unwrap();
    let (finished, completion) = mpsc::channel();
    let poll = now + POLL_INTERVAL;
    let reader = std::thread::spawn(move || {
        let ready = input.prepare_tick(poll, false);
        let _ = finished.send((input, ready));
    });
    let completed_while_locked = completion.recv_timeout(Duration::from_secs(5));
    // Unlock even on a failing blocking implementation, before joining its reader.
    file.writer.unlock().unwrap();
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

#[test]
fn prepared_pointer_uses_the_time_sampled_when_the_platform_is_reacquired() {
    let file = InputFile::new("apply-clock", b"POINTER click:16384x16384\n");
    let (mut platform, mut mem) = new_platform_and_ram();
    program_xhci_bar0(&mut platform, &mut mem);
    configure_dci3_and_dci5_interrupt_in_over_bar0(&mut platform, &mut mem);
    for index in 0..2 {
        write_dci5_normal_trb(
            &mut mem,
            DCI5_RING + TRB_SIZE * index,
            DCI5_POINTER_BUFFER + 0x20 * index,
        );
    }
    platform.set_xhci_report_interval(Duration::from_secs(1));
    let prepared_at = Instant::now();
    platform.set_host_now(prepared_at);
    let mut input = file.controller(prepared_at);
    assert!(input.prepare_tick(prepared_at, false));
    assert_eq!(
        platform
            .xhci_pointer_input_report_stats()
            .emitted_button_reports,
        0
    );
    let background_time = prepared_at + Duration::from_millis(850);
    platform.set_host_now(background_time);
    let applied_at = prepared_at + Duration::from_millis(900);
    input.apply_pending(&mut platform, &mut mem, applied_at);
    assert!(input.pending.is_empty());
    assert_eq!(
        platform
            .xhci_pointer_input_report_stats()
            .emitted_button_reports,
        1
    );
    assert_eq!(
        platform.xhci_pointer_report_deadline(),
        Some(applied_at + Duration::from_secs(1))
    );
}
