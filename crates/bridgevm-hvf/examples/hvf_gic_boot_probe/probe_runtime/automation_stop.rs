//! The existing automation stop decisions, evaluated while the caller owns the platform.

macro_rules! automation_stop {
    ($checkpoint_committed:ident, $stop_on_linux:ident, $locked_platform:ident, $serial_stop_scans:ident, $ramfb_sample_loop:ident, $vcpu:ident $(,)?) => {{
        if $checkpoint_committed {
            Some("VM checkpoint committed; suspended process exiting".into())
        } else if $stop_on_linux
            && serial_reached_linux_early_boot(
                $locked_platform.uart_output(),
                &mut $serial_stop_scans,
            )
        {
            Some("serial reached Linux early boot".into())
        } else if serial_reached_shell($locked_platform.uart_output(), &mut $serial_stop_scans) {
            match $ramfb_sample_loop.observe_shell($vcpu) {
                RamfbSampleShellAction::Continue => None,
                RamfbSampleShellAction::StopNow { reason } => Some(reason.into()),
            }
        } else {
            None
        }
    }};
}
pub(crate) use automation_stop;
