//! Apply at most one prepared input command while holding the platform.

use super::{key_actions, LiveInputCommand, LiveInputController};
use crate::xhci_hid_input::parse_pointer_input_actions;
use bridgevm_hvf::fwcfg::GuestMemoryMut;
use bridgevm_hvf::platform_virt::VirtPlatform;
use bridgevm_hvf::xhci::{XhciPointerInputQueueError, XhciSetupInputQueueError};
use std::time::Instant;

impl LiveInputController {
    /// `now` is sampled after reacquiring the platform, independently of file I/O.
    pub fn apply_pending(
        &mut self,
        platform: &mut VirtPlatform,
        mem: &mut dyn GuestMemoryMut,
        now: Instant,
    ) {
        let Some(command) = self.pending.front() else {
            return;
        };
        platform.set_host_now(now);
        let result = match command {
            LiveInputCommand::Key(value) => match key_actions::parse(value) {
                Ok(actions) => platform
                    .queue_xhci_setup_input_actions_with_mem(&actions, mem)
                    .map(|()| true)
                    .map_err(|error| matches!(error, XhciSetupInputQueueError::Busy)),
                Err(error) => {
                    eprintln!("live input rejected: kind=key parse_error={}", error);
                    Ok(false)
                }
            },
            LiveInputCommand::Pointer(value) => match parse_pointer_input_actions(value) {
                Ok(actions) => {
                    { platform.queue_xhci_pointer_input_actions_with_mem(&actions, mem) }
                        .map(|()| true)
                        .map_err(|error| matches!(error, XhciPointerInputQueueError::Busy))
                }
                Err(error) => {
                    eprintln!(
                        "live input rejected: kind=pointer parse_error={}",
                        error.name()
                    );
                    Ok(false)
                }
            },
            LiveInputCommand::Resize { width, height } => {
                if platform.request_virtio_gpu_resolution(*width, *height) {
                    println!("live input accepted: resize={width}x{height}");
                } else {
                    eprintln!(
                        "live input rejected: kind=resize {width}x{height} (no-op/no-gpu/out-of-range)"
                    );
                }
                Ok(false)
            }
            LiveInputCommand::Snapshot(label) => {
                crate::ramfb_dump::print_checkpoint_for_platform(label, platform, mem);
                println!("live input accepted: snapshot={label:?}");
                Ok(false)
            }
        };
        match result {
            Ok(accepted) => {
                if accepted {
                    if command.is_pointer_move() {
                        self.accepted_pointer_moves = self.accepted_pointer_moves.saturating_add(1);
                        if self.accepted_pointer_moves % 1024 == 0 {
                            println!(
                                "live input accepted: kind=pointer_move total={}",
                                self.accepted_pointer_moves
                            );
                        }
                    } else {
                        println!("live input accepted: command={command:?}");
                    }
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
}
