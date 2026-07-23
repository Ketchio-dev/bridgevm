//! Manifest-to-network plan and the -netdev argument for each network mode.

use crate::*;
use bridgevm_config::VmManifest;
use bridgevm_network::plan_network;
use bridgevm_network::NetworkBackend;
use bridgevm_network::NetworkMode;
use bridgevm_network::NetworkPlan;
use bridgevm_network::PortForwardRule;
use std::str::FromStr;

pub(crate) fn netdev_arg(manifest: &VmManifest) -> Result<String, QemuError> {
    let plan = qemu_network_plan(manifest)?;
    let mut arg = match plan.mode {
        NetworkMode::Nat => "user,id=net0".to_string(),
        NetworkMode::HostOnly => "vmnet-host,id=net0".to_string(),
        NetworkMode::Isolated => "user,id=net0,restrict=on".to_string(),
        // Bridged guests attach directly to a host interface via QEMU's
        // vmnet-bridged netdev and receive a real LAN IP (DHCP from the LAN),
        // so there is no NAT/hostfwd here -- the planner already rejects port
        // forwards for any non-NAT mode, so `plan.port_forwards` is empty below.
        // vmnet-bridged additionally requires the qemu process to run as root
        // or carry the com.apple.vm.networking entitlement; that runtime
        // privilege requirement is surfaced through the network plan
        // (`requires_privileged_helper` + the bridged requirement), not by
        // failing arg generation.
        NetworkMode::Bridged => format!(
            "vmnet-bridged,id=net0,ifname={}",
            escape_qemu_opt(manifest.network.bridge_interface())
        ),
        // Advanced networking is intentionally open-ended and has no settled
        // schema, so it remains unsupported at the arg-builder level.
        NetworkMode::Advanced => {
            let requirement = plan.requirements.first().cloned().unwrap_or_else(|| {
                bridgevm_network::NetworkRequirement {
                    blocker: "qemu-advanced-network-requires-schema".to_string(),
                    requirement:
                        "Compatibility Mode QEMU requires an advanced network schema and launcher wiring before launch"
                            .to_string(),
                }
            });
            return Err(QemuError::UnsupportedNetworkRequirement {
                mode: plan.mode.to_string(),
                blocker: requirement.blocker,
                requirement: requirement.requirement,
            });
        }
    };
    for forward in &plan.port_forwards {
        arg.push_str(&format!(
            ",hostfwd=tcp::{}-:{}",
            forward.host, forward.guest
        ));
    }
    Ok(arg)
}

pub(crate) fn qemu_network_plan(manifest: &VmManifest) -> Result<NetworkPlan, QemuError> {
    let mode = NetworkMode::from_str(&manifest.network.mode)?;
    let port_forwards = manifest
        .network
        .forwards
        .iter()
        .map(|forward| PortForwardRule {
            host: forward.host,
            guest: forward.guest,
        })
        .collect();

    Ok(plan_network(
        NetworkBackend::Qemu,
        mode,
        manifest.network.hostname.clone(),
        port_forwards,
    )?)
}
