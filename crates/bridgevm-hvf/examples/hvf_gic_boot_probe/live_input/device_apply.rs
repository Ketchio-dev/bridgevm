//! Apply at most one prepared input command while holding the platform.

use super::key_actions::{self, KeyCommand};
use super::{LiveInputCommand, LiveInputController};
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
        // A busy xHCI queue keeps the front KEY/POINTER command until the guest
        // reads its reports. The power button must not wait behind input the
        // guest is not reading (early boot, xHCI off), so it is pressed first.
        while let Some(index) = self
            .pending
            .iter()
            .position(LiveInputCommand::is_power_button)
        {
            self.pending.remove(index);
            press_power_button(platform);
        }
        // A reset ends this boot and a pause stops it: the run loop takes
        // either after this tick, and queued input waits until then.
        if self.pending.iter().any(LiveInputCommand::is_run_control) {
            return;
        }
        let Some(command) = self.pending.front() else {
            return;
        };
        platform.set_host_now(now);
        let result = match command {
            LiveInputCommand::Key(value) => match key_actions::parse_command(value) {
                Ok(KeyCommand::Tap(actions)) => platform
                    .queue_xhci_setup_input_actions_with_mem(&actions, mem)
                    .map(|()| true)
                    .map_err(|error| matches!(error, XhciSetupInputQueueError::Busy)),
                // A press already held, or a release of a key not held, is accepted and changes nothing.
                Ok(KeyCommand::Hold { action, down }) => platform
                    .queue_xhci_setup_input_hold_with_mem(action, down, mem)
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
            LiveInputCommand::Resize {
                width,
                height,
                scale,
            } => {
                let size = (*width, *height);
                platform.request_virtio_gpu_resolution(size.0, size.1);
                // The device now reports the size (also when it already did);
                // Windows only switches when the guest agent asks it to.
                if platform.virtio_gpu_resolution() == Some(size) {
                    crate::agent_console::request_desktop_mode(size);
                    match scale {
                        // The agent sets it once the desktop has switched.
                        Some(percent) => {
                            crate::agent_console::request_desktop_scale(*percent);
                            println!("live input accepted: resize={width}x{height}@{percent}");
                        }
                        None => println!("live input accepted: resize={width}x{height}"),
                    }
                } else {
                    eprintln!(
                        "live input rejected: kind=resize {width}x{height} (no-gpu/out-of-range)"
                    );
                }
                Ok(false)
            }
            LiveInputCommand::Snapshot(label) => {
                crate::ramfb_dump::print_checkpoint_for_platform(label, platform, mem);
                println!("live input accepted: snapshot={label:?}");
                Ok(false)
            }
            LiveInputCommand::PowerButton | LiveInputCommand::Reset | LiveInputCommand::Pause(_) => {
                unreachable!("host control lines are handled above")
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

impl LiveInputController {
    /// True when a RESET line was read since the last call: the run loop then
    /// ends the boot as a guest SYSTEM_RESET does, so the runtime recreates
    /// the helper.
    pub fn take_host_reset(&mut self) -> bool {
        let before = self.pending.len();
        self.pending.retain(|command| !command.is_reset());
        if self.pending.len() == before {
            return false;
        }
        let t = super::STARTED.get().map_or(0, |at| at.elapsed().as_millis());
        println!("BVPOWER host reset t={t}");
        true
    }

    /// The last PAUSE (`Some(true)`) or RESUME (`Some(false)`) read since the
    /// last call, if any; the run loop pauses or resumes every vCPU.
    pub fn take_host_pause(&mut self) -> Option<bool> {
        let mut last = None;
        self.pending.retain(|command| match command {
            LiveInputCommand::Pause(paused) => {
                last = Some(*paused);
                false
            }
            _ => true,
        });
        last
    }
}

fn press_power_button(platform: &mut VirtPlatform) {
    platform.press_power_button();
    let t = super::STARTED.get().map_or(0, |at| at.elapsed().as_millis());
    println!("BVPOWER button pressed t={t}");
    use std::io::Write as _;
    let _ = std::io::stdout().flush();
}
