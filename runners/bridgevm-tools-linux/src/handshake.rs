//! Token resolution and validation, capability parsing, and the guest hello envelope.

use crate::*;
use anyhow::Context;
use anyhow::Result;
use bridgevm_agent_protocol::AgentAuth;
use bridgevm_agent_protocol::AgentCapability;
use bridgevm_agent_protocol::AgentEnvelope;
use bridgevm_agent_protocol::AgentMessage;
use std::collections::BTreeSet;
use std::path::PathBuf;

pub(crate) const MAX_TOKEN_FILE_BYTES: usize = 64 * 1024;

pub(crate) const AGENT_VERSION: &str = env!("CARGO_PKG_VERSION");

pub(crate) fn resolve_token(token: Option<String>, token_file: Option<PathBuf>) -> Result<String> {
    match (token, token_file) {
        (Some(_), Some(_)) => anyhow::bail!("use either --token or --token-file, not both"),
        (Some(token), None) => validate_token(&token),
        (None, Some(path)) => {
            let contents = read_utf8_file_bounded(&path, MAX_TOKEN_FILE_BYTES)
                .with_context(|| format!("failed to read token file {}", path.display()))?;
            parse_token_file(&contents)
        }
        (None, None) => {
            anyhow::bail!("--token or --token-file is required when a transport is provided")
        }
    }
}

pub(crate) fn parse_token_file(contents: &str) -> Result<String> {
    let trimmed = contents.trim();
    if trimmed.starts_with('{') {
        let value: serde_json::Value =
            serde_json::from_str(trimmed).context("invalid guest tools token JSON")?;
        let token = value
            .get("token")
            .and_then(|token| token.as_str())
            .context("guest tools token JSON is missing string field 'token'")?;
        return validate_token(token);
    }

    validate_token(trimmed)
}

pub(crate) fn validate_token(token: &str) -> Result<String> {
    let token = token.trim();
    if token.is_empty() {
        anyhow::bail!("guest tools token cannot be empty");
    }

    Ok(token.to_string())
}

pub(crate) fn guest_hello(
    token: &str,
    guest_os: &str,
    capabilities: Vec<AgentCapability>,
) -> AgentEnvelope {
    AgentEnvelope::new(AgentMessage::GuestHello {
        version: bridgevm_agent_protocol::PROTOCOL_VERSION,
        guest_os: guest_os.to_string(),
        agent_version: Some(AGENT_VERSION.to_string()),
        capabilities,
        auth: Some(AgentAuth::ToolsToken {
            token: token.to_string(),
        }),
    })
}

pub(crate) fn resolve_capabilities(values: &[String]) -> Result<Vec<AgentCapability>> {
    if values.is_empty() {
        return Ok(default_capabilities());
    }

    let mut seen = BTreeSet::new();
    values
        .iter()
        .map(|value| parse_capability(value, &mut seen))
        .collect()
}

pub(crate) fn parse_capability(
    value: &str,
    seen: &mut BTreeSet<String>,
) -> Result<AgentCapability> {
    let (name, version) = value
        .split_once(':')
        .map_or((value, "1"), |(name, version)| (name, version));
    let name = name.trim();
    if name.is_empty() {
        anyhow::bail!("capability name cannot be empty");
    }
    if !seen.insert(name.to_string()) {
        anyhow::bail!("duplicate capability '{name}'");
    }
    let version = version
        .trim()
        .parse::<u16>()
        .with_context(|| format!("invalid version for capability '{name}'"))?;
    if version == 0 {
        anyhow::bail!("capability '{name}' version must be greater than zero");
    }

    Ok(AgentCapability {
        name: name.to_string(),
        version,
    })
}

pub(crate) fn default_capabilities() -> Vec<AgentCapability> {
    [
        "heartbeat",
        "time-sync",
        "guest-ip",
        "clipboard",
        "display-resize",
        "shared-folders",
        "drag-drop",
        "applications",
        "windows",
        "fs-freeze",
        "fs-thaw",
        "guest-metrics",
        "agent-update",
        "benchmark",
    ]
    .into_iter()
    .map(|name| AgentCapability {
        name: name.to_string(),
        version: 1,
    })
    .collect()
}

pub(crate) fn supports_capability(capabilities: &[AgentCapability], name: &str) -> bool {
    capabilities
        .iter()
        .any(|capability| capability.name == name)
}
