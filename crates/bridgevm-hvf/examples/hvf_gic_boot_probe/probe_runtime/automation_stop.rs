//! The existing automation stop decisions. The serial scans run while the
//! caller owns the platform; the UEFI-shell decision, which may print, runs
//! after that pass's framebuffer checkpoints have printed.

use crate::*;

pub(crate) enum AutomationStop {
    None,
    Checkpoint,
    LinuxEarlyBoot,
    Shell,
}

impl AutomationStop {
    /// Under the platform lock: `serial` is its UART output.
    pub(crate) fn check(
        checkpoint_committed: bool,
        stop_on_linux: bool,
        serial: &[u8],
        scans: &mut SerialStopScans,
    ) -> Self {
        if checkpoint_committed {
            Self::Checkpoint
        } else if stop_on_linux && serial_reached_linux_early_boot(serial, scans) {
            Self::LinuxEarlyBoot
        } else if serial_reached_shell(serial, scans) {
            Self::Shell
        } else {
            Self::None
        }
    }

    pub(crate) fn reason(
        self,
        ramfb_sample_loop: &mut RamfbSampleLoop,
        vcpu: HvVcpuT,
    ) -> Option<String> {
        match self {
            Self::None => None,
            Self::Checkpoint => Some("VM checkpoint committed; suspended process exiting".into()),
            Self::LinuxEarlyBoot => Some("serial reached Linux early boot".into()),
            Self::Shell => match ramfb_sample_loop.observe_shell(vcpu) {
                RamfbSampleShellAction::Continue => None,
                RamfbSampleShellAction::StopNow { reason } => Some(reason.into()),
            },
        }
    }
}
