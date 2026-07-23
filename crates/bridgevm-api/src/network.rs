//! Split out of lib.rs by responsibility.

use crate::*;

pub fn list_ports(store: &VmStore, name: &str) -> Result<PortForwardListRecord, String> {
    let (_, manifest) = store.get_vm(name).map_err(|error| error.to_string())?;
    Ok(port_forward_list(name, &manifest.network.forwards))
}

pub fn add_port(
    store: &VmStore,
    name: &str,
    host: u16,
    guest: u16,
) -> Result<PortForwardListRecord, String> {
    let (bundle, mut manifest) = store.get_vm(name).map_err(|error| error.to_string())?;
    manifest.network.forwards.push(PortForward { host, guest });
    validate_network_plan(&manifest)?;
    manifest
        .network
        .forwards
        .sort_by_key(|forward| (forward.host, forward.guest));
    manifest
        .write(&bundle.join("manifest.yaml"))
        .map_err(|error| error.to_string())?;
    Ok(port_forward_list(name, &manifest.network.forwards))
}

pub fn remove_port(
    store: &VmStore,
    name: &str,
    host: u16,
    guest: u16,
) -> Result<PortForwardListRecord, String> {
    validate_port_pair(host, guest)?;
    let (bundle, mut manifest) = store.get_vm(name).map_err(|error| error.to_string())?;
    let initial_len = manifest.network.forwards.len();
    manifest
        .network
        .forwards
        .retain(|forward| !(forward.host == host && forward.guest == guest));
    if manifest.network.forwards.len() == initial_len {
        return Err(format!("port forward {host}:{guest} is not configured"));
    }
    manifest
        .write(&bundle.join("manifest.yaml"))
        .map_err(|error| error.to_string())?;
    Ok(port_forward_list(name, &manifest.network.forwards))
}

pub fn network_plan(store: &VmStore, name: &str) -> Result<NetworkPlanRecord, String> {
    let (_, manifest) = store.get_vm(name).map_err(|error| error.to_string())?;
    Ok(network_plan_for_manifest(&manifest))
}

pub(crate) fn network_plan_for_manifest(manifest: &VmManifest) -> NetworkPlanRecord {
    let backend = CurrentRuntimeEngine::for_manifest(manifest).network_backend();
    let port_forwards = manifest
        .network
        .forwards
        .iter()
        .map(|forward| PortForwardRecord {
            host: forward.host,
            guest: forward.guest,
        })
        .collect::<Vec<_>>();
    let rules = manifest
        .network
        .forwards
        .iter()
        .map(|forward| PortForwardRule {
            host: forward.host,
            guest: forward.guest,
        })
        .collect::<Vec<_>>();
    let hostname = manifest.network.hostname.clone();
    let mut blockers = Vec::new();
    let mut notes = vec![
        "dry-run network plan; no backend launch or host networking mutation was performed"
            .to_string(),
    ];
    let mut capabilities = None;

    match manifest.network.mode.parse::<NetworkMode>() {
        Ok(mode) => match plan_network(backend, mode.clone(), hostname.clone(), rules) {
            Ok(plan) => {
                capabilities = Some(network_capabilities_record(&plan.capabilities));
                blockers.extend(plan.requirements.into_iter().map(|requirement| {
                    NetworkPlanBlockerRecord {
                        code: requirement.blocker,
                        message: requirement.requirement,
                    }
                }));
                notes.extend(plan.notes);
            }
            Err(error) => blockers.push(network_plan_error_blocker(error)),
        },
        Err(error) => blockers.push(network_plan_error_blocker(error)),
    }

    NetworkPlanRecord {
        vm: manifest.name.clone(),
        backend: network_backend_label(backend).to_string(),
        mode: manifest.network.mode.clone(),
        hostname,
        dry_run: true,
        executable: blockers.is_empty(),
        port_forwards,
        capabilities,
        blockers,
        notes,
    }
}

pub(crate) fn network_capabilities_record(
    capabilities: &NetworkCapabilities,
) -> NetworkCapabilitiesRecord {
    NetworkCapabilitiesRecord {
        guest_outbound: capabilities.guest_outbound,
        host_to_guest: capabilities.host_to_guest,
        guest_to_host: capabilities.guest_to_host,
        host_visible_hostname: capabilities.host_visible_hostname,
        supports_port_forwarding: capabilities.supports_port_forwarding,
        requires_privileged_helper: capabilities.requires_privileged_helper,
    }
}

pub(crate) fn network_backend_label(backend: NetworkBackend) -> &'static str {
    match backend {
        NetworkBackend::AppleVz => "apple-vz",
        NetworkBackend::Qemu => "qemu",
    }
}

pub(crate) fn network_plan_error_blocker(error: NetworkPlanError) -> NetworkPlanBlockerRecord {
    let code = match &error {
        NetworkPlanError::UnsupportedModeName(_) => "unsupported-network-mode",
        NetworkPlanError::EmptyHostname => "empty-network-hostname",
        NetworkPlanError::InvalidPortForward { .. } => "invalid-port-forward",
        NetworkPlanError::DuplicateHostPort { .. } => "duplicate-host-port",
        NetworkPlanError::UnsupportedMode { .. } => "unsupported-network-backend-mode",
        NetworkPlanError::UnsupportedPortForwarding { .. } => "unsupported-port-forwarding",
    };
    NetworkPlanBlockerRecord {
        code: code.to_string(),
        message: error.to_string(),
    }
}

pub(crate) fn validate_port_pair(host: u16, guest: u16) -> Result<(), String> {
    if host == 0 || guest == 0 {
        return Err("ports must be between 1 and 65535".to_string());
    }
    Ok(())
}

pub(crate) fn validate_network_plan(manifest: &VmManifest) -> Result<(), String> {
    let mode = manifest
        .network
        .mode
        .parse::<NetworkMode>()
        .map_err(|error| error.to_string())?;
    let backend = CurrentRuntimeEngine::for_manifest(manifest).network_backend();
    let forwards = manifest
        .network
        .forwards
        .iter()
        .map(|forward| PortForwardRule {
            host: forward.host,
            guest: forward.guest,
        })
        .collect();
    plan_network(backend, mode, manifest.network.hostname.clone(), forwards)
        .map(|_| ())
        .map_err(|error| error.to_string())
}

pub(crate) fn port_forward_list(name: &str, forwards: &[PortForward]) -> PortForwardListRecord {
    PortForwardListRecord {
        vm: name.to_string(),
        forwards: forwards
            .iter()
            .map(|forward| PortForwardRecord {
                host: forward.host,
                guest: forward.guest,
            })
            .collect(),
    }
}
