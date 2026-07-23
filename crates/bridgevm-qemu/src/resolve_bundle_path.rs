//! Split out of lib.rs to keep files under 800 lines.

use crate::*;
use bridgevm_config::VmManifest;
use bridgevm_network::plan_network;
use bridgevm_network::NetworkBackend;
use bridgevm_network::NetworkMode;
use bridgevm_network::NetworkPlan;
use bridgevm_network::PortForwardRule;
use std::path::Path;
use std::path::PathBuf;
use std::str::FromStr;

pub(crate) fn resolve_bundle_path(bundle_path: &Path, relative_or_absolute: &str) -> PathBuf {
    let path = PathBuf::from(relative_or_absolute);
    if path.is_absolute() {
        path
    } else {
        bundle_path.join(path)
    }
}

pub(crate) fn machine_for_arch(arch: &str) -> &'static str {
    match arch {
        "arm64" | "aarch64" | "riscv64" => "virt",
        _ => "q35",
    }
}

pub(crate) fn qemu_profile_for_manifest(manifest: &VmManifest) -> QemuProfile {
    if is_windows_11_arm(manifest) {
        QemuProfile::restricted_windows_arm()
    } else {
        QemuProfile::compatibility_default()
    }
}

pub(crate) fn is_windows_11_arm(manifest: &VmManifest) -> bool {
    let os = manifest.guest.os.to_ascii_lowercase();
    let version = manifest
        .guest
        .version
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let arch = manifest.guest.arch.to_ascii_lowercase();
    os == "windows" && version.starts_with("11") && matches!(arch.as_str(), "arm64" | "aarch64")
}

pub(crate) fn accelerator_arg(profile: &QemuProfile) -> &str {
    match profile.accelerator.as_str() {
        "hvf-or-tcg" => "hvf",
        accelerator => accelerator,
    }
}

/// Escape a value interpolated into a comma-delimited QEMU option string (e.g.
/// `-drive file=...`, `-chardev socket,path=...`). QEMU parses these option
/// strings on commas, so a literal comma in a (manifest-derived) path must be
/// doubled (`,,`) or it would inject additional QEMU options.
pub(crate) fn escape_qemu_opt(value: impl std::fmt::Display) -> String {
    value.to_string().replace(',', ",,")
}

pub(crate) fn memory_arg(value: &str) -> String {
    if value == "auto" {
        "4096".to_string()
    } else if value.ends_with("GiB") {
        value
            .trim_end_matches("GiB")
            .parse::<u64>()
            // checked_mul: a huge GiB value would otherwise panic (debug) or wrap
            // (release) into a garbage -m argument. On overflow, pass through.
            .ok()
            .and_then(|gib| gib.checked_mul(1024))
            .map(|mib| mib.to_string())
            .unwrap_or_else(|| value.to_string())
    } else if value.ends_with("MiB") {
        value.trim_end_matches("MiB").to_string()
    } else {
        value.to_string()
    }
}

pub(crate) fn cpu_arg(value: &str) -> String {
    if value == "auto" {
        "2".to_string()
    } else {
        value.to_string()
    }
}

pub(crate) fn display_arg(renderer: &str) -> &'static str {
    match renderer {
        "spice" => "default,show-cursor=on",
        "spice-or-vnc" => "vnc=:0",
        "vnc" => "vnc=:0",
        "metal-adapter-preferred" => "cocoa,gl=on",
        _ => "default,show-cursor=on",
    }
}

/// The TCP port VNC display `:0` listens on; display `:N` listens on
/// `VNC_BASE_PORT + N`.
pub(crate) const VNC_BASE_PORT: u16 = 5900;
/// How many VNC display numbers to scan for a free one before giving up.
pub(crate) const VNC_DISPLAY_SCAN_LIMIT: u16 = 64;

/// Move a built command's `-display vnc=:0` onto the lowest free VNC display
/// number, so concurrently running Compatibility Mode VMs don't collide on TCP
/// 5900 (the second QEMU would otherwise fail to start with "Failed to find an
/// available port: Address already in use"). The command builder is kept pure
/// and deterministic (always `vnc=:0`); spawn paths call this just before they
/// launch + record the command, so the recorded `-display` reflects the chosen
/// display (the macOS app's viewer endpoint reads `vnc=:N` back to compute the
/// VNC port).
///
/// `avoid` lists display numbers already handed to other live VMs. This is
/// required because a VM's QEMU does not bind its VNC port until partway through
/// startup, so a pure "is the port free right now" probe would hand the same
/// `:0` to two VMs launched back-to-back (the second would then lose the race
/// and fail to start). The caller passes the displays of its running backends so
/// each new VM gets a distinct one.
///
/// Returns `Ok(())` after assigning a display (or as a no-op for a non-VNC
/// display). Returns `Err` if this IS a VNC display but no free number exists in
/// range — so the spawn fails loudly instead of silently leaving the colliding
/// `vnc=:0` template (which would re-introduce the very "Address already in use"
/// crash this function exists to prevent).
pub fn assign_free_vnc_display(command: &mut QemuCommand, avoid: &[u16]) -> Result<(), String> {
    let Some(index) = command.args.iter().position(|arg| arg == "-display") else {
        return Ok(());
    };
    let Some(value) = command.args.get(index + 1) else {
        return Ok(());
    };
    if !value.starts_with("vnc=:") {
        return Ok(());
    }
    let display = lowest_free_vnc_display(VNC_DISPLAY_SCAN_LIMIT, avoid).ok_or_else(|| {
        format!(
            "no free VNC display in range :0..:{VNC_DISPLAY_SCAN_LIMIT} (in use/avoided: {avoid:?}); too many Compatibility Mode VMs are running at once"
        )
    })?;
    command.args[index + 1] = format!("vnc=:{display}");
    Ok(())
}

/// Extract the VNC display number from a rendered command's `-display vnc=:N`
/// (used to collect the displays already in use by running VMs). Returns `None`
/// for a non-VNC display or a malformed value.
pub fn vnc_display_in_command(args: &[String]) -> Option<u16> {
    let index = args.iter().position(|arg| arg == "-display")?;
    args.get(index + 1)?.strip_prefix("vnc=:")?.parse().ok()
}

/// Find the lowest VNC display number that is not in `avoid` and whose TCP port
/// is bindable (free).
pub(crate) fn lowest_free_vnc_display(scan_limit: u16, avoid: &[u16]) -> Option<u16> {
    use std::net::TcpListener;
    (0..scan_limit).find(|display| {
        !avoid.contains(display)
            && TcpListener::bind(("127.0.0.1", VNC_BASE_PORT + display)).is_ok()
    })
}

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
