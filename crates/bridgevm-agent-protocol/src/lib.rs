use serde::{Deserialize, Serialize};
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

fn validate_non_empty(field: &'static str, value: &str) -> Result<(), ProtocolValidationError> {
    if value.trim().is_empty() {
        return Err(ProtocolValidationError::EmptyField { field });
    }

    Ok(())
}

fn validate_required_optional(
    field: &'static str,
    value: Option<&str>,
) -> Result<(), ProtocolValidationError> {
    match value {
        Some(value) => validate_non_empty(field, value),
        None => Err(ProtocolValidationError::MissingField { field }),
    }
}

fn validate_benchmark_duration(
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

fn validate_filesystem_freeze_timeout(
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
    fn validate(&self) -> Result<(), ProtocolValidationError> {
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
    fn validate(&self) -> Result<(), ProtocolValidationError> {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

    #[test]
    fn envelope_round_trips_guest_hello() {
        let envelope = AgentEnvelope::new(valid_guest_hello());

        let json = serde_json::to_string(&envelope).expect("serialize envelope");
        let decoded: AgentEnvelope = serde_json::from_str(&json).expect("deserialize envelope");

        assert_eq!(decoded, envelope);
        assert_eq!(decoded.validate(), Ok(()));
    }

    #[test]
    fn guest_hello_deserializes_legacy_shape_but_requires_new_handshake_fields() {
        let json = format!(
            r#"{{"protocol_version":{},"message":{{"GuestHello":{{"version":{},"guest_os":"linux"}}}}}}"#,
            PROTOCOL_VERSION, PROTOCOL_VERSION
        );
        let decoded: AgentEnvelope = serde_json::from_str(&json).expect("deserialize envelope");

        assert_eq!(
            decoded.validate(),
            Err(ProtocolValidationError::EmptyCapabilities)
        );
    }

    #[test]
    fn guest_ip_changed_round_trips_and_validates() {
        let envelope = AgentEnvelope::new(AgentMessage::GuestIpChanged {
            addresses: vec![
                GuestIpAddress {
                    address: IpAddr::V4(Ipv4Addr::new(192, 168, 64, 2)),
                    interface: Some("eth0".to_string()),
                },
                GuestIpAddress {
                    address: IpAddr::V6(Ipv6Addr::LOCALHOST),
                    interface: Some("lo".to_string()),
                },
            ],
        });

        let json = serde_json::to_string(&envelope).expect("serialize guest IP envelope");
        let decoded: AgentEnvelope =
            serde_json::from_str(&json).expect("deserialize guest IP envelope");

        assert_eq!(decoded, envelope);
        assert_eq!(decoded.validate(), Ok(()));
    }

    #[test]
    fn agent_update_available_round_trips_and_validates() {
        let envelope = AgentEnvelope::new(AgentMessage::AgentUpdateAvailable {
            current_version: "1.2.3".to_string(),
            available_version: "1.2.4".to_string(),
            download_url: Some("https://updates.example.invalid/agent/1.2.4".to_string()),
            signature: Some("minisign-signature".to_string()),
        });

        let json = serde_json::to_string(&envelope).expect("serialize agent update envelope");
        let decoded: AgentEnvelope =
            serde_json::from_str(&json).expect("deserialize agent update envelope");

        assert_eq!(decoded, envelope);
        assert_eq!(decoded.validate(), Ok(()));
    }

    #[test]
    fn p0_control_messages_round_trip_and_validate() {
        let messages = [
            AgentMessage::Heartbeat,
            AgentMessage::TimeSync {
                unix_epoch_millis: 1_781_470_000_000,
            },
            AgentMessage::SetClipboard {
                text: "hello from host".to_string(),
            },
            AgentMessage::ResizeDisplay {
                width: 1920,
                height: 1080,
                scale: 2,
            },
            AgentMessage::MountShare {
                name: "Projects".to_string(),
                host_path_token: "share-token-1".to_string(),
            },
            AgentMessage::UnmountShare {
                name: "Projects".to_string(),
            },
            AgentMessage::FileDropStart {
                transfer_id: "drop-1".to_string(),
                file_name: "notes.txt".to_string(),
                size_bytes: 11,
            },
            AgentMessage::FileDropChunk {
                transfer_id: "drop-1".to_string(),
                chunk_index: 0,
                data_base64: "aGVsbG8gd29ybGQ=".to_string(),
            },
            AgentMessage::FileDropComplete {
                transfer_id: "drop-1".to_string(),
            },
            AgentMessage::FreezeFilesystem {
                timeout_millis: Some(30_000),
            },
            AgentMessage::ThawFilesystem,
            AgentMessage::RunBenchmark {
                duration_millis: None,
            },
            AgentMessage::RunBenchmark {
                duration_millis: Some(500),
            },
        ];

        for message in messages {
            let envelope = AgentEnvelope::new(message);
            let json = serde_json::to_string(&envelope).expect("serialize envelope");
            let decoded: AgentEnvelope = serde_json::from_str(&json).expect("deserialize envelope");

            assert_eq!(decoded, envelope);
            assert_eq!(decoded.validate(), Ok(()));
        }
    }

    #[test]
    fn envelope_request_id_round_trips_and_validates() {
        let envelope = AgentEnvelope::with_request_id(
            AgentMessage::ResizeDisplay {
                width: 1440,
                height: 900,
                scale: 2,
            },
            "resize-1",
        );

        let json = serde_json::to_string(&envelope).expect("serialize envelope");
        let decoded: AgentEnvelope = serde_json::from_str(&json).expect("deserialize envelope");

        assert_eq!(decoded, envelope);
        assert_eq!(decoded.validate(), Ok(()));
    }

    #[test]
    fn command_result_round_trips_and_validates() {
        let envelope = AgentEnvelope::new(AgentMessage::CommandResult {
            request_id: "resize-1".to_string(),
            ok: false,
            error_code: Some("unsupported_resolution".to_string()),
            message: Some("guest refused 0x0 display".to_string()),
            result: Some(serde_json::json!({
                "width": 1440,
                "height": 900,
                "applied": false
            })),
            metadata: Some(serde_json::json!({
                "source": "display-agent",
                "attempt": 1
            })),
        });

        let json = serde_json::to_string(&envelope).expect("serialize command result");
        let decoded: AgentEnvelope =
            serde_json::from_str(&json).expect("deserialize command result");

        assert_eq!(decoded, envelope);
        assert_eq!(decoded.validate(), Ok(()));
    }

    #[test]
    fn validation_rejects_wrong_envelope_version() {
        let envelope = AgentEnvelope {
            protocol_version: PROTOCOL_VERSION + 1,
            request_id: None,
            message: AgentMessage::Heartbeat,
        };

        assert_eq!(
            envelope.validate(),
            Err(ProtocolValidationError::UnsupportedVersion {
                expected: PROTOCOL_VERSION,
                actual: PROTOCOL_VERSION + 1,
            })
        );
    }

    #[test]
    fn validation_rejects_empty_guest_ip_report() {
        let envelope = AgentEnvelope::new(AgentMessage::GuestIpChanged { addresses: vec![] });

        assert_eq!(
            envelope.validate(),
            Err(ProtocolValidationError::EmptyGuestIpList)
        );
    }

    #[test]
    fn validation_rejects_unspecified_guest_ip_report() {
        let envelope = AgentEnvelope::new(AgentMessage::GuestIpChanged {
            addresses: vec![GuestIpAddress {
                address: IpAddr::V4(Ipv4Addr::UNSPECIFIED),
                interface: Some("eth0".to_string()),
            }],
        });

        assert_eq!(
            envelope.validate(),
            Err(ProtocolValidationError::UnspecifiedGuestIp {
                address: IpAddr::V4(Ipv4Addr::UNSPECIFIED)
            })
        );
    }

    #[test]
    fn validation_rejects_invalid_p0_control_messages() {
        let cases = [
            (
                AgentMessage::TimeSync {
                    unix_epoch_millis: 0,
                },
                ProtocolValidationError::InvalidTimestamp,
            ),
            (
                AgentMessage::ResizeDisplay {
                    width: 0,
                    height: 1080,
                    scale: 2,
                },
                ProtocolValidationError::InvalidDisplaySize {
                    width: 0,
                    height: 1080,
                    scale: 2,
                },
            ),
            (
                AgentMessage::MountShare {
                    name: " ".to_string(),
                    host_path_token: "share-token-1".to_string(),
                },
                ProtocolValidationError::EmptyField {
                    field: "share.name",
                },
            ),
            (
                AgentMessage::MountShare {
                    name: "Projects".to_string(),
                    host_path_token: "".to_string(),
                },
                ProtocolValidationError::EmptyField {
                    field: "share.host_path_token",
                },
            ),
            (
                AgentMessage::LaunchApplication {
                    id: "\t".to_string(),
                },
                ProtocolValidationError::EmptyField {
                    field: "application.id",
                },
            ),
            (
                AgentMessage::FileDropStart {
                    transfer_id: "drop-1".to_string(),
                    file_name: "notes.txt".to_string(),
                    size_bytes: 0,
                },
                ProtocolValidationError::InvalidFileDropSize,
            ),
            (
                AgentMessage::FileDropChunk {
                    transfer_id: " ".to_string(),
                    chunk_index: 0,
                    data_base64: "aGVsbG8=".to_string(),
                },
                ProtocolValidationError::EmptyField {
                    field: "file_drop.transfer_id",
                },
            ),
            (
                AgentMessage::GuestMetrics {
                    cpu_percent: 101,
                    memory_used_mib: 1024,
                },
                ProtocolValidationError::InvalidCpuPercent(101),
            ),
            (
                AgentMessage::FreezeFilesystem {
                    timeout_millis: Some(0),
                },
                ProtocolValidationError::InvalidFilesystemFreezeTimeout {
                    timeout_millis: 0,
                    max_timeout_millis: MAX_FREEZE_THAW_TIMEOUT_MILLIS,
                },
            ),
            (
                AgentMessage::FreezeFilesystem {
                    timeout_millis: Some(MAX_FREEZE_THAW_TIMEOUT_MILLIS + 1),
                },
                ProtocolValidationError::InvalidFilesystemFreezeTimeout {
                    timeout_millis: MAX_FREEZE_THAW_TIMEOUT_MILLIS + 1,
                    max_timeout_millis: MAX_FREEZE_THAW_TIMEOUT_MILLIS,
                },
            ),
            (
                AgentMessage::RunBenchmark {
                    duration_millis: Some(0),
                },
                ProtocolValidationError::InvalidBenchmarkDuration {
                    duration_millis: 0,
                    max_duration_millis: MAX_BENCHMARK_DURATION_MILLIS,
                },
            ),
            (
                AgentMessage::RunBenchmark {
                    duration_millis: Some(MAX_BENCHMARK_DURATION_MILLIS + 1),
                },
                ProtocolValidationError::InvalidBenchmarkDuration {
                    duration_millis: MAX_BENCHMARK_DURATION_MILLIS + 1,
                    max_duration_millis: MAX_BENCHMARK_DURATION_MILLIS,
                },
            ),
            (
                AgentMessage::CommandResult {
                    request_id: "".to_string(),
                    ok: true,
                    error_code: None,
                    message: None,
                    result: None,
                    metadata: None,
                },
                ProtocolValidationError::EmptyField {
                    field: "command_result.request_id",
                },
            ),
            (
                AgentMessage::CommandResult {
                    request_id: "resize-1".to_string(),
                    ok: false,
                    error_code: Some(" ".to_string()),
                    message: None,
                    result: None,
                    metadata: None,
                },
                ProtocolValidationError::EmptyField {
                    field: "command_result.error_code",
                },
            ),
            (
                AgentMessage::CommandResult {
                    request_id: "resize-1".to_string(),
                    ok: false,
                    error_code: None,
                    message: None,
                    result: None,
                    metadata: None,
                },
                ProtocolValidationError::MissingField {
                    field: "command_result.error_code",
                },
            ),
            (
                AgentMessage::AgentUpdateAvailable {
                    current_version: "".to_string(),
                    available_version: "1.2.4".to_string(),
                    download_url: None,
                    signature: None,
                },
                ProtocolValidationError::EmptyField {
                    field: "agent_update.current_version",
                },
            ),
            (
                AgentMessage::AgentUpdateAvailable {
                    current_version: "1.2.3".to_string(),
                    available_version: " ".to_string(),
                    download_url: None,
                    signature: None,
                },
                ProtocolValidationError::EmptyField {
                    field: "agent_update.available_version",
                },
            ),
            (
                AgentMessage::AgentUpdateAvailable {
                    current_version: "1.2.3".to_string(),
                    available_version: "1.2.4".to_string(),
                    download_url: Some("\t".to_string()),
                    signature: None,
                },
                ProtocolValidationError::EmptyField {
                    field: "agent_update.download_url",
                },
            ),
            (
                AgentMessage::AgentUpdateAvailable {
                    current_version: "1.2.3".to_string(),
                    available_version: "1.2.4".to_string(),
                    download_url: None,
                    signature: Some(" ".to_string()),
                },
                ProtocolValidationError::EmptyField {
                    field: "agent_update.signature",
                },
            ),
        ];

        for (message, expected) in cases {
            let envelope = AgentEnvelope::new(message);
            assert_eq!(envelope.validate(), Err(expected));
        }
    }

    #[test]
    fn validation_rejects_empty_envelope_request_id() {
        let envelope = AgentEnvelope::with_request_id(AgentMessage::Heartbeat, " ");

        assert_eq!(
            envelope.validate(),
            Err(ProtocolValidationError::EmptyField {
                field: "request_id"
            })
        );
    }

    #[test]
    fn validation_rejects_invalid_guest_hello_handshake() {
        let cases = [
            (
                AgentMessage::GuestHello {
                    version: PROTOCOL_VERSION,
                    guest_os: "linux".to_string(),
                    agent_version: Some("1.0.0".to_string()),
                    capabilities: vec![],
                    auth: Some(AgentAuth::ToolsToken {
                        token: "vm-token".to_string(),
                    }),
                },
                ProtocolValidationError::EmptyCapabilities,
            ),
            (
                AgentMessage::GuestHello {
                    version: PROTOCOL_VERSION,
                    guest_os: "linux".to_string(),
                    agent_version: Some(" ".to_string()),
                    capabilities: valid_capabilities(),
                    auth: Some(AgentAuth::ToolsToken {
                        token: "vm-token".to_string(),
                    }),
                },
                ProtocolValidationError::EmptyField {
                    field: "guest_hello.agent_version",
                },
            ),
            (
                AgentMessage::GuestHello {
                    version: PROTOCOL_VERSION,
                    guest_os: "linux".to_string(),
                    agent_version: Some("1.0.0".to_string()),
                    capabilities: vec![AgentCapability {
                        name: "clipboard".to_string(),
                        version: 0,
                    }],
                    auth: Some(AgentAuth::ToolsToken {
                        token: "vm-token".to_string(),
                    }),
                },
                ProtocolValidationError::InvalidCapabilityVersion {
                    capability: "clipboard".to_string(),
                    version: 0,
                },
            ),
            (
                AgentMessage::GuestHello {
                    version: PROTOCOL_VERSION,
                    guest_os: "linux".to_string(),
                    agent_version: Some("1.0.0".to_string()),
                    capabilities: valid_capabilities(),
                    auth: None,
                },
                ProtocolValidationError::MissingField {
                    field: "guest_hello.auth",
                },
            ),
            (
                AgentMessage::GuestHello {
                    version: PROTOCOL_VERSION,
                    guest_os: "linux".to_string(),
                    agent_version: Some("1.0.0".to_string()),
                    capabilities: valid_capabilities(),
                    auth: Some(AgentAuth::ToolsToken {
                        token: "".to_string(),
                    }),
                },
                ProtocolValidationError::EmptyField {
                    field: "auth.tools_token",
                },
            ),
        ];

        for (message, expected) in cases {
            let envelope = AgentEnvelope::new(message);
            assert_eq!(envelope.validate(), Err(expected));
        }
    }

    fn valid_guest_hello() -> AgentMessage {
        AgentMessage::GuestHello {
            version: PROTOCOL_VERSION,
            guest_os: "linux".to_string(),
            agent_version: Some("1.0.0".to_string()),
            capabilities: valid_capabilities(),
            auth: Some(AgentAuth::ToolsToken {
                token: "vm-token".to_string(),
            }),
        }
    }

    fn valid_capabilities() -> Vec<AgentCapability> {
        vec![
            AgentCapability {
                name: "heartbeat".to_string(),
                version: 1,
            },
            AgentCapability {
                name: "clipboard".to_string(),
                version: 1,
            },
            AgentCapability {
                name: "display-resize".to_string(),
                version: 1,
            },
        ]
    }
}
