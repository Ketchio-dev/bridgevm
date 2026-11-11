use std::collections::VecDeque;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

#[path = "input_control_file.rs"]
mod input_control_file;
pub(crate) use input_control_file::InputControlFile;
const POLL_INTERVAL: Duration = Duration::from_millis(16);
const MAX_PENDING_COMMANDS: usize = 64;
use line_frames::MAX_COMMAND_BYTES;
const MAX_READ_BYTES_PER_TICK: u64 = 1024 * 1024;
const COMPACT_AFTER_BYTES: u64 = 1024 * 1024;
/// When the VMM opened its live-input channel; the `t=` of `BVPOWER` lines.
static STARTED: OnceLock<Instant> = OnceLock::new();
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
#[cfg(test)]
#[path = "live_input/drain_line_tests.rs"]
mod drain_line_tests;
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
    /// One press of the ACPI power button; needs no guest agent.
    PowerButton,
    /// A hard reset of the VM, the way a guest PSCI SYSTEM_RESET ends a boot;
    /// needs no guest agent. The run loop takes it (`take_host_reset`).
    Reset,
    /// Stop (`Pause(true)`) or continue every vCPU; the run loop takes it
    /// (`take_host_pause`, pause_gate.rs).
    Pause(bool),
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
            Self::PowerButton => f.write_str("PowerButton"),
            Self::Reset => f.write_str("Reset"),
            Self::Pause(paused) => f.debug_tuple("Pause").field(paused).finish(),
        }
    }
}

impl LiveInputCommand {
    fn is_pointer_move(&self) -> bool {
        matches!(self, Self::Pointer(value) if value.starts_with("move:"))
    }

    fn is_power_button(&self) -> bool {
        matches!(self, Self::PowerButton)
    }

    fn is_reset(&self) -> bool {
        matches!(self, Self::Reset)
    }

    /// Taken by the run loop, never applied to a device.
    fn is_run_control(&self) -> bool {
        matches!(self, Self::Reset | Self::Pause(_))
    }

    /// Lines for the VMM itself that must not wait behind guest input.
    fn host_control(line: &[u8]) -> Option<Self> {
        match line {
            b"POWERBUTTON" => Some(Self::PowerButton),
            b"RESET" => Some(Self::Reset),
            b"PAUSE" => Some(Self::Pause(true)),
            b"RESUME" => Some(Self::Pause(false)),
            _ => None,
        }
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
    /// File offset up to which a full-queue read-ahead already queued every
    /// host control line (POWERBUTTON, RESET, PAUSE, RESUME).
    power_scanned_to: u64,
}

impl LiveInputController {
    pub fn from_env() -> Self {
        STARTED.get_or_init(Instant::now);
        let mut source = InputControlFile::from_env();
        Self {
            offset: restart_resume::initial_offset(source.as_mut()),
            source,
            lines: Default::default(),
            power_scanned_to: 0,
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

    /// With a full queue this still reads ahead for POWERBUTTON and RESET lines.
    fn read_new_commands(&mut self) {
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
        } else if let Some(command) = LiveInputCommand::host_control(line.as_bytes()) {
            command
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
            power_scanned_to: 0,
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
    fn live_input_queues_one_power_button_press_per_line() {
        let mut input = controller();
        input.push_line("POWERBUTTON");
        input.push_line("POWERBUTTON now");
        assert_eq!(input.pending.len(), 1);
        assert!(matches!(input.pending[0], LiveInputCommand::PowerButton));

        let mut platform = bridgevm_hvf::platform_virt::VirtPlatform::new(Default::default());
        let mut mem = bridgevm_hvf::platform_virt::FlatGuestRam::new(0x4000_0000, 0);
        input.apply_pending(&mut platform, &mut mem, Instant::now());
        assert!(input.pending.is_empty());
        assert_eq!(platform.take_pending_spi_levels(), [(41, true)]);
    }

    #[test]
    fn power_button_does_not_wait_behind_input_the_guest_is_not_reading() {
        use bridgevm_hvf::platform_virt::{
            FlatGuestRam, VirtPlatform, VirtPlatformConfig, VirtPlatformDeviceConfig,
        };
        // Without xHCI every KEY is Busy, so it stays at the queue front.
        let mut platform = VirtPlatform::new_with_config(VirtPlatformConfig {
            fdt: Default::default(),
            devices: VirtPlatformDeviceConfig {
                xhci_present: false,
                ..Default::default()
            },
        });
        let mut mem = FlatGuestRam::new(0x4000_0000, 0);
        let mut input = controller();
        input.push_line("KEY enter");
        input.push_line("POWERBUTTON");
        input.apply_pending(&mut platform, &mut mem, Instant::now());
        input.apply_pending(&mut platform, &mut mem, Instant::now());

        assert_eq!(platform.take_pending_spi_levels(), [(41, true)]);
        assert_eq!(input.pending.len(), 1);
        assert!(matches!(input.pending[0], LiveInputCommand::Key(_)));
    }

    #[test]
    fn power_button_is_read_past_a_full_queue_and_pressed_once() {
        use bridgevm_hvf::platform_virt::{
            FlatGuestRam, VirtPlatform, VirtPlatformConfig, VirtPlatformDeviceConfig,
        };
        let path = std::env::temp_dir().join(format!(
            "bridgevm-live-input-power-{}-{}.ctl",
            std::process::id(),
            Instant::now().elapsed().as_nanos()
        ));
        // The guest reads no HID (xHCI off), so 64 clicks fill the queue first.
        let mut contents = String::new();
        for index in 0..super::MAX_PENDING_COMMANDS {
            contents.push_str(&format!("POINTER click:{index}x1\n"));
        }
        contents.push_str("POWERBUTTON\n");
        fs::write(&path, contents).unwrap();
        let mut platform = VirtPlatform::new_with_config(VirtPlatformConfig {
            fdt: Default::default(),
            devices: VirtPlatformDeviceConfig {
                xhci_present: false,
                ..Default::default()
            },
        });
        let mut mem = FlatGuestRam::new(0x4000_0000, 0);
        let mut input = controller();
        input.source = Some(InputControlFile::from_path(path.clone()));

        input.read_new_commands();
        input.apply_pending(&mut platform, &mut mem, Instant::now());
        assert_eq!(platform.take_pending_spi_levels(), [(41, true)]);

        // When the queue moves again, the reader skips the line it already pressed.
        input.pending.clear();
        input.read_new_commands();
        input.apply_pending(&mut platform, &mut mem, Instant::now());
        assert!(platform.take_pending_spi_levels().is_empty());
        assert!(input.pending.is_empty());
        fs::remove_file(path).unwrap();
    }

    /// The app's Reset: one exact `RESET` line ends the boot once, and input
    /// queued before it does not reach the guest first.
    #[test]
    fn reset_line_is_taken_once_ahead_of_queued_input() {
        let mut platform = bridgevm_hvf::platform_virt::VirtPlatform::new(Default::default());
        let mut mem = bridgevm_hvf::platform_virt::FlatGuestRam::new(0x4000_0000, 0);
        let mut input = controller();
        input.push_line("POINTER click:1x1");
        input.push_line("RESET now");
        input.push_line("RESET");
        input.apply_pending(&mut platform, &mut mem, Instant::now());
        assert_eq!(input.pending.len(), 2, "the click waits behind the reset");

        assert!(input.take_host_reset());
        assert!(!input.take_host_reset());
        assert_eq!(input.pending.len(), 1);
        assert!(matches!(input.pending[0], LiveInputCommand::Pointer(_)));
    }

    /// PAUSE and RESUME are exact lines; the last one read wins, and queued
    /// input waits behind them for the run loop.
    #[test]
    fn pause_and_resume_lines_are_taken_by_the_run_loop() {
        let mut platform = bridgevm_hvf::platform_virt::VirtPlatform::new(Default::default());
        let mut mem = bridgevm_hvf::platform_virt::FlatGuestRam::new(0x4000_0000, 0);
        let mut input = controller();
        input.push_line("PAUSE now");
        input.push_line("POINTER click:1x1");
        input.push_line("PAUSE");
        input.apply_pending(&mut platform, &mut mem, Instant::now());
        assert_eq!(input.pending.len(), 2, "the click waits");
        assert_eq!(input.take_host_pause(), Some(true));
        assert_eq!(input.take_host_pause(), None);

        input.push_line("RESUME");
        input.push_line("PAUSE");
        input.push_line("RESUME");
        assert_eq!(input.take_host_pause(), Some(false));
        assert_eq!(input.pending.len(), 1);
        assert!(matches!(input.pending[0], LiveInputCommand::Pointer(_)));
    }

    /// RESUME must reach a paused VM even behind a full queue of input.
    #[test]
    fn resume_is_read_past_a_full_queue() {
        let path = std::env::temp_dir().join(format!(
            "bridgevm-live-input-resume-{}-{}.ctl",
            std::process::id(),
            Instant::now().elapsed().as_nanos()
        ));
        let mut contents = String::new();
        for index in 0..super::MAX_PENDING_COMMANDS {
            contents.push_str(&format!("POINTER click:{index}x1\n"));
        }
        contents.push_str("RESUME\n");
        fs::write(&path, contents).unwrap();
        let mut input = controller();
        input.source = Some(InputControlFile::from_path(path.clone()));
        input.read_new_commands();
        assert_eq!(input.take_host_pause(), Some(false));
        fs::remove_file(path).unwrap();
    }

    /// A hung guest reads no HID, which is when Reset is needed most.
    #[test]
    fn reset_is_read_past_a_full_queue_and_taken_once() {
        let path = std::env::temp_dir().join(format!(
            "bridgevm-live-input-reset-{}-{}.ctl",
            std::process::id(),
            Instant::now().elapsed().as_nanos()
        ));
        let mut contents = String::new();
        for index in 0..super::MAX_PENDING_COMMANDS {
            contents.push_str(&format!("POINTER click:{index}x1\n"));
        }
        contents.push_str("RESET\n");
        fs::write(&path, contents).unwrap();
        let mut input = controller();
        input.source = Some(InputControlFile::from_path(path.clone()));

        input.read_new_commands();
        assert!(input.take_host_reset());

        // When the queue moves again, the reader skips the line it already took.
        input.pending.clear();
        input.read_new_commands();
        assert!(!input.take_host_reset());
        assert!(input.pending.is_empty());
        fs::remove_file(path).unwrap();
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
            power_scanned_to: 0,
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
            power_scanned_to: 0,
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
            power_scanned_to: 0,
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
