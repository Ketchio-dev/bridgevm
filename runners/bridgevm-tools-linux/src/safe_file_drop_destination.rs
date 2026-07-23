//! Split out of main.rs to keep files under 800 lines.

use crate::*;
use anyhow::Context;
use anyhow::Result;
use bridgevm_agent_protocol::AgentAuth;
use bridgevm_agent_protocol::AgentCapability;
use bridgevm_agent_protocol::AgentEnvelope;
use bridgevm_agent_protocol::AgentMessage;
use bridgevm_agent_protocol::GuestIpAddress;
use std::collections::BTreeSet;
use std::net::IpAddr;
use std::path::Component;
use std::path::Path;
use std::path::PathBuf;

pub(crate) fn safe_file_drop_destination(root: &Path, file_name: &str) -> Option<PathBuf> {
    let mut components = Path::new(file_name).components();
    let Some(Component::Normal(name)) = components.next() else {
        return None;
    };
    if components.next().is_some() {
        return None;
    }
    Some(root.join(name))
}

pub(crate) fn decode_base64(input: &str) -> Result<Vec<u8>, String> {
    let bytes = input.as_bytes();
    if bytes.len() % 4 != 0 {
        return Err("base64 payload length must be a multiple of 4".to_string());
    }

    let mut output = Vec::with_capacity(bytes.len() / 4 * 3);
    let mut index = 0usize;
    while index < bytes.len() {
        let chunk = &bytes[index..index + 4];
        let mut values = [0_u8; 4];
        let mut padding = 0usize;
        for (offset, byte) in chunk.iter().enumerate() {
            if *byte == b'=' {
                padding += 1;
                values[offset] = 0;
                continue;
            }
            if padding > 0 {
                return Err("base64 padding must be at the end of the payload".to_string());
            }
            values[offset] = decode_base64_value(*byte)
                .ok_or_else(|| format!("base64 payload contains invalid byte 0x{byte:02x}"))?;
        }
        if padding > 2 {
            return Err("base64 payload has too much padding".to_string());
        }
        if padding > 0 && index + 4 != bytes.len() {
            return Err("base64 padding is only allowed in the final chunk".to_string());
        }

        output.push((values[0] << 2) | (values[1] >> 4));
        if padding < 2 {
            output.push((values[1] << 4) | (values[2] >> 2));
        }
        if padding == 0 {
            output.push((values[2] << 6) | values[3]);
        }
        index += 4;
    }

    Ok(output)
}

pub(crate) fn decode_base64_value(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TelemetryConfig {
    pub(crate) guest_ips: Vec<GuestIpAddress>,
    pub(crate) metrics: Option<GuestMetricsConfig>,
    pub(crate) clipboard_text: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GuestMetricsConfig {
    pub(crate) cpu_percent: u8,
    pub(crate) memory_used_mib: u64,
}

impl TelemetryConfig {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_args(
        capabilities: &[AgentCapability],
        guest_ips: &[String],
        no_guest_ip: bool,
        metrics_cpu_percent: u8,
        metrics_memory_used_mib: u64,
        no_metrics: bool,
        no_real_metrics: bool,
        clipboard_text: Option<String>,
    ) -> Result<Self> {
        Self::from_args_with_reader(
            capabilities,
            guest_ips,
            no_guest_ip,
            metrics_cpu_percent,
            metrics_memory_used_mib,
            no_metrics,
            no_real_metrics,
            clipboard_text,
            read_proc_metrics,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_args_with_reader(
        capabilities: &[AgentCapability],
        guest_ips: &[String],
        no_guest_ip: bool,
        metrics_cpu_percent: u8,
        metrics_memory_used_mib: u64,
        no_metrics: bool,
        no_real_metrics: bool,
        clipboard_text: Option<String>,
        metrics_reader: impl Fn() -> Option<GuestMetricsConfig>,
    ) -> Result<Self> {
        if no_guest_ip && !guest_ips.is_empty() {
            anyhow::bail!("use either --guest-ip or --no-guest-ip, not both");
        }
        if no_metrics && (metrics_cpu_percent != 1 || metrics_memory_used_mib != 256) {
            anyhow::bail!("metrics values cannot be set with --no-metrics");
        }
        if no_metrics && no_real_metrics {
            anyhow::bail!("use either --no-metrics or --no-real-metrics, not both");
        }
        if metrics_cpu_percent > 100 {
            anyhow::bail!("--metrics-cpu-percent must be between 0 and 100");
        }

        let supports_guest_ip = supports_capability(capabilities, "guest-ip");
        let supports_guest_metrics = supports_capability(capabilities, "guest-metrics");
        let supports_clipboard = supports_capability(capabilities, "clipboard");
        let guest_ips = if no_guest_ip || !supports_guest_ip {
            Vec::new()
        } else if guest_ips.is_empty() {
            vec![parse_guest_ip("10.0.2.15@eth0")?]
        } else {
            guest_ips
                .iter()
                .map(|value| parse_guest_ip(value))
                .collect::<Result<Vec<_>>>()?
        };
        let configured_metrics = GuestMetricsConfig {
            cpu_percent: metrics_cpu_percent,
            memory_used_mib: metrics_memory_used_mib,
        };
        let metrics = if no_metrics || !supports_guest_metrics {
            None
        } else if no_real_metrics {
            // Honor the synthetic --metrics-* values verbatim.
            Some(configured_metrics)
        } else {
            // Prefer real /proc-derived metrics; fall back to the configured
            // synthetic values if /proc is unavailable (e.g. non-Linux build).
            Some(metrics_reader().unwrap_or(configured_metrics))
        };
        let clipboard_text = match clipboard_text {
            Some(_) if !supports_clipboard => {
                anyhow::bail!("--clipboard-text requires the clipboard capability")
            }
            Some(text) => Some(normalize_clipboard_text(&text)?),
            None => None,
        };

        Ok(Self {
            guest_ips,
            metrics,
            clipboard_text,
        })
    }
}

/// Read real guest metrics from /proc. Returns None if the files cannot be
/// read or parsed (e.g. when running off-Linux for unit tests), so the caller
/// can fall back to the configured synthetic values.
pub(crate) fn read_proc_metrics() -> Option<GuestMetricsConfig> {
    let meminfo = read_utf8_file_bounded(Path::new("/proc/meminfo"), MAX_PROC_TEXT_BYTES).ok()?;
    let memory_used_mib = parse_memory_used_mib(&meminfo)?;
    // CPU load is approximated from the 1-minute load average over the online
    // CPU count; clamped to 0..=100 to satisfy the protocol invariant.
    let loadavg = read_utf8_file_bounded(Path::new("/proc/loadavg"), MAX_PROC_TEXT_BYTES).ok();
    let cpu_percent = loadavg
        .as_deref()
        .and_then(parse_loadavg_one_minute)
        .map(|load| load_to_cpu_percent(load, online_cpu_count()))
        .unwrap_or(0);
    Some(GuestMetricsConfig {
        cpu_percent,
        memory_used_mib,
    })
}

/// Used = MemTotal - MemAvailable (kB in /proc/meminfo), reported in MiB.
pub(crate) fn parse_memory_used_mib(meminfo: &str) -> Option<u64> {
    let mut total_kib = None;
    let mut available_kib = None;
    for line in meminfo.lines() {
        if let Some(value) = parse_meminfo_kib(line, "MemTotal:") {
            total_kib = Some(value);
        } else if let Some(value) = parse_meminfo_kib(line, "MemAvailable:") {
            available_kib = Some(value);
        }
    }
    let total = total_kib?;
    let available = available_kib?;
    let used_kib = total.saturating_sub(available);
    Some(used_kib / 1024)
}

pub(crate) fn parse_meminfo_kib(line: &str, key: &str) -> Option<u64> {
    let rest = line.strip_prefix(key)?;
    rest.split_whitespace().next()?.parse::<u64>().ok()
}

pub(crate) fn parse_loadavg_one_minute(loadavg: &str) -> Option<f64> {
    loadavg.split_whitespace().next()?.parse::<f64>().ok()
}

pub(crate) fn load_to_cpu_percent(load: f64, cpu_count: u64) -> u8 {
    let cpu_count = cpu_count.max(1) as f64;
    let percent = (load / cpu_count * 100.0).round();
    percent.clamp(0.0, 100.0) as u8
}

pub(crate) fn online_cpu_count() -> u64 {
    std::thread::available_parallelism()
        .map(|count| count.get() as u64)
        .unwrap_or(1)
}

pub(crate) fn normalize_clipboard_text(text: &str) -> Result<String> {
    let text = text.trim_end_matches(['\r', '\n']).to_string();
    if text.is_empty() {
        anyhow::bail!("clipboard text cannot be empty");
    }
    Ok(text)
}

pub(crate) fn supports_capability(capabilities: &[AgentCapability], name: &str) -> bool {
    capabilities
        .iter()
        .any(|capability| capability.name == name)
}

pub(crate) fn parse_guest_ip(value: &str) -> Result<GuestIpAddress> {
    let (address, interface) = value
        .split_once('@')
        .map_or((value, None), |(address, interface)| {
            (address, Some(interface))
        });
    let address = address
        .trim()
        .parse::<IpAddr>()
        .with_context(|| format!("invalid guest IP address '{address}'"))?;
    if address.is_unspecified() {
        anyhow::bail!("guest IP address cannot be unspecified");
    }
    let interface = interface
        .map(str::trim)
        .filter(|interface| !interface.is_empty())
        .map(ToString::to_string);

    Ok(GuestIpAddress { address, interface })
}

pub(crate) fn initial_status_envelopes(telemetry: &TelemetryConfig) -> Vec<AgentEnvelope> {
    let mut envelopes = vec![AgentEnvelope::new(AgentMessage::Heartbeat)];
    if !telemetry.guest_ips.is_empty() {
        envelopes.push(AgentEnvelope::new(AgentMessage::GuestIpChanged {
            addresses: telemetry.guest_ips.clone(),
        }));
    }
    if let Some(metrics) = telemetry.metrics {
        envelopes.push(AgentEnvelope::new(AgentMessage::GuestMetrics {
            cpu_percent: metrics.cpu_percent,
            memory_used_mib: metrics.memory_used_mib,
        }));
    }
    if let Some(text) = &telemetry.clipboard_text {
        envelopes.push(AgentEnvelope::new(AgentMessage::ClipboardChanged {
            text: text.clone(),
        }));
    }
    envelopes
}
