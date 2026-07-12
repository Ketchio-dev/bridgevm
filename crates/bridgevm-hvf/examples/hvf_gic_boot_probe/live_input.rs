use std::collections::VecDeque;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use bridgevm_hvf::fwcfg::GuestMemoryMut;
use bridgevm_hvf::platform_virt::VirtPlatform;
use bridgevm_hvf::xhci::{XhciPointerInputQueueError, XhciSetupInputQueueError};

use crate::xhci_hid_input::{parse_pointer_input_actions, parse_setup_input_actions};

const POLL_INTERVAL: Duration = Duration::from_millis(16);
const MAX_PENDING_COMMANDS: usize = 64;
const MAX_COMMAND_BYTES: usize = 256;

#[derive(Debug)]
enum LiveInputCommand {
    Key(String),
    Pointer(String),
}

pub struct LiveInputController {
    path: Option<PathBuf>,
    offset: u64,
    partial: String,
    pending: VecDeque<LiveInputCommand>,
    next_poll: Instant,
}

impl LiveInputController {
    pub fn from_env() -> Self {
        Self {
            path: std::env::var_os("BRIDGEVM_INPUT_CONTROL")
                .filter(|value| !value.is_empty())
                .map(PathBuf::from),
            offset: 0,
            partial: String::new(),
            pending: VecDeque::new(),
            next_poll: Instant::now(),
        }
    }

    pub fn poll_due(&self, now: Instant) -> bool {
        self.path.is_some() && now >= self.next_poll
    }

    pub fn tick(
        &mut self,
        platform: &mut VirtPlatform,
        mem: &mut dyn GuestMemoryMut,
        now: Instant,
    ) {
        if !self.poll_due(now) {
            return;
        }
        self.next_poll = now + POLL_INTERVAL;
        self.read_new_commands();
        let Some(command) = self.pending.front() else {
            return;
        };
        let result = match command {
            LiveInputCommand::Key(value) => match parse_setup_input_actions(value) {
                Ok(actions) => platform
                    .queue_xhci_setup_input_actions_with_mem(&actions, mem)
                    .map(|()| true)
                    .map_err(|error| matches!(error, XhciSetupInputQueueError::Busy)),
                Err(error) => {
                    eprintln!("live input rejected: kind=key parse_error={}", error.name());
                    Ok(false)
                }
            },
            LiveInputCommand::Pointer(value) => match parse_pointer_input_actions(value) {
                Ok(actions) => platform
                    .queue_xhci_pointer_input_actions_with_mem(&actions, mem)
                    .map(|()| true)
                    .map_err(|error| matches!(error, XhciPointerInputQueueError::Busy)),
                Err(error) => {
                    eprintln!(
                        "live input rejected: kind=pointer parse_error={}",
                        error.name()
                    );
                    Ok(false)
                }
            },
        };
        match result {
            Ok(accepted) => {
                if accepted {
                    println!("live input accepted: command={command:?}");
                }
                self.pending.pop_front();
            }
            Err(true) => {}
            Err(false) => {
                eprintln!("live input rejected: command={command:?} queue_error=invalid");
                self.pending.pop_front();
            }
        }
    }

    fn read_new_commands(&mut self) {
        let Some(path) = self.path.as_deref() else {
            return;
        };
        let Ok(mut file) = File::open(path) else {
            return;
        };
        let Ok(len) = file.metadata().map(|metadata| metadata.len()) else {
            return;
        };
        if len < self.offset {
            self.offset = 0;
            self.partial.clear();
        }
        if len == self.offset {
            return;
        }
        if file.seek(SeekFrom::Start(self.offset)).is_err() {
            return;
        }
        let mut bytes = Vec::new();
        if file.read_to_end(&mut bytes).is_err() {
            return;
        }
        self.offset = self.offset.saturating_add(bytes.len() as u64);
        self.partial.push_str(&String::from_utf8_lossy(&bytes));
        while let Some(newline) = self.partial.find('\n') {
            let line = self.partial[..newline]
                .trim_end_matches('\r')
                .trim()
                .to_string();
            self.partial.drain(..=newline);
            self.push_line(&line);
        }
        if self.partial.len() > MAX_COMMAND_BYTES {
            self.partial.clear();
            eprintln!("live input rejected: command_too_long");
        }
    }

    fn push_line(&mut self, line: &str) {
        if line.is_empty() || line.len() > MAX_COMMAND_BYTES {
            return;
        }
        if self.pending.len() >= MAX_PENDING_COMMANDS {
            eprintln!("live input rejected: queue_full");
            return;
        }
        let command = if let Some(value) = line.strip_prefix("KEY ") {
            LiveInputCommand::Key(value.to_string())
        } else if let Some(value) = line.strip_prefix("POINTER ") {
            LiveInputCommand::Pointer(value.to_string())
        } else {
            eprintln!("live input rejected: unknown_command");
            return;
        };
        self.pending.push_back(command);
    }
}

#[cfg(test)]
mod tests {
    use super::{LiveInputCommand, LiveInputController};
    use std::collections::VecDeque;
    use std::time::Instant;

    fn controller() -> LiveInputController {
        LiveInputController {
            path: None,
            offset: 0,
            partial: String::new(),
            pending: VecDeque::new(),
            next_poll: Instant::now(),
        }
    }

    #[test]
    fn live_input_accepts_only_bounded_typed_commands() {
        let mut input = controller();
        input.push_line("KEY text:abc123");
        input.push_line("POINTER click:100x200");
        input.push_line("UNKNOWN anything");
        assert_eq!(input.pending.len(), 2);
        assert!(matches!(input.pending[0], LiveInputCommand::Key(_)));
        assert!(matches!(input.pending[1], LiveInputCommand::Pointer(_)));
    }
}
