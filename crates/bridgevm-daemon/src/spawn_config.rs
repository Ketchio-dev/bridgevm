//! Environment-driven spawn configuration for Fast Mode runners and extra QEMU args.

use crate::*;
use anyhow::Context;
use anyhow::Result;
use std::env;
use std::path::Path;
use std::path::PathBuf;

pub(crate) struct FastModeSpawnConfig {
    pub(crate) lightvm_runner: PathBuf,
    pub(crate) apple_vz_runner: PathBuf,
    pub(crate) stop_after_seconds: Option<u64>,
    pub(crate) force_stop_grace_seconds: Option<u64>,
    pub(crate) verify_apple_vz_runner_entitlement: bool,
}

pub(crate) fn env_flag_enabled(name: &str) -> bool {
    env::var(name)
        .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(false)
}

/// Test-only extra QEMU args for daemon-spawned Compatibility backends.
///
/// Read from `BRIDGEVM_COMPAT_EXTRA_QEMU_ARGS` and shell-word split. This is an
/// integration-test seam (e.g. attaching a NoCloud cidata seed ISO for the
/// application-consistent live opt-in smoke) and is unset in normal operation.
pub(crate) fn compat_extra_qemu_args() -> Vec<String> {
    match env::var("BRIDGEVM_COMPAT_EXTRA_QEMU_ARGS") {
        Ok(value) => shell_word_split(&value),
        Err(_) => Vec::new(),
    }
}

/// Minimal POSIX-ish shell word splitter supporting single and double quotes.
/// Sufficient for passing QEMU `-drive file=...,...` style args from tests.
pub(crate) fn shell_word_split(input: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut in_word = false;
    let mut quote: Option<char> = None;
    let mut chars = input.chars();
    while let Some(c) = chars.next() {
        match quote {
            Some(q) => {
                if c == q {
                    quote = None;
                } else if q == '"' && c == '\\' {
                    if let Some(next) = chars.next() {
                        current.push(next);
                    }
                } else {
                    current.push(c);
                }
            }
            None => {
                if c == '\'' || c == '"' {
                    quote = Some(c);
                    in_word = true;
                } else if c == '\\' {
                    if let Some(next) = chars.next() {
                        current.push(next);
                        in_word = true;
                    }
                } else if c.is_whitespace() {
                    if in_word {
                        words.push(std::mem::take(&mut current));
                        in_word = false;
                    }
                } else {
                    current.push(c);
                    in_word = true;
                }
            }
        }
    }
    if in_word {
        words.push(current);
    }
    words
}

pub(crate) fn env_optional_u64(name: &str) -> Result<Option<u64>> {
    let Some(value) = env::var(name).ok().filter(|value| !value.trim().is_empty()) else {
        return Ok(None);
    };
    let parsed = value
        .parse::<u64>()
        .with_context(|| format!("{name} must be a positive integer"))?;
    if parsed == 0 {
        anyhow::bail!("{name} must be a positive integer");
    }
    Ok(Some(parsed))
}

impl FastModeSpawnConfig {
    pub(crate) fn from_env() -> Result<Option<Self>> {
        if !env_flag_enabled("BRIDGEVM_APPLE_VZ_ALLOW_REAL_START") {
            return Ok(None);
        }
        let apple_vz_runner = if let Some(path) =
            env::var_os("BRIDGEVM_APPLE_VZ_RUNNER").map(PathBuf::from)
        {
            path
        } else if let Some(path) = bundled_helper_path("AppleVzRunner") {
            path
        } else {
            anyhow::bail!("BRIDGEVM_APPLE_VZ_ALLOW_REAL_START=1 requires BRIDGEVM_APPLE_VZ_RUNNER");
        };
        let lightvm_runner = env::var_os("BRIDGEVM_LIGHTVM_RUNNER")
            .map(PathBuf::from)
            .or_else(|| bundled_helper_path("lightvm-runner"))
            .unwrap_or_else(|| PathBuf::from("lightvm-runner"));

        Ok(Some(Self {
            lightvm_runner,
            apple_vz_runner,
            stop_after_seconds: env_optional_u64("BRIDGEVM_APPLE_VZ_STOP_AFTER_SECONDS")?,
            force_stop_grace_seconds: env_optional_u64(
                "BRIDGEVM_APPLE_VZ_FORCE_STOP_GRACE_SECONDS",
            )?,
            verify_apple_vz_runner_entitlement: true,
        }))
    }

    pub(crate) fn validate(&self) -> Result<()> {
        require_executable(
            &self.lightvm_runner,
            "BRIDGEVM_LIGHTVM_RUNNER/lightvm-runner",
        )?;
        require_executable(
            &self.apple_vz_runner,
            "BRIDGEVM_APPLE_VZ_RUNNER/AppleVzRunner",
        )?;
        if self.verify_apple_vz_runner_entitlement {
            verify_apple_vz_runner_entitlement(&self.apple_vz_runner)?;
        }
        Ok(())
    }

    /// Build the `lightvm-runner` argv, optionally restoring a saved Apple VZ
    /// machine state (`--apple-vz-restore-state`) for a Fast Mode resume.
    pub(crate) fn runner_args_with_restore(
        &self,
        launch_spec_path: &Path,
        restore_state: Option<&Path>,
    ) -> Vec<String> {
        let mut args = vec![
            "--launch-spec".to_string(),
            launch_spec_path.display().to_string(),
            "--require-ready".to_string(),
            "--launch".to_string(),
            "--apple-vz-runner".to_string(),
            self.apple_vz_runner.display().to_string(),
            "--apple-vz-allow-real-start".to_string(),
        ];
        if let Some(state_path) = restore_state {
            args.push("--apple-vz-restore-state".to_string());
            args.push(state_path.display().to_string());
        }
        if let Some(seconds) = self.stop_after_seconds {
            args.push("--apple-vz-stop-after-seconds".to_string());
            args.push(seconds.to_string());
        }
        if let Some(seconds) = self.force_stop_grace_seconds {
            args.push("--apple-vz-force-stop-grace-seconds".to_string());
            args.push(seconds.to_string());
        }
        args
    }
}
