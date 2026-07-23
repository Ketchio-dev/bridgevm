//! Time-sync, clipboard, display-resize, freeze/thaw and benchmark command handlers.

use crate::*;
use bridgevm_agent_protocol::DEFAULT_BENCHMARK_DURATION_MILLIS;
use bridgevm_agent_protocol::MAX_BENCHMARK_DURATION_MILLIS;
use std::time::Duration;

impl GuestToolsState {
    pub(crate) fn sync_time(&mut self, unix_epoch_millis: u64) -> CommandOutcome {
        if !self.time_sync_supported {
            return CommandOutcome::error(
                "capability-not-enabled",
                "time-sync capability is not enabled",
            );
        }

        match self.clock_setter.set_epoch_millis(unix_epoch_millis) {
            Ok(message) => CommandOutcome {
                ok: true,
                error_code: None,
                message,
                result: Some(serde_json::json!({
                    "applied_unix_epoch_millis": unix_epoch_millis,
                })),
                metadata: None,
            },
            Err(message) => CommandOutcome::error("time-sync-failed", message),
        }
    }

    pub(crate) fn freeze_filesystem(&mut self, timeout_millis: Option<u64>) -> CommandOutcome {
        if !self.fs_freeze_supported {
            return CommandOutcome::error(
                "capability-not-enabled",
                "filesystem freeze capability is not enabled",
            );
        }
        if self.filesystem_frozen {
            return CommandOutcome::error(
                "filesystem-already-frozen",
                "filesystem freeze scaffold boundary is already active",
            );
        }

        match self.filesystem_freezer.freeze(timeout_millis) {
            Ok(message) => {
                self.filesystem_frozen = true;
                CommandOutcome::ok(Some(message))
            }
            Err(message) => CommandOutcome::error("filesystem-freeze-failed", message),
        }
    }

    pub(crate) fn thaw_filesystem(&mut self) -> CommandOutcome {
        if !self.fs_thaw_supported {
            return CommandOutcome::error(
                "capability-not-enabled",
                "filesystem thaw capability is not enabled",
            );
        }
        if !self.filesystem_frozen {
            return CommandOutcome::error(
                "filesystem-not-frozen",
                "filesystem thaw scaffold boundary is not active",
            );
        }

        match self.filesystem_freezer.thaw() {
            Ok(message) => {
                self.filesystem_frozen = false;
                CommandOutcome::ok(Some(message))
            }
            Err(message) => CommandOutcome::error("filesystem-thaw-failed", message),
        }
    }

    pub(crate) fn run_benchmark(&mut self, duration_millis: Option<u64>) -> CommandOutcome {
        if !self.benchmark_supported {
            return CommandOutcome::error(
                "capability-not-enabled",
                "benchmark capability is not enabled",
            );
        }

        // Clamp the requested budget into [1, MAX] and default when absent. The
        // protocol already rejects an explicit out-of-bounds value, but we clamp
        // again here so a future caller (or a value that slipped past
        // validation) can never make the guest run an unbounded benchmark.
        let budget_millis = duration_millis
            .unwrap_or(DEFAULT_BENCHMARK_DURATION_MILLIS)
            .clamp(1, MAX_BENCHMARK_DURATION_MILLIS);

        let report = run_cpu_benchmark(Duration::from_millis(budget_millis));
        let mut payload = serde_json::json!({
            "requested_duration_millis": duration_millis,
            "budget_duration_millis": budget_millis,
            "cpu": {
                "iterations": report.iterations,
                "elapsed_millis": report.elapsed_millis,
                "ops_per_sec": report.ops_per_sec,
                "checksum": report.checksum,
            },
        });

        // Optional tiny, bounded disk write+fsync micro-benchmark. Only runs
        // when a file-drop directory was configured as a safe scratch location;
        // otherwise CPU-only (which is an acceptable result). The temp file is a
        // fixed small size and is always removed.
        if let Some(scratch_dir) = self.file_drop_dir.clone() {
            match run_disk_benchmark(&scratch_dir) {
                Ok(disk) => {
                    payload["disk"] = serde_json::json!({
                        "bytes_written": disk.bytes_written,
                        "elapsed_millis": disk.elapsed_millis,
                        "mib_per_sec": disk.mib_per_sec,
                    });
                }
                Err(error) => {
                    payload["disk_error"] = serde_json::Value::String(error);
                }
            }
        }

        CommandOutcome::ok_with_result(
            Some(format!(
                "ran benchmark for {budget_millis} ms ({} cpu iterations, {} ops/sec)",
                report.iterations, report.ops_per_sec
            )),
            payload,
        )
    }

    pub(crate) fn set_clipboard(&mut self, text: &str) -> CommandOutcome {
        if !self.clipboard_supported {
            return CommandOutcome::error(
                "capability-not-enabled",
                "clipboard capability is not enabled",
            );
        }

        match self.clipboard_writer.write_text(text) {
            Ok(message) => CommandOutcome::ok(message),
            Err(message) => CommandOutcome::error("clipboard-write-failed", message),
        }
    }

    pub(crate) fn resize_display(&mut self, width: u32, height: u32, scale: u16) -> CommandOutcome {
        if !self.display_resize_supported {
            return CommandOutcome::error(
                "capability-not-enabled",
                "display resize capability is not enabled",
            );
        }

        match self.display_resizer.resize(width, height, scale) {
            Ok(message) => CommandOutcome::ok(message),
            Err(message) => CommandOutcome::error("display-resize-failed", message),
        }
    }
}
