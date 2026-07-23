//! Continuation of the `xdotool_button` impl block, split for the 1000-line rule.

use super::*;

use bridgevm_agent_protocol::WindowInputEvent;
use bridgevm_agent_protocol::DEFAULT_BENCHMARK_DURATION_MILLIS;
use bridgevm_agent_protocol::MAX_BENCHMARK_DURATION_MILLIS;
use std::fs;
use std::time::Duration;

impl GuestToolsState {
    pub(crate) fn complete_file_drop(&mut self, transfer_id: &str) -> CommandOutcome {
        if !self.drag_drop_supported {
            return CommandOutcome::error(
                "capability-not-enabled",
                "drag-and-drop capability is not enabled",
            );
        }
        let Some(transfer) = self.file_drops.get(transfer_id) else {
            return CommandOutcome::error(
                "transfer-not-started",
                format!("file drop {transfer_id} has not started"),
            );
        };
        if transfer.bytes.len() as u64 != transfer.size_bytes {
            return CommandOutcome::error(
                "transfer-size-mismatch",
                format!(
                    "file drop {} expected {} bytes but received {}",
                    transfer.file_name,
                    transfer.size_bytes,
                    transfer.bytes.len()
                ),
            );
        }
        if let Some(file_drop_dir) = &self.file_drop_dir {
            let Some(destination) = safe_file_drop_destination(file_drop_dir, &transfer.file_name)
            else {
                return CommandOutcome::error(
                    "unsafe-file-name",
                    format!("file drop file name is not safe: {}", transfer.file_name),
                );
            };
            if let Err(error) = fs::create_dir_all(file_drop_dir) {
                return CommandOutcome::error(
                    "file-drop-write-failed",
                    format!(
                        "failed to create file drop directory {}: {error}",
                        file_drop_dir.display()
                    ),
                );
            }
            if let Err(error) = fs::write(&destination, &transfer.bytes) {
                return CommandOutcome::error(
                    "file-drop-write-failed",
                    format!(
                        "failed to write file drop {}: {error}",
                        destination.display()
                    ),
                );
            }
        }
        let transfer = self
            .file_drops
            .remove(transfer_id)
            .expect("transfer was checked above");

        let mut message = format!(
            "completed file drop {} ({} bytes across {} chunks)",
            transfer.file_name, transfer.size_bytes, transfer.chunks_seen
        );
        if let Some(file_drop_dir) = &self.file_drop_dir {
            if let Some(destination) =
                safe_file_drop_destination(file_drop_dir, &transfer.file_name)
            {
                message.push_str(&format!(" at {}", destination.display()));
            }
        }
        CommandOutcome::ok(Some(message))
    }

    pub(crate) fn list_applications(&self) -> CommandOutcome {
        if !self.applications_supported {
            return CommandOutcome::error(
                "capability-not-enabled",
                "applications capability is not enabled",
            );
        }

        if let Some(outcome) = self.desktop_controller.list_applications() {
            return outcome;
        }

        let names = self
            .applications
            .iter()
            .map(|(id, app)| format!("{id}:{}", app.name))
            .collect::<Vec<_>>()
            .join(",");
        let applications = self
            .applications
            .iter()
            .map(|(id, app)| {
                serde_json::json!({
                    "id": id,
                    "name": app.name,
                    "launched": app.launched
                })
            })
            .collect::<Vec<_>>();
        CommandOutcome::ok_with_result(
            Some(format!("applications: {names}")),
            serde_json::json!({ "applications": applications }),
        )
    }

    pub(crate) fn launch_application(&mut self, id: &str) -> CommandOutcome {
        if !self.applications_supported {
            return CommandOutcome::error(
                "capability-not-enabled",
                "applications capability is not enabled",
            );
        }
        if let Some(outcome) = self.desktop_controller.launch_application(id) {
            return outcome;
        }
        let Some(app) = self.applications.get_mut(id) else {
            return CommandOutcome::error(
                "application-not-found",
                format!("application {id} was not found"),
            );
        };

        app.launched = true;
        CommandOutcome::ok_with_result(
            Some(format!(
                "accepted launch request for application {}",
                app.name
            )),
            serde_json::json!({
                "application": {
                    "id": id,
                    "name": app.name,
                    "launched": app.launched
                }
            }),
        )
    }

    pub(crate) fn list_windows(&self) -> CommandOutcome {
        if !self.windows_supported {
            return CommandOutcome::error(
                "capability-not-enabled",
                "windows capability is not enabled",
            );
        }

        if let Some(outcome) = self.desktop_controller.list_windows() {
            return outcome;
        }

        let windows = self
            .windows
            .iter()
            .filter(|(_, window)| !window.closed)
            .map(|(id, window)| format!("{id}:{}", window.title))
            .collect::<Vec<_>>()
            .join(",");
        let window_payload = self
            .windows
            .iter()
            .filter(|(_, window)| !window.closed)
            .map(|(id, window)| {
                let mut payload = window_entry_payload(id, window);
                payload["focused"] = serde_json::Value::Bool(window.focused);
                payload
            })
            .collect::<Vec<_>>();
        CommandOutcome::ok_with_result(
            Some(format!("windows: {windows}")),
            serde_json::json!({ "windows": window_payload }),
        )
    }

    pub(crate) fn focus_window(&mut self, id: &str) -> CommandOutcome {
        if !self.windows_supported {
            return CommandOutcome::error(
                "capability-not-enabled",
                "windows capability is not enabled",
            );
        }
        if let Some(outcome) = self.desktop_controller.focus_window(id) {
            return outcome;
        }
        if !self.windows.get(id).is_some_and(|window| !window.closed) {
            return CommandOutcome::error("window-not-found", format!("window {id} was not found"));
        }

        for window in self.windows.values_mut() {
            window.focused = false;
        }
        let window = self.windows.get_mut(id).expect("window checked above");
        window.focused = true;
        let mut window_payload = window_entry_payload(id, window);
        window_payload["focused"] = serde_json::Value::Bool(window.focused);
        CommandOutcome::ok_with_result(
            Some(format!(
                "accepted focus request for window {}",
                window.title
            )),
            serde_json::json!({ "window": window_payload }),
        )
    }

    pub(crate) fn close_window(&mut self, id: &str) -> CommandOutcome {
        if !self.windows_supported {
            return CommandOutcome::error(
                "capability-not-enabled",
                "windows capability is not enabled",
            );
        }
        if let Some(outcome) = self.desktop_controller.close_window(id) {
            return outcome;
        }
        let Some(window) = self.windows.get_mut(id) else {
            return CommandOutcome::error("window-not-found", format!("window {id} was not found"));
        };
        if window.closed {
            return CommandOutcome::error("window-not-found", format!("window {id} was not found"));
        }

        window.closed = true;
        window.focused = false;
        let mut window_payload = window_entry_payload(id, window);
        window_payload["closed"] = serde_json::Value::Bool(window.closed);
        CommandOutcome::ok_with_result(
            Some(format!("closed window {}", window.title)),
            serde_json::json!({ "window": window_payload }),
        )
    }

    pub(crate) fn set_window_bounds(
        &mut self,
        id: &str,
        x: i64,
        y: i64,
        width: u64,
        height: u64,
    ) -> CommandOutcome {
        if !self.windows_supported {
            return CommandOutcome::error(
                "capability-not-enabled",
                "windows capability is not enabled",
            );
        }
        if let Some(outcome) = self
            .desktop_controller
            .set_window_bounds(id, x, y, width, height)
        {
            return outcome;
        }
        let Some(window) = self.windows.get_mut(id) else {
            return CommandOutcome::error("window-not-found", format!("window {id} was not found"));
        };
        if window.closed {
            return CommandOutcome::error("window-not-found", format!("window {id} was not found"));
        }

        window.bounds = Some(DesktopWindowBounds {
            x,
            y,
            width,
            height,
        });
        let mut window_payload = window_entry_payload(id, window);
        window_payload["bounds_changed"] = serde_json::Value::Bool(true);
        CommandOutcome::ok_with_result(
            Some(format!("set bounds for window {}", window.title)),
            serde_json::json!({ "window": window_payload }),
        )
    }

    pub(crate) fn window_input(&mut self, id: &str, event: &WindowInputEvent) -> CommandOutcome {
        if !self.windows_supported {
            return CommandOutcome::error(
                "capability-not-enabled",
                "windows capability is not enabled",
            );
        }
        if let Some(outcome) = self.desktop_controller.input_window(id, event) {
            return outcome;
        }
        let Some(window) = self.windows.get(id) else {
            return CommandOutcome::error("window-not-found", format!("window {id} was not found"));
        };
        if window.closed {
            return CommandOutcome::error("window-not-found", format!("window {id} was not found"));
        }

        let mut window_payload = window_entry_payload(id, window);
        window_payload["focused"] = serde_json::Value::Bool(window.focused);
        window_payload["input"] = window_input_payload(event, "scaffold");
        CommandOutcome::ok_with_result(
            Some(format!(
                "accepted {} input for window {}",
                window_input_label(event),
                window.title
            )),
            serde_json::json!({ "window": window_payload }),
        )
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
