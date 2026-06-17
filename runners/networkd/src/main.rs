use anyhow::{Context, Result};
use bridgevm_network::{plan_network, NetworkBackend, NetworkMode, NetworkPlan, PortForwardRule};
use clap::{Parser, ValueEnum};
use std::str::FromStr;

#[derive(Debug, Parser)]
#[command(name = "networkd", about = "BridgeVM network helper plan scaffold")]
struct Cli {
    #[arg(long)]
    print_plan: bool,
    #[arg(long, value_enum, default_value_t = BackendArg::Qemu)]
    backend: BackendArg,
    #[arg(long, value_enum, default_value_t = ModeArg::Nat)]
    mode: ModeArg,
    #[arg(long, default_value = "vm.bridgevm.local")]
    hostname: String,
    #[arg(long = "forward", value_name = "HOST:GUEST", value_parser = parse_forward)]
    forwards: Vec<PortForwardRule>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[value(rename_all = "kebab-case")]
enum BackendArg {
    Qemu,
    AppleVz,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[value(rename_all = "kebab-case")]
enum ModeArg {
    Nat,
    Isolated,
    HostOnly,
    Bridged,
    Advanced,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let plan = build_network_plan(&cli).context("failed to build network plan")?;

    if cli.print_plan {
        println!("{}", serde_json::to_string_pretty(&plan)?);
    } else {
        println!("{}", summary_line(&cli, &plan));
    }
    Ok(())
}

fn summary_line(cli: &Cli, plan: &NetworkPlan) -> String {
    if plan.requirements.is_empty() {
        format!(
            "networkd ready: {} backend, {} mode, {} forward rule(s)",
            cli.backend.as_str(),
            plan.mode,
            plan.port_forwards.len()
        )
    } else {
        format!(
            "networkd blocked: {} backend, {} mode, {} forward rule(s), {} requirement(s)",
            cli.backend.as_str(),
            plan.mode,
            plan.port_forwards.len(),
            plan.requirements.len()
        )
    }
}

fn build_network_plan(cli: &Cli) -> Result<NetworkPlan> {
    plan_network(
        cli.backend.into(),
        cli.mode.into(),
        cli.hostname.clone(),
        cli.forwards.clone(),
    )
    .map_err(Into::into)
}

fn parse_forward(value: &str) -> Result<PortForwardRule, String> {
    let (host, guest) = value
        .split_once(':')
        .ok_or_else(|| "forward must use HOST:GUEST".to_string())?;
    let host = parse_port(host).map_err(|error| format!("invalid host port: {error}"))?;
    let guest = parse_port(guest).map_err(|error| format!("invalid guest port: {error}"))?;
    Ok(PortForwardRule { host, guest })
}

fn parse_port(value: &str) -> Result<u16, String> {
    let port = u16::from_str(value).map_err(|_| format!("{value} is not a valid TCP port"))?;
    if port == 0 {
        return Err("port must be between 1 and 65535".to_string());
    }
    Ok(port)
}

impl BackendArg {
    fn as_str(self) -> &'static str {
        match self {
            BackendArg::Qemu => "qemu",
            BackendArg::AppleVz => "apple-vz",
        }
    }
}

impl From<BackendArg> for NetworkBackend {
    fn from(value: BackendArg) -> Self {
        match value {
            BackendArg::Qemu => NetworkBackend::Qemu,
            BackendArg::AppleVz => NetworkBackend::AppleVz,
        }
    }
}

impl From<ModeArg> for NetworkMode {
    fn from(value: ModeArg) -> Self {
        match value {
            ModeArg::Nat => NetworkMode::Nat,
            ModeArg::Isolated => NetworkMode::Isolated,
            ModeArg::HostOnly => NetworkMode::HostOnly,
            ModeArg::Bridged => NetworkMode::Bridged,
            ModeArg::Advanced => NetworkMode::Advanced,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cli(backend: BackendArg, mode: ModeArg) -> Cli {
        Cli {
            print_plan: true,
            backend,
            mode,
            hostname: "dev.bridgevm.local".to_string(),
            forwards: Vec::new(),
        }
    }

    #[test]
    fn parses_forward_rule() {
        assert_eq!(
            parse_forward("2222:22").unwrap(),
            PortForwardRule {
                host: 2222,
                guest: 22,
            }
        );
    }

    #[test]
    fn rejects_malformed_forward_rule() {
        assert!(parse_forward("2222").is_err());
        assert!(parse_forward("0:22").is_err());
        assert!(parse_forward("ssh:22").is_err());
    }

    #[test]
    fn builds_qemu_nat_plan_with_forwarding() {
        let mut cli = cli(BackendArg::Qemu, ModeArg::Nat);
        cli.forwards = vec![
            PortForwardRule {
                host: 2222,
                guest: 22,
            },
            PortForwardRule {
                host: 8080,
                guest: 80,
            },
        ];

        let plan = build_network_plan(&cli).unwrap();

        assert_eq!(plan.backend, NetworkBackend::Qemu);
        assert_eq!(plan.mode, NetworkMode::Nat);
        assert_eq!(plan.hostname, "dev.bridgevm.local");
        assert_eq!(plan.port_forwards.len(), 2);
        assert!(plan.capabilities.guest_outbound);
        assert!(plan.capabilities.supports_port_forwarding);
    }

    #[test]
    fn builds_apple_vz_host_only_plan() {
        let plan = build_network_plan(&cli(BackendArg::AppleVz, ModeArg::HostOnly)).unwrap();

        assert_eq!(plan.backend, NetworkBackend::AppleVz);
        assert_eq!(plan.mode, NetworkMode::HostOnly);
        assert!(plan.capabilities.host_to_guest);
        assert!(!plan.capabilities.guest_outbound);
    }

    #[test]
    fn summary_reports_requirements_as_blocked() {
        let cli = cli(BackendArg::Qemu, ModeArg::HostOnly);
        let plan = build_network_plan(&cli).unwrap();

        assert_eq!(
            summary_line(&cli, &plan),
            "networkd blocked: qemu backend, host-only mode, 0 forward rule(s), 1 requirement(s)"
        );
    }

    #[test]
    fn reuses_network_validation_for_unsupported_forwarding() {
        let mut cli = cli(BackendArg::Qemu, ModeArg::Isolated);
        cli.forwards = vec![PortForwardRule {
            host: 2222,
            guest: 22,
        }];

        let error = build_network_plan(&cli).unwrap_err().to_string();

        assert!(error.contains("does not support port forwarding"));
    }

    #[test]
    fn reuses_network_validation_for_apple_vz_bridged() {
        let error = build_network_plan(&cli(BackendArg::AppleVz, ModeArg::Bridged))
            .unwrap_err()
            .to_string();

        assert!(error.contains("AppleVz does not support bridged networking yet"));
    }

    #[test]
    fn clap_accepts_required_public_flags() {
        let cli = Cli::parse_from([
            "networkd",
            "--print-plan",
            "--backend",
            "apple-vz",
            "--mode",
            "nat",
            "--hostname",
            "ubuntu.bridgevm.local",
            "--forward",
            "2222:22",
        ]);

        assert!(cli.print_plan);
        assert_eq!(cli.backend, BackendArg::AppleVz);
        assert_eq!(cli.mode, ModeArg::Nat);
        assert_eq!(cli.hostname, "ubuntu.bridgevm.local");
        assert_eq!(
            cli.forwards[0],
            PortForwardRule {
                host: 2222,
                guest: 22
            }
        );
    }
}
