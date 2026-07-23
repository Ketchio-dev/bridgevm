//! Split out of lib.rs to keep files under 800 lines.

use serde::Deserialize;
use serde::Serialize;
use std::net::IpAddr;

pub const PROTOCOL_VERSION: u16 = 1;
pub const MAX_FREEZE_THAW_TIMEOUT_MILLIS: u64 = 300_000;

/// Default wall-clock budget for an in-guest benchmark when the host does not
/// specify one. Small enough to stay unobtrusive on a busy guest.
pub const DEFAULT_BENCHMARK_DURATION_MILLIS: u64 = 1_000;
/// Hard upper bound on the benchmark wall-clock budget. The guest never runs a
/// benchmark longer than this no matter what the host requests, so a hostile or
/// buggy host cannot pin the guest CPU indefinitely.
pub const MAX_BENCHMARK_DURATION_MILLIS: u64 = 10_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentEnvelope {
    pub protocol_version: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    pub message: AgentMessage,
}

impl AgentEnvelope {
    pub fn new(message: AgentMessage) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            request_id: None,
            message,
        }
    }

    pub fn with_request_id(message: AgentMessage, request_id: impl Into<String>) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            request_id: Some(request_id.into()),
            message,
        }
    }

    pub fn validate(&self) -> Result<(), ProtocolValidationError> {
        if self.protocol_version != PROTOCOL_VERSION {
            return Err(ProtocolValidationError::UnsupportedVersion {
                expected: PROTOCOL_VERSION,
                actual: self.protocol_version,
            });
        }

        if self
            .request_id
            .as_ref()
            .is_some_and(|request_id| request_id.trim().is_empty())
        {
            return Err(ProtocolValidationError::EmptyField {
                field: "request_id",
            });
        }

        self.message.validate()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentMessage {
    GuestHello {
        version: u16,
        guest_os: String,
        #[serde(default)]
        agent_version: Option<String>,
        #[serde(default)]
        capabilities: Vec<AgentCapability>,
        #[serde(default)]
        auth: Option<AgentAuth>,
    },
    Heartbeat,
    TimeSync {
        unix_epoch_millis: u64,
    },
    GuestIpChanged {
        addresses: Vec<GuestIpAddress>,
    },
    AgentUpdateAvailable {
        current_version: String,
        available_version: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        download_url: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        signature: Option<String>,
    },
    ClipboardChanged {
        text: String,
    },
    SetClipboard {
        text: String,
    },
    ResizeDisplay {
        width: u32,
        height: u32,
        scale: u16,
    },
    MountShare {
        name: String,
        host_path_token: String,
    },
    UnmountShare {
        name: String,
    },
    FileDropStart {
        transfer_id: String,
        file_name: String,
        size_bytes: u64,
    },
    FileDropChunk {
        transfer_id: String,
        chunk_index: u32,
        data_base64: String,
    },
    FileDropComplete {
        transfer_id: String,
    },
    ListApplications,
    LaunchApplication {
        id: String,
    },
    ListWindows,
    FocusWindow {
        id: String,
    },
    CloseWindow {
        id: String,
    },
    SetWindowBounds {
        id: String,
        x: i64,
        y: i64,
        width: u64,
        height: u64,
    },
    WindowInput {
        id: String,
        event: WindowInputEvent,
    },
    FreezeFilesystem {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        timeout_millis: Option<u64>,
    },
    ThawFilesystem,
    GuestMetrics {
        cpu_percent: u8,
        memory_used_mib: u64,
    },
    /// Host->guest request to run a bounded in-guest performance benchmark.
    /// `duration_millis` is the wall-clock budget; when `None` the guest uses
    /// `DEFAULT_BENCHMARK_DURATION_MILLIS`, and any explicit value is validated
    /// against `MAX_BENCHMARK_DURATION_MILLIS`. The result is reported back
    /// through the standard `CommandResult.result` payload (same channel as
    /// guest-metrics / list-applications), not a bespoke transport.
    RunBenchmark {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        duration_millis: Option<u64>,
    },
    CommandResult {
        request_id: String,
        ok: bool,
        error_code: Option<String>,
        message: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        result: Option<serde_json::Value>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        metadata: Option<serde_json::Value>,
    },
}

impl AgentMessage {
    pub fn validate(&self) -> Result<(), ProtocolValidationError> {
        match self {
            Self::GuestHello {
                version,
                guest_os,
                agent_version,
                capabilities,
                auth,
            } => {
                if *version != PROTOCOL_VERSION {
                    return Err(ProtocolValidationError::UnsupportedVersion {
                        expected: PROTOCOL_VERSION,
                        actual: *version,
                    });
                }
                if guest_os.trim().is_empty() {
                    return Err(ProtocolValidationError::EmptyGuestOs);
                }
                if let Some(agent_version) = agent_version {
                    validate_non_empty("guest_hello.agent_version", agent_version)?;
                }
                if capabilities.is_empty() {
                    return Err(ProtocolValidationError::EmptyCapabilities);
                }
                for capability in capabilities {
                    capability.validate()?;
                }
                let auth = auth.as_ref().ok_or(ProtocolValidationError::MissingField {
                    field: "guest_hello.auth",
                })?;
                auth.validate()?;
                Ok(())
            }
            Self::GuestIpChanged { addresses } => {
                if addresses.is_empty() {
                    return Err(ProtocolValidationError::EmptyGuestIpList);
                }

                for address in addresses {
                    if address
                        .interface
                        .as_ref()
                        .is_some_and(|interface| interface.trim().is_empty())
                    {
                        return Err(ProtocolValidationError::EmptyField {
                            field: "guest_ip.interface",
                        });
                    }

                    if address.address.is_unspecified() {
                        return Err(ProtocolValidationError::UnspecifiedGuestIp {
                            address: address.address,
                        });
                    }
                }

                Ok(())
            }
            Self::AgentUpdateAvailable {
                current_version,
                available_version,
                download_url,
                signature,
            } => {
                validate_non_empty("agent_update.current_version", current_version)?;
                validate_non_empty("agent_update.available_version", available_version)?;
                if let Some(download_url) = download_url {
                    validate_non_empty("agent_update.download_url", download_url)?;
                }
                if let Some(signature) = signature {
                    validate_non_empty("agent_update.signature", signature)?;
                }
                Ok(())
            }
            Self::TimeSync { unix_epoch_millis } => {
                if *unix_epoch_millis == 0 {
                    return Err(ProtocolValidationError::InvalidTimestamp);
                }
                Ok(())
            }
            Self::ResizeDisplay {
                width,
                height,
                scale,
            } => {
                if *width == 0 || *height == 0 || *scale == 0 {
                    return Err(ProtocolValidationError::InvalidDisplaySize {
                        width: *width,
                        height: *height,
                        scale: *scale,
                    });
                }
                Ok(())
            }
            Self::MountShare {
                name,
                host_path_token,
            } => {
                validate_non_empty("share.name", name)?;
                validate_non_empty("share.host_path_token", host_path_token)
            }
            Self::UnmountShare { name } => validate_non_empty("share.name", name),
            Self::FileDropStart {
                transfer_id,
                file_name,
                size_bytes,
            } => {
                validate_non_empty("file_drop.transfer_id", transfer_id)?;
                validate_non_empty("file_drop.file_name", file_name)?;
                if *size_bytes == 0 {
                    return Err(ProtocolValidationError::InvalidFileDropSize);
                }
                Ok(())
            }
            Self::FileDropChunk {
                transfer_id,
                data_base64,
                ..
            } => {
                validate_non_empty("file_drop.transfer_id", transfer_id)?;
                validate_non_empty("file_drop.data_base64", data_base64)
            }
            Self::FileDropComplete { transfer_id } => {
                validate_non_empty("file_drop.transfer_id", transfer_id)
            }
            Self::LaunchApplication { id } => validate_non_empty("application.id", id),
            Self::FocusWindow { id } | Self::CloseWindow { id } => {
                validate_non_empty("window.id", id)
            }
            Self::SetWindowBounds {
                id, width, height, ..
            } => {
                validate_non_empty("window.id", id)?;
                if *width == 0 || *height == 0 {
                    return Err(ProtocolValidationError::InvalidWindowBounds {
                        width: *width,
                        height: *height,
                    });
                }
                Ok(())
            }
            Self::WindowInput { id, event } => {
                validate_non_empty("window.id", id)?;
                event.validate()
            }
            Self::FreezeFilesystem { timeout_millis } => {
                validate_filesystem_freeze_timeout(*timeout_millis)
            }
            Self::GuestMetrics { cpu_percent, .. } => {
                if *cpu_percent > 100 {
                    return Err(ProtocolValidationError::InvalidCpuPercent(*cpu_percent));
                }
                Ok(())
            }
            Self::RunBenchmark { duration_millis } => validate_benchmark_duration(*duration_millis),
            Self::CommandResult {
                request_id,
                ok,
                error_code,
                message: _,
                result: _,
                metadata: _,
            } => {
                validate_non_empty("command_result.request_id", request_id)?;
                if !ok {
                    validate_required_optional("command_result.error_code", error_code.as_deref())?;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WindowInputEvent {
    Pointer {
        x: i64,
        y: i64,
        action: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        button: Option<String>,
    },
    Key {
        key: String,
        action: String,
    },
}

impl WindowInputEvent {
    pub(crate) fn validate(&self) -> Result<(), ProtocolValidationError> {
        match self {
            Self::Pointer { action, button, .. } => {
                validate_window_input_action(
                    "window_input.pointer.action",
                    action,
                    &["move", "press", "release", "click"],
                )?;
                if action != "move" {
                    validate_required_optional("window_input.pointer.button", button.as_deref())?;
                }
                if let Some(button) = button {
                    validate_window_input_action(
                        "window_input.pointer.button",
                        button,
                        &["left", "middle", "right"],
                    )?;
                }
                Ok(())
            }
            Self::Key { key, action } => {
                validate_non_empty("window_input.key", key)?;
                validate_window_input_action(
                    "window_input.key.action",
                    action,
                    &["press", "release", "tap"],
                )
            }
        }
    }
}

pub(crate) fn validate_non_empty(
    field: &'static str,
    value: &str,
) -> Result<(), ProtocolValidationError> {
    if value.trim().is_empty() {
        return Err(ProtocolValidationError::EmptyField { field });
    }

    Ok(())
}

pub(crate) fn validate_required_optional(
    field: &'static str,
    value: Option<&str>,
) -> Result<(), ProtocolValidationError> {
    match value {
        Some(value) => validate_non_empty(field, value),
        None => Err(ProtocolValidationError::MissingField { field }),
    }
}

pub(crate) fn validate_benchmark_duration(
    duration_millis: Option<u64>,
) -> Result<(), ProtocolValidationError> {
    let Some(duration_millis) = duration_millis else {
        return Ok(());
    };
    if duration_millis == 0 || duration_millis > MAX_BENCHMARK_DURATION_MILLIS {
        return Err(ProtocolValidationError::InvalidBenchmarkDuration {
            duration_millis,
            max_duration_millis: MAX_BENCHMARK_DURATION_MILLIS,
        });
    }

    Ok(())
}

pub(crate) fn validate_filesystem_freeze_timeout(
    timeout_millis: Option<u64>,
) -> Result<(), ProtocolValidationError> {
    let Some(timeout_millis) = timeout_millis else {
        return Ok(());
    };
    if timeout_millis == 0 || timeout_millis > MAX_FREEZE_THAW_TIMEOUT_MILLIS {
        return Err(ProtocolValidationError::InvalidFilesystemFreezeTimeout {
            timeout_millis,
            max_timeout_millis: MAX_FREEZE_THAW_TIMEOUT_MILLIS,
        });
    }

    Ok(())
}

pub(crate) fn validate_window_input_action(
    field: &'static str,
    value: &str,
    allowed: &[&str],
) -> Result<(), ProtocolValidationError> {
    validate_non_empty(field, value)?;
    if !allowed.contains(&value) {
        return Err(ProtocolValidationError::InvalidWindowInputValue {
            field,
            value: value.to_string(),
        });
    }

    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuestIpAddress {
    pub address: IpAddr,
    pub interface: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentCapability {
    pub name: String,
    pub version: u16,
}

impl AgentCapability {
    pub(crate) fn validate(&self) -> Result<(), ProtocolValidationError> {
        validate_non_empty("capability.name", &self.name)?;
        if self.version == 0 {
            return Err(ProtocolValidationError::InvalidCapabilityVersion {
                capability: self.name.clone(),
                version: self.version,
            });
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentAuth {
    ToolsToken { token: String },
}

impl AgentAuth {
    pub(crate) fn validate(&self) -> Result<(), ProtocolValidationError> {
        match self {
            Self::ToolsToken { token } => validate_non_empty("auth.tools_token", token),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtocolValidationError {
    UnsupportedVersion {
        expected: u16,
        actual: u16,
    },
    EmptyGuestOs,
    EmptyGuestIpList,
    EmptyCapabilities,
    UnspecifiedGuestIp {
        address: IpAddr,
    },
    EmptyField {
        field: &'static str,
    },
    MissingField {
        field: &'static str,
    },
    InvalidCapabilityVersion {
        capability: String,
        version: u16,
    },
    InvalidTimestamp,
    InvalidDisplaySize {
        width: u32,
        height: u32,
        scale: u16,
    },
    InvalidFileDropSize,
    InvalidFilesystemFreezeTimeout {
        timeout_millis: u64,
        max_timeout_millis: u64,
    },
    InvalidCpuPercent(u8),
    InvalidBenchmarkDuration {
        duration_millis: u64,
        max_duration_millis: u64,
    },
    InvalidWindowBounds {
        width: u64,
        height: u64,
    },
    InvalidWindowInputValue {
        field: &'static str,
        value: String,
    },
}
