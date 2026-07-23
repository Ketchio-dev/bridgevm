//! Guest IP, metrics and clipboard telemetry config, /proc parsing, and the initial status envelopes.

use crate::*;
use anyhow::Context;
use anyhow::Result;
use bridgevm_agent_protocol::AgentCapability;
use bridgevm_agent_protocol::AgentEnvelope;
use bridgevm_agent_protocol::AgentMessage;
use bridgevm_agent_protocol::GuestIpAddress;
use std::net::IpAddr;
use std::path::Path;

pub(crate) const MAX_PROC_TEXT_BYTES: usize = 1024 * 1024;

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
