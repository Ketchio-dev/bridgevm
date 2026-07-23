//! NAT-only network plan derivation for the Apple VZ backend.

use crate::*;
use bridgevm_config::VmManifest;
use bridgevm_network::plan_network;
use bridgevm_network::NetworkBackend;
use bridgevm_network::NetworkMode;
use bridgevm_network::NetworkPlan;
use bridgevm_network::NetworkPlanError;
use bridgevm_network::PortForwardRule;
use std::str::FromStr;

pub(crate) fn apple_vz_network_plan(manifest: &VmManifest) -> Result<NetworkPlan, AppleVzError> {
    let mode = NetworkMode::from_str(&manifest.network.mode)
        .map_err(|_| AppleVzError::UnsupportedNetworkMode(manifest.network.mode.clone()))?;
    let port_forwards = manifest
        .network
        .forwards
        .iter()
        .map(|forward| PortForwardRule {
            host: forward.host,
            guest: forward.guest,
        })
        .collect();
    let plan = plan_network(
        NetworkBackend::AppleVz,
        mode,
        manifest.network.hostname.clone(),
        port_forwards,
    )
    .map_err(|error| match error {
        NetworkPlanError::UnsupportedMode { mode, .. }
        | NetworkPlanError::UnsupportedPortForwarding { mode } => {
            AppleVzError::UnsupportedNetworkMode(mode.to_string())
        }
        other => AppleVzError::NetworkPlan(other),
    })?;

    if plan.mode != NetworkMode::Nat {
        return Err(AppleVzError::UnsupportedNetworkMode(plan.mode.to_string()));
    }

    Ok(plan)
}
