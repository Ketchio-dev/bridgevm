//! Split test module.

use crate::*;
use bridgevm_config::Guest;
use bridgevm_config::PortForward;
use bridgevm_config::VmManifest;
use bridgevm_config::VmMode;
use bridgevm_network::NetworkMode;
use bridgevm_network::NetworkPlanError;
use std::path::Path;
use std::path::PathBuf;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

pub(super) static TEMP_SOCKET_COUNTER: AtomicU64 = AtomicU64::new(0);

pub(super) fn arg_after<'a>(args: &'a [String], flag: &str) -> &'a str {
    args.windows(2)
        .find_map(|pair| (pair[0] == flag).then_some(pair[1].as_str()))
        .unwrap_or_else(|| panic!("missing {flag} argument"))
}

pub(super) fn win11_firmware_manifest() -> VmManifest {
    let mut manifest = VmManifest::new(
        "win11-arm",
        VmMode::Compatibility,
        Guest {
            os: "windows".to_string(),
            version: Some("11".to_string()),
            arch: "arm64".to_string(),
        },
        "128GiB",
    );
    manifest.display.renderer = "vnc".to_string();
    manifest
}

#[test]
fn qemu_network_planner_rejects_port_forwards_outside_nat() {
    let mut manifest = VmManifest::new(
        "legacy",
        VmMode::Compatibility,
        Guest {
            os: "ubuntu".to_string(),
            version: None,
            arch: "x86_64".to_string(),
        },
        "64GiB",
    );
    manifest.network.mode = "host-only".to_string();
    manifest.network.forwards.push(PortForward {
        host: 8080,
        guest: 80,
    });

    let error = build_compatibility_command(&manifest, Path::new("/tmp/legacy.vmbridge"))
        .expect_err("host-only port forwards must be rejected by the network planner");

    assert!(matches!(
        error,
        QemuError::NetworkPlan(NetworkPlanError::UnsupportedPortForwarding {
            mode: NetworkMode::HostOnly
        })
    ));
}

#[test]
fn qemu_netdev_maps_isolated_mode_from_network_plan() {
    let mut manifest = VmManifest::new(
        "legacy",
        VmMode::Compatibility,
        Guest {
            os: "ubuntu".to_string(),
            version: None,
            arch: "x86_64".to_string(),
        },
        "64GiB",
    );
    manifest.network.mode = "isolated".to_string();

    assert_eq!(
        netdev_arg(&manifest).expect("planned isolated netdev"),
        "user,id=net0,restrict=on"
    );
}

pub(super) fn temp_socket_path() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let counter = TEMP_SOCKET_COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "bridgevm-qmp-test-{}-{nanos}-{counter}.sock",
        std::process::id()
    ))
}
