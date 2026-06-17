use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, fmt, str::FromStr};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NetworkMode {
    Nat,
    HostOnly,
    Isolated,
    Bridged,
    Advanced,
}

impl fmt::Display for NetworkMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NetworkMode::Nat => write!(f, "nat"),
            NetworkMode::HostOnly => write!(f, "host-only"),
            NetworkMode::Isolated => write!(f, "isolated"),
            NetworkMode::Bridged => write!(f, "bridged"),
            NetworkMode::Advanced => write!(f, "advanced"),
        }
    }
}

impl FromStr for NetworkMode {
    type Err = NetworkPlanError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "nat" => Ok(NetworkMode::Nat),
            "host-only" => Ok(NetworkMode::HostOnly),
            "isolated" => Ok(NetworkMode::Isolated),
            "bridged" => Ok(NetworkMode::Bridged),
            "advanced" => Ok(NetworkMode::Advanced),
            _ => Err(NetworkPlanError::UnsupportedModeName(value.to_string())),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortForwardRule {
    pub host: u16,
    pub guest: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NetworkBackend {
    AppleVz,
    Qemu,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkPlan {
    pub mode: NetworkMode,
    pub backend: NetworkBackend,
    pub hostname: String,
    pub port_forwards: Vec<PortForwardRule>,
    pub capabilities: NetworkCapabilities,
    pub requirements: Vec<NetworkRequirement>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkCapabilities {
    pub guest_outbound: bool,
    pub host_to_guest: bool,
    pub guest_to_host: bool,
    pub host_visible_hostname: bool,
    pub supports_port_forwarding: bool,
    pub requires_privileged_helper: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkRequirement {
    pub blocker: String,
    pub requirement: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetworkPlanError {
    UnsupportedModeName(String),
    EmptyHostname,
    InvalidPortForward {
        host: u16,
        guest: u16,
    },
    DuplicateHostPort {
        host: u16,
    },
    UnsupportedMode {
        backend: NetworkBackend,
        mode: NetworkMode,
    },
    UnsupportedPortForwarding {
        mode: NetworkMode,
    },
}

impl fmt::Display for NetworkPlanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NetworkPlanError::UnsupportedModeName(mode) => {
                write!(f, "unsupported network mode {mode}")
            }
            NetworkPlanError::EmptyHostname => write!(f, "network hostname cannot be empty"),
            NetworkPlanError::InvalidPortForward { host, guest } => {
                write!(
                    f,
                    "invalid port forward {host}:{guest}; ports must be between 1 and 65535"
                )
            }
            NetworkPlanError::DuplicateHostPort { host } => {
                write!(f, "host port {host} is already forwarded")
            }
            NetworkPlanError::UnsupportedMode { backend, mode } => {
                write!(f, "{backend:?} does not support {mode} networking yet")
            }
            NetworkPlanError::UnsupportedPortForwarding { mode } => {
                write!(f, "{mode} networking does not support port forwarding")
            }
        }
    }
}

impl std::error::Error for NetworkPlanError {}

pub fn default_hostname(vm_name: &str) -> String {
    format!("{}.bridgevm.local", bridgevm_config_like_slug(vm_name))
}

pub fn plan_network(
    backend: NetworkBackend,
    mode: NetworkMode,
    hostname: impl Into<String>,
    port_forwards: Vec<PortForwardRule>,
) -> Result<NetworkPlan, NetworkPlanError> {
    let hostname = hostname.into();
    if hostname.trim().is_empty() {
        return Err(NetworkPlanError::EmptyHostname);
    }
    validate_backend_mode(backend, &mode)?;
    validate_port_forwards(&mode, &port_forwards)?;

    let capabilities = capabilities_for(backend, &mode);
    let requirements = requirements_for(backend, &mode);
    let notes = notes_for(&mode, &port_forwards);

    Ok(NetworkPlan {
        mode,
        backend,
        hostname,
        port_forwards,
        capabilities,
        requirements,
        notes,
    })
}

pub fn validate_port_forwards(
    mode: &NetworkMode,
    rules: &[PortForwardRule],
) -> Result<(), NetworkPlanError> {
    if !matches!(mode, NetworkMode::Nat) && !rules.is_empty() {
        return Err(NetworkPlanError::UnsupportedPortForwarding { mode: mode.clone() });
    }

    let mut seen_host_ports = BTreeSet::new();
    for rule in rules {
        if rule.host == 0 || rule.guest == 0 {
            return Err(NetworkPlanError::InvalidPortForward {
                host: rule.host,
                guest: rule.guest,
            });
        }
        if !seen_host_ports.insert(rule.host) {
            return Err(NetworkPlanError::DuplicateHostPort { host: rule.host });
        }
    }
    Ok(())
}

fn validate_backend_mode(
    backend: NetworkBackend,
    mode: &NetworkMode,
) -> Result<(), NetworkPlanError> {
    match (backend, mode) {
        (NetworkBackend::AppleVz, NetworkMode::Bridged) => Err(NetworkPlanError::UnsupportedMode {
            backend,
            mode: mode.clone(),
        }),
        _ => Ok(()),
    }
}

fn capabilities_for(backend: NetworkBackend, mode: &NetworkMode) -> NetworkCapabilities {
    NetworkCapabilities {
        guest_outbound: matches!(mode, NetworkMode::Nat | NetworkMode::Bridged),
        host_to_guest: matches!(
            mode,
            NetworkMode::Nat | NetworkMode::HostOnly | NetworkMode::Bridged
        ),
        guest_to_host: !matches!(mode, NetworkMode::Isolated),
        host_visible_hostname: matches!(
            mode,
            NetworkMode::Nat | NetworkMode::HostOnly | NetworkMode::Bridged
        ),
        supports_port_forwarding: matches!(mode, NetworkMode::Nat),
        requires_privileged_helper: matches!(
            (backend, mode),
            (
                NetworkBackend::Qemu,
                NetworkMode::HostOnly | NetworkMode::Bridged | NetworkMode::Advanced
            )
        ),
    }
}

/// Blocker code for QEMU bridged networking. Bridged args ARE generated (a
/// `vmnet-bridged` netdev), but `vmnet-bridged` on macOS requires the qemu
/// process to run as root or carry the `com.apple.vm.networking` entitlement,
/// so this is a privilege requirement rather than an unimplemented feature.
pub const QEMU_BRIDGED_PRIVILEGE_BLOCKER: &str = "qemu-bridged-requires-privilege";

/// Honest requirement text for [`QEMU_BRIDGED_PRIVILEGE_BLOCKER`].
pub const QEMU_BRIDGED_PRIVILEGE_REQUIREMENT: &str =
    "Compatibility Mode QEMU bridged networking uses vmnet-bridged, which requires the qemu \
process to run as root or carry the com.apple.vm.networking entitlement";

fn requirements_for(backend: NetworkBackend, mode: &NetworkMode) -> Vec<NetworkRequirement> {
    match (backend, mode) {
        (NetworkBackend::Qemu, NetworkMode::Bridged) => vec![NetworkRequirement {
            blocker: QEMU_BRIDGED_PRIVILEGE_BLOCKER.to_string(),
            requirement: QEMU_BRIDGED_PRIVILEGE_REQUIREMENT.to_string(),
        }],
        (NetworkBackend::Qemu, NetworkMode::Advanced) => vec![NetworkRequirement {
            blocker: "qemu-advanced-network-unimplemented".to_string(),
            requirement:
                "Compatibility Mode QEMU requires an advanced network schema and launcher wiring before launch"
                    .to_string(),
        }],
        _ => Vec::new(),
    }
}

fn notes_for(mode: &NetworkMode, port_forwards: &[PortForwardRule]) -> Vec<String> {
    let mut notes = Vec::new();
    match mode {
        NetworkMode::Nat => {
            notes.push("default NAT networking with automatic DNS intent".to_string())
        }
        NetworkMode::HostOnly => {
            notes.push("host-only network intent; guest outbound internet is disabled".to_string())
        }
        NetworkMode::Isolated => {
            notes.push("isolated VM network intent; no host or internet reachability".to_string())
        }
        NetworkMode::Bridged => {
            notes.push("bridged network intent; privileged helper may be required".to_string())
        }
        NetworkMode::Advanced => {
            notes.push("advanced network intent; backend-specific wiring is required".to_string())
        }
    }
    if !port_forwards.is_empty() {
        notes.push(
            "port forwards are planning-time rules consumed by backend launchers".to_string(),
        );
    }
    notes
}

fn bridgevm_config_like_slug(value: &str) -> String {
    value
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_hostname_slugifies_vm_name() {
        assert_eq!(default_hostname("Ubuntu Dev"), "ubuntu-dev.bridgevm.local");
        assert_eq!(default_hostname("  QA__VM  "), "qa-vm.bridgevm.local");
    }

    #[test]
    fn plans_nat_with_port_forward_capabilities() {
        let plan = plan_network(
            NetworkBackend::Qemu,
            NetworkMode::Nat,
            "legacy.bridgevm.local",
            vec![PortForwardRule {
                host: 2222,
                guest: 22,
            }],
        )
        .unwrap();

        assert_eq!(plan.backend, NetworkBackend::Qemu);
        assert_eq!(plan.mode, NetworkMode::Nat);
        assert!(plan.capabilities.guest_outbound);
        assert!(plan.capabilities.host_to_guest);
        assert!(plan.capabilities.supports_port_forwarding);
        assert!(!plan.capabilities.requires_privileged_helper);
        assert!(plan
            .notes
            .iter()
            .any(|note| note.contains("planning-time rules")));
    }

    #[test]
    fn rejects_duplicate_or_zero_port_forwards() {
        let duplicate = plan_network(
            NetworkBackend::Qemu,
            NetworkMode::Nat,
            "legacy.bridgevm.local",
            vec![
                PortForwardRule {
                    host: 2222,
                    guest: 22,
                },
                PortForwardRule {
                    host: 2222,
                    guest: 8080,
                },
            ],
        )
        .unwrap_err();
        assert_eq!(
            duplicate,
            NetworkPlanError::DuplicateHostPort { host: 2222 }
        );

        let zero = plan_network(
            NetworkBackend::Qemu,
            NetworkMode::Nat,
            "legacy.bridgevm.local",
            vec![PortForwardRule { host: 0, guest: 22 }],
        )
        .unwrap_err();
        assert_eq!(
            zero,
            NetworkPlanError::InvalidPortForward { host: 0, guest: 22 }
        );
    }

    #[test]
    fn rejects_port_forwards_outside_nat() {
        let error = plan_network(
            NetworkBackend::AppleVz,
            NetworkMode::HostOnly,
            "dev.bridgevm.local",
            vec![PortForwardRule {
                host: 8080,
                guest: 80,
            }],
        )
        .unwrap_err();

        assert_eq!(
            error,
            NetworkPlanError::UnsupportedPortForwarding {
                mode: NetworkMode::HostOnly
            }
        );
    }

    #[test]
    fn records_mode_capability_boundaries() {
        let host_only = plan_network(
            NetworkBackend::AppleVz,
            NetworkMode::HostOnly,
            "dev.bridgevm.local",
            Vec::new(),
        )
        .unwrap();
        assert!(!host_only.capabilities.guest_outbound);
        assert!(host_only.capabilities.host_to_guest);
        assert!(!host_only.capabilities.requires_privileged_helper);
        assert_eq!(
            host_only
                .requirements
                .first()
                .map(|requirement| requirement.blocker.as_str()),
            None
        );

        let qemu_host_only = plan_network(
            NetworkBackend::Qemu,
            NetworkMode::HostOnly,
            "legacy.bridgevm.local",
            Vec::new(),
        )
        .unwrap();
        assert!(qemu_host_only.capabilities.requires_privileged_helper);
        assert!(qemu_host_only.requirements.is_empty());

        let isolated = plan_network(
            NetworkBackend::Qemu,
            NetworkMode::Isolated,
            "lab.bridgevm.local",
            Vec::new(),
        )
        .unwrap();
        assert!(!isolated.capabilities.guest_outbound);
        assert!(!isolated.capabilities.host_to_guest);
        assert!(!isolated.capabilities.guest_to_host);

        let advanced = plan_network(
            NetworkBackend::Qemu,
            NetworkMode::Advanced,
            "lab.bridgevm.local",
            Vec::new(),
        )
        .unwrap();
        assert!(advanced.capabilities.requires_privileged_helper);
        assert_eq!(
            advanced
                .requirements
                .first()
                .map(|requirement| requirement.blocker.as_str()),
            Some("qemu-advanced-network-unimplemented")
        );
    }

    #[test]
    fn qemu_bridged_is_supported_and_requires_privilege() {
        let plan = plan_network(
            NetworkBackend::Qemu,
            NetworkMode::Bridged,
            "legacy.bridgevm.local",
            Vec::new(),
        )
        .expect("QEMU bridged networking is supported");

        assert_eq!(plan.mode, NetworkMode::Bridged);
        // Bridged guests get a real LAN IP: outbound + host reachability, no
        // NAT port forwarding.
        assert!(plan.capabilities.guest_outbound);
        assert!(plan.capabilities.host_to_guest);
        assert!(!plan.capabilities.supports_port_forwarding);
        // vmnet-bridged needs elevated privilege at runtime.
        assert!(plan.capabilities.requires_privileged_helper);

        // The remaining requirement is an HONEST privilege requirement, not an
        // "unimplemented" blocker.
        let requirement = plan
            .requirements
            .first()
            .expect("bridged carries a privilege requirement");
        assert_eq!(requirement.blocker, QEMU_BRIDGED_PRIVILEGE_BLOCKER);
        assert!(!requirement.blocker.contains("unimplemented"));
        assert!(requirement.requirement.contains("root"));
        assert!(requirement
            .requirement
            .contains("com.apple.vm.networking"));
    }

    #[test]
    fn apple_vz_rejects_bridged_until_backend_support_exists() {
        let error = plan_network(
            NetworkBackend::AppleVz,
            NetworkMode::Bridged,
            "dev.bridgevm.local",
            Vec::new(),
        )
        .unwrap_err();

        assert_eq!(
            error,
            NetworkPlanError::UnsupportedMode {
                backend: NetworkBackend::AppleVz,
                mode: NetworkMode::Bridged
            }
        );
    }
}
