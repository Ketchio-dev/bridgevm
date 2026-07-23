//! Negotiated capability state, simulated fixtures, command dispatch, and the command outcome.

use crate::*;
use bridgevm_agent_protocol::AgentCapability;
use bridgevm_agent_protocol::AgentEnvelope;
use bridgevm_agent_protocol::AgentMessage;
use std::collections::BTreeMap;
use std::path::PathBuf;

pub(crate) fn default_applications() -> BTreeMap<String, ApplicationEntry> {
    [
        (
            "org.bridgevm.terminal",
            ApplicationEntry {
                name: "Terminal".to_string(),
                launched: false,
            },
        ),
        (
            "org.bridgevm.files",
            ApplicationEntry {
                name: "Files".to_string(),
                launched: false,
            },
        ),
    ]
    .into_iter()
    .map(|(id, entry)| (id.to_string(), entry))
    .collect()
}

pub(crate) fn default_windows() -> BTreeMap<String, WindowEntry> {
    [(
        "window-1",
        WindowEntry {
            title: "BridgeVM Linux Desktop".to_string(),
            focused: true,
            closed: false,
            bounds: None,
        },
    )]
    .into_iter()
    .map(|(id, entry)| (id.to_string(), entry))
    .collect()
}

pub(crate) struct GuestToolsState {
    pub(crate) shared_folders_supported: bool,
    pub(crate) drag_drop_supported: bool,
    pub(crate) applications_supported: bool,
    pub(crate) windows_supported: bool,
    pub(crate) clipboard_supported: bool,
    pub(crate) display_resize_supported: bool,
    pub(crate) fs_freeze_supported: bool,
    pub(crate) fs_thaw_supported: bool,
    pub(crate) time_sync_supported: bool,
    pub(crate) benchmark_supported: bool,
    pub(crate) shared_folders: BTreeMap<String, SharedFolderMount>,
    pub(crate) file_drops: BTreeMap<String, FileDropTransfer>,
    pub(crate) applications: BTreeMap<String, ApplicationEntry>,
    pub(crate) windows: BTreeMap<String, WindowEntry>,
    pub(crate) file_drop_dir: Option<PathBuf>,
    pub(crate) filesystem_frozen: bool,
    pub(crate) filesystem_freezer: FilesystemFreezer,
    pub(crate) clipboard_writer: ClipboardWriter,
    pub(crate) display_resizer: DisplayResizer,
    pub(crate) clock_setter: ClockSetter,
    pub(crate) desktop_controller: DesktopController,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SharedFolderMount {
    pub(crate) host_path_token: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FileDropTransfer {
    pub(crate) file_name: String,
    pub(crate) size_bytes: u64,
    pub(crate) bytes: Vec<u8>,
    pub(crate) chunks_seen: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ApplicationEntry {
    pub(crate) name: String,
    pub(crate) launched: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WindowEntry {
    pub(crate) title: String,
    pub(crate) focused: bool,
    pub(crate) closed: bool,
    pub(crate) bounds: Option<DesktopWindowBounds>,
}

pub(crate) fn window_entry_payload(id: &str, window: &WindowEntry) -> serde_json::Value {
    let mut payload = serde_json::json!({
        "id": id,
        "title": window.title,
    });
    if let Some(bounds) = &window.bounds {
        payload["bounds"] = window_bounds_payload(bounds.x, bounds.y, bounds.width, bounds.height);
    }
    payload
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CommandOutcome {
    pub(crate) ok: bool,
    pub(crate) error_code: Option<String>,
    pub(crate) message: Option<String>,
    pub(crate) result: Option<serde_json::Value>,
    pub(crate) metadata: Option<serde_json::Value>,
}

impl CommandOutcome {
    pub(crate) fn ok(message: impl Into<Option<String>>) -> Self {
        Self {
            ok: true,
            error_code: None,
            message: message.into(),
            result: None,
            metadata: None,
        }
    }

    pub(crate) fn ok_with_result(
        message: impl Into<Option<String>>,
        result: impl Into<serde_json::Value>,
    ) -> Self {
        Self {
            ok: true,
            error_code: None,
            message: message.into(),
            result: Some(result.into()),
            metadata: None,
        }
    }

    pub(crate) fn error(error_code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            ok: false,
            error_code: Some(error_code.into()),
            message: Some(message.into()),
            result: None,
            metadata: None,
        }
    }
}

impl GuestToolsState {
    pub(crate) fn new(capabilities: &[AgentCapability]) -> Self {
        Self {
            shared_folders_supported: supports_capability(capabilities, "shared-folders"),
            drag_drop_supported: supports_capability(capabilities, "drag-drop"),
            applications_supported: supports_capability(capabilities, "applications"),
            windows_supported: supports_capability(capabilities, "windows"),
            clipboard_supported: supports_capability(capabilities, "clipboard"),
            display_resize_supported: supports_capability(capabilities, "display-resize"),
            fs_freeze_supported: supports_capability(capabilities, "fs-freeze"),
            fs_thaw_supported: supports_capability(capabilities, "fs-thaw"),
            time_sync_supported: supports_capability(capabilities, "time-sync"),
            benchmark_supported: supports_capability(capabilities, "benchmark"),
            shared_folders: BTreeMap::new(),
            file_drops: BTreeMap::new(),
            applications: default_applications(),
            windows: default_windows(),
            file_drop_dir: None,
            filesystem_frozen: false,
            filesystem_freezer: FilesystemFreezer::simulated(),
            clipboard_writer: ClipboardWriter::simulated(),
            display_resizer: DisplayResizer::simulated(),
            clock_setter: ClockSetter::simulated(),
            desktop_controller: DesktopController::simulated(),
        }
    }

    pub(crate) fn with_file_drop_dir(mut self, file_drop_dir: Option<PathBuf>) -> Self {
        self.file_drop_dir = file_drop_dir;
        self
    }

    pub(crate) fn with_filesystem_freezer(mut self, filesystem_freezer: FilesystemFreezer) -> Self {
        self.filesystem_freezer = filesystem_freezer;
        self
    }

    pub(crate) fn with_clipboard_writer(mut self, clipboard_writer: ClipboardWriter) -> Self {
        self.clipboard_writer = clipboard_writer;
        self
    }

    pub(crate) fn with_display_resizer(mut self, display_resizer: DisplayResizer) -> Self {
        self.display_resizer = display_resizer;
        self
    }

    pub(crate) fn with_clock_setter(mut self, clock_setter: ClockSetter) -> Self {
        self.clock_setter = clock_setter;
        self
    }

    pub(crate) fn with_desktop_controller(mut self, desktop_controller: DesktopController) -> Self {
        self.desktop_controller = desktop_controller;
        self
    }

    pub(crate) fn handle_command(&mut self, command: &AgentEnvelope) -> Option<AgentEnvelope> {
        let outcome = self.apply_command(&command.message);
        let request_id = command.request_id.as_ref()?;

        Some(AgentEnvelope::new(AgentMessage::CommandResult {
            request_id: request_id.clone(),
            ok: outcome.ok,
            error_code: outcome.error_code,
            message: outcome.message,
            result: outcome.result,
            metadata: outcome.metadata,
        }))
    }

    pub(crate) fn apply_command(&mut self, message: &AgentMessage) -> CommandOutcome {
        match message {
            AgentMessage::TimeSync { unix_epoch_millis } => self.sync_time(*unix_epoch_millis),
            AgentMessage::ResizeDisplay {
                width,
                height,
                scale,
            } => self.resize_display(*width, *height, *scale),
            AgentMessage::SetClipboard { text } => self.set_clipboard(text),
            AgentMessage::MountShare {
                name,
                host_path_token,
            } => self.mount_share(name, host_path_token),
            AgentMessage::UnmountShare { name } => self.unmount_share(name),
            AgentMessage::FileDropStart {
                transfer_id,
                file_name,
                size_bytes,
            } => self.start_file_drop(transfer_id, file_name, *size_bytes),
            AgentMessage::FileDropChunk {
                transfer_id,
                chunk_index,
                data_base64,
            } => self.record_file_drop_chunk(transfer_id, *chunk_index, data_base64),
            AgentMessage::FileDropComplete { transfer_id } => self.complete_file_drop(transfer_id),
            AgentMessage::ListApplications => self.list_applications(),
            AgentMessage::LaunchApplication { id } => self.launch_application(id),
            AgentMessage::ListWindows => self.list_windows(),
            AgentMessage::FocusWindow { id } => self.focus_window(id),
            AgentMessage::CloseWindow { id } => self.close_window(id),
            AgentMessage::SetWindowBounds {
                id,
                x,
                y,
                width,
                height,
            } => self.set_window_bounds(id, *x, *y, *width, *height),
            AgentMessage::WindowInput { id, event } => self.window_input(id, event),
            AgentMessage::FreezeFilesystem { timeout_millis } => {
                self.freeze_filesystem(*timeout_millis)
            }
            AgentMessage::ThawFilesystem => self.thaw_filesystem(),
            AgentMessage::RunBenchmark { duration_millis } => self.run_benchmark(*duration_millis),
            _ => CommandOutcome::error(
                "unsupported-command",
                "command is not implemented by the Linux tools scaffold",
            ),
        }
    }
}
