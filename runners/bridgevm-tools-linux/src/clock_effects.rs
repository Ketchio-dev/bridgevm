//! The guest clock setter, the ClockBackend seam, and the settimeofday implementation.

use anyhow::Result;

/// Applies host TimeSync commands to the guest clock.
pub(crate) struct ClockSetter {
    pub(crate) mode: ClockSetterMode,
}

pub(crate) enum ClockSetterMode {
    /// Acknowledge the host epoch without touching the real clock (used on
    /// non-Linux builds, when --no-real-time-sync is passed, or in tests).
    Simulated,
    /// Apply the host epoch to the real guest clock through the backend.
    Real { backend: Box<dyn ClockBackend> },
}

pub(crate) trait ClockBackend {
    fn set_epoch_millis(&mut self, unix_epoch_millis: u64) -> Result<(), String>;
}

/// Real Linux backend: set the wall clock with settimeofday(2). The agent runs
/// as root under cloud-init, so CAP_SYS_TIME is available.
pub(crate) struct SettimeofdayClockBackend;

#[cfg(target_os = "linux")]
pub(crate) fn set_system_clock_millis(unix_epoch_millis: u64) -> Result<(), String> {
    let seconds = (unix_epoch_millis / 1_000) as libc::time_t;
    let micros = ((unix_epoch_millis % 1_000) * 1_000) as libc::suseconds_t;
    let tv = libc::timeval {
        tv_sec: seconds,
        tv_usec: micros,
    };
    // SAFETY: tv is a fully-initialized timeval; settimeofday reads it and does
    // not retain the pointer.
    let rc = unsafe { libc::settimeofday(&tv, std::ptr::null()) };
    if rc == 0 {
        Ok(())
    } else {
        Err(format!(
            "settimeofday failed: {}",
            std::io::Error::last_os_error()
        ))
    }
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn set_system_clock_millis(_unix_epoch_millis: u64) -> Result<(), String> {
    Err("real clock sync is only supported on Linux guests".to_string())
}

impl ClockSetter {
    pub(crate) fn simulated() -> Self {
        Self {
            mode: ClockSetterMode::Simulated,
        }
    }

    pub(crate) fn real(backend: Box<dyn ClockBackend>) -> Self {
        Self {
            mode: ClockSetterMode::Real { backend },
        }
    }

    /// Returns an optional human-readable message on success.
    pub(crate) fn set_epoch_millis(
        &mut self,
        unix_epoch_millis: u64,
    ) -> Result<Option<String>, String> {
        match &mut self.mode {
            ClockSetterMode::Simulated => Ok(Some(format!(
                "acknowledged time-sync to {unix_epoch_millis} ms since epoch; guest clock was not changed (simulated)"
            ))),
            ClockSetterMode::Real { backend } => {
                backend.set_epoch_millis(unix_epoch_millis)?;
                Ok(Some(format!(
                    "set guest clock to {unix_epoch_millis} ms since epoch"
                )))
            }
        }
    }
}

impl ClockBackend for SettimeofdayClockBackend {
    fn set_epoch_millis(&mut self, unix_epoch_millis: u64) -> Result<(), String> {
        set_system_clock_millis(unix_epoch_millis)
    }
}
