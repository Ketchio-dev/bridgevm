use std::collections::VecDeque;
use std::time::{Duration, Instant};

#[path = "input_control_file.rs"]
mod input_control_file;
pub(crate) use input_control_file::InputControlFile;
const POLL_INTERVAL: Duration = Duration::from_millis(16);
const MAX_PENDING_COMMANDS: usize = 64;
use line_frames::MAX_COMMAND_BYTES;
const MAX_READ_BYTES_PER_TICK: u64 = 1024 * 1024;
const COMPACT_AFTER_BYTES: u64 = 1024 * 1024;
#[cfg(test)]
#[path = "live_input/backpressure_tests.rs"]
mod backpressure_tests;
#[path = "live_input/control_reader.rs"]
mod control_reader;
#[path = "live_input/tick_adapter.rs"]
mod tick_adapter;
#[path = "live_input/device_apply.rs"]
mod device_apply;
#[cfg(test)]
#[path = "live_input/host_prepare_tests.rs"]
mod host_prepare_tests;
#[cfg(test)]
#[path = "live_input/identity_tests.rs"]
mod identity_tests;
#[path = "live_input/key_actions.rs"]
mod key_actions;
#[path = "live_input/line_frames.rs"]
mod line_frames;
#[cfg(test)]
#[path = "live_input/restart_replay_tests.rs"]
mod restart_replay_tests;
#[path = "live_input/restart_resume.rs"]
mod restart_resume;
enum LiveInputCommand {
    Key(String),
    Pointer(String),
    Resize {
        width: u32,
        height: u32,
        scale: Option<u32>,
    },
    Snapshot(String),
}

impl std::fmt::Debug for LiveInputCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            // Diagnostics must never persist typed text or individual keys.
            Self::Key(_) => f.write_str("Key(<redacted>)"),
            Self::Pointer(value) => f.debug_tuple("Pointer").field(value).finish(),
            Self::Resize {
                width,
                height,
                scale,
            } => f
                .debug_struct("Resize")
                .field("width", width)
                .field("height", height)
                .field("scale", scale)
                .finish(),
            Self::Snapshot(label) => f.debug_tuple("Snapshot").field(label).finish(),
        }
    }
}

impl LiveInputCommand {
    fn is_pointer_move(&self) -> bool {
        matches!(self, Self::Pointer(value) if value.starts_with("move:"))
    }
}

/// `WxH`, optionally `@P`: a Windows display scale in percent.
fn parse_resize(value: &str) -> Option<(u32, u32, Option<u32>)> {
    let (size, scale) = match value.split_once('@') {
        Some((size, scale)) => {
            let scale: u32 = scale.trim().parse().ok()?;
            (size, Some(scale))
        }
        None => (value, None),
    };
    if scale.is_some_and(|p| !crate::agent_console::DESKTOP_SCALES.contains(&p)) {
        return None;
    }
    let (w, h) = size.trim().split_once('x')?;
    Some((w.trim().parse().ok()?, h.trim().parse().ok()?, scale))
}

pub struct LiveInputController {
    source: Option<InputControlFile>,
    offset: u64,
    lines: line_frames::InputLines,
    pending: VecDeque<LiveInputCommand>,
    accepted_pointer_moves: u64,
    next_poll: Instant,
}

impl LiveInputController {
    pub fn from_env() -> Self {
        let mut source = InputControlFile::from_env();
        Self {
            offset: restart_resume::initial_offset(source.as_mut()),
            source,
            lines: Default::default(),
            pending: VecDeque::new(),
            accepted_pointer_moves: 0,
            next_poll: Instant::now(),
        }
    }

    pub fn poll_due(&self, now: Instant) -> bool {
        self.source.is_some() && now >= self.next_poll
    }

    /// Read and queue host input without a platform or guest-memory borrow.
    /// A busy writer defers only this read; already queued device work can run.
    pub fn prepare_tick(&mut self, now: Instant, event_wake: bool) -> bool {
        if !event_wake && !self.poll_due(now) {
            return false;
        }
        self.next_poll = now + POLL_INTERVAL;
        self.read_new_commands();
        true
    }

    fn read_new_commands(&mut self) {
        if self.pending.len() >= MAX_PENDING_COMMANDS {
            return;
        }
        let Some(mut source) = self.source.take() else {
            return;
        };
        let _ = source.try_with_exclusive(|file| self.read_new_commands_locked(file));
        self.source = Some(source);
    }

    fn push_line(&mut self, line: &str) {
        if line.is_empty() || line.len() > MAX_COMMAND_BYTES {
            return;
        }
        let command = if let Some(value) = line.strip_prefix("KEY ") {
            LiveInputCommand::Key(value.to_string())
        } else if let Some(value) = line.strip_prefix("POINTER ") {
            LiveInputCommand::Pointer(value.to_string())
        } else if let Some(value) = line.strip_prefix("RESIZE ") {
            match parse_resize(value) {
                Some((width, height, scale)) => LiveInputCommand::Resize {
                    width,
                    height,
                    scale,
                },
                None => {
                    eprintln!("live input rejected: kind=resize parse_error");
                    return;
                }
            }
        } else if let Some(value) = line.strip_prefix("SNAPSHOT ") {
            let label = value.trim();
            if label.is_empty() {
                eprintln!("live input rejected: kind=snapshot empty_label");
                return;
            }
            LiveInputCommand::Snapshot(label.to_string())
        } else {
            eprintln!("live input rejected: unknown_command");
            return;
        };
        if command.is_pointer_move()
            && self
                .pending
                .back()
                .is_some_and(LiveInputCommand::is_pointer_move)
        {
            self.pending.pop_back();
            self.pending.push_back(command);
            return;
        }
        if self.pending.len() >= MAX_PENDING_COMMANDS {
            if command.is_pointer_move() {
                eprintln!("live input rejected: queue_full kind=pointer_move");
                return;
            }
            if let Some(index) = self
                .pending
                .iter()
                .position(LiveInputCommand::is_pointer_move)
            {
                self.pending.remove(index);
            } else {
                eprintln!("live input rejected: queue_full kind=critical");
                return;
            }
        }
        self.pending.push_back(command);
    }
}

#[cfg(test)]
#[path = "live_input_clock_tests/mod.rs"]
mod clock_tests;
#[cfg(test)]
mod tests {
    use super::{InputControlFile, LiveInputCommand, LiveInputController, COMPACT_AFTER_BYTES};
    use std::collections::VecDeque;
    use std::fs::{self, OpenOptions};
    use std::io::Write;
    use std::path::PathBuf;
    use std::time::Instant;

    fn controller() -> LiveInputController {
        LiveInputController {
            source: None,
            offset: 0,
            lines: Default::default(),
            pending: VecDeque::new(),
            accepted_pointer_moves: 0,
            next_poll: Instant::now(),
        }
    }

    #[test]
    fn key_diagnostics_redact_payloads_for_all_debug_formats() {
        for value in [
            "text:private-input",
            "text-hex:736563726574",
            "a",
            "ctrl+v",
            "text:line\nbreak",
            "malformed-private-input",
        ] {
            let command = LiveInputCommand::Key(value.into());
            assert_eq!(format!("{command:?}"), "Key(<redacted>)");
            assert_eq!(format!("{command:#?}"), "Key(<redacted>)");
        }
    }

    #[test]
    fn live_input_accepts_only_bounded_typed_commands() {
        let mut input = controller();
        input.source = Some(InputControlFile::from_path(PathBuf::from("input.ctl")));
        input.push_line("KEY text:abc123");
        input.push_line("POINTER click:100x200");
        input.push_line("UNKNOWN anything");
        assert_eq!(input.pending.len(), 2);
        assert!(matches!(input.pending[0], LiveInputCommand::Key(_)));
        assert!(matches!(input.pending[1], LiveInputCommand::Pointer(_)));
    }

    #[test]
    fn live_input_coalesces_adjacent_pointer_moves() {
        let mut input = controller();
        input.push_line("POINTER move:1x2");
        input.push_line("POINTER move:3x4");

        assert_eq!(input.pending.len(), 1);
        assert!(matches!(
            &input.pending[0],
            LiveInputCommand::Pointer(value) if value == "move:3x4"
        ));
    }

    #[test]
    fn live_input_evicts_move_to_preserve_critical_input_when_full() {
        let mut input = controller();
        input.push_line("POINTER move:1x2");
        for index in 1..64 {
            input.push_line(&format!("KEY text:critical-{index}"));
        }
        assert_eq!(input.pending.len(), 64);

        input.push_line("POINTER release:10x20");

        assert_eq!(input.pending.len(), 64);
        assert!(!input.pending.iter().any(LiveInputCommand::is_pointer_move));
        assert!(matches!(
            input.pending.back(),
            Some(LiveInputCommand::Pointer(value)) if value == "release:10x20"
        ));
    }

    #[test]
    fn live_input_parses_resize_command_and_rejects_malformed() {
        let mut input = controller();
        input.push_line("RESIZE 1920x1080");
        assert!(matches!(
            input.pending.back(),
            Some(LiveInputCommand::Resize {
                width: 1920,
                height: 1080,
                scale: None
            })
        ));
        let before = input.pending.len();
        input.push_line("RESIZE not-a-size");
        input.push_line("RESIZE 1920");
        assert_eq!(input.pending.len(), before, "malformed resizes are dropped");
    }

    #[test]
    fn live_input_takes_a_resize_scale_only_from_the_windows_ladder() {
        let mut input = controller();
        input.push_line("RESIZE 3456x2234@200");
        assert!(matches!(
            input.pending.back(),
            Some(LiveInputCommand::Resize {
                width: 3456,
                height: 2234,
                scale: Some(200)
            })
        ));
        for percent in [100, 125, 150, 175, 200, 225, 250, 300, 350, 400, 450, 500] {
            input.pending.clear();
            input.push_line(&format!("RESIZE 1280x800@{percent}"));
            assert!(
                matches!(
                    input.pending.back(),
                    Some(LiveInputCommand::Resize { scale: Some(p), .. }) if *p == percent
                ),
                "@{percent}"
            );
        }
        input.pending.clear();
        for line in [
            "RESIZE 1280x800@0",
            "RESIZE 1280x800@99",
            "RESIZE 1280x800@120",
            "RESIZE 1280x800@201",
            "RESIZE 1280x800@275",
            "RESIZE 1280x800@550",
            "RESIZE 1280x800@-200",
            "RESIZE 1280x800@",
            "RESIZE 1280x800@2x",
            "RESIZE 1280x800@200@200",
            "RESIZE @200",
        ] {
            input.push_line(line);
            assert!(input.pending.is_empty(), "{line} accepted");
        }
    }

    #[test]
    fn live_input_accepts_a_bounded_snapshot_label_and_rejects_an_empty_one() {
        let mut input = LiveInputController {
            source: None,
            offset: 0,
            lines: Default::default(),
            pending: VecDeque::new(),
            accepted_pointer_moves: 0,
            next_poll: Instant::now(),
        };

        input.push_line("SNAPSHOT ppi-uac");
        assert!(matches!(
            input.pending.back(),
            Some(LiveInputCommand::Snapshot(label)) if label == "ppi-uac"
        ));

        let before = input.pending.len();
        input.push_line("SNAPSHOT ");
        assert_eq!(input.pending.len(), before, "empty labels are dropped");
    }

    #[test]
    fn live_input_compacts_fully_consumed_large_control_file() {
        let path = std::env::temp_dir().join(format!(
            "bridgevm-live-input-{}-{}.ctl",
            std::process::id(),
            Instant::now().elapsed().as_nanos()
        ));
        let mut contents = b"POINTER click:1x2\n".to_vec();
        contents.resize(COMPACT_AFTER_BYTES as usize + 1, b'x');
        contents.push(b'\n');
        fs::write(&path, contents).unwrap();
        let mut input = LiveInputController {
            source: Some(InputControlFile::from_path(PathBuf::from(&path))),
            offset: 0,
            lines: Default::default(),
            pending: VecDeque::new(),
            accepted_pointer_moves: 0,
            next_poll: Instant::now(),
        };

        input.read_new_commands();
        assert_eq!(input.offset, COMPACT_AFTER_BYTES);
        assert!(fs::metadata(&path).unwrap().len() > 0);
        input.read_new_commands();

        assert_eq!(fs::metadata(&path).unwrap().len(), 0);
        assert_eq!(input.offset, 0);
        assert_eq!(input.pending.len(), 1);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn live_input_reuse_does_not_replay_commands_after_guest_reset() {
        let path = std::env::temp_dir().join(format!(
            "bridgevm-live-input-reset-{}-{}.ctl",
            std::process::id(),
            Instant::now().elapsed().as_nanos()
        ));
        fs::write(&path, b"KEY text:first\n").unwrap();
        let mut input = LiveInputController {
            source: Some(InputControlFile::from_path(PathBuf::from(&path))),
            offset: 0,
            lines: Default::default(),
            pending: VecDeque::new(),
            accepted_pointer_moves: 0,
            next_poll: Instant::now(),
        };

        input.read_new_commands();
        assert_eq!(input.pending.len(), 1);
        input.pending.clear();

        OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap()
            .write_all(b"KEY text:second\n")
            .unwrap();
        input.read_new_commands();

        assert_eq!(input.pending.len(), 1);
        assert!(matches!(
            input.pending.front(),
            Some(LiveInputCommand::Key(value)) if value == "text:second"
        ));
        fs::remove_file(path).unwrap();
    }
}
