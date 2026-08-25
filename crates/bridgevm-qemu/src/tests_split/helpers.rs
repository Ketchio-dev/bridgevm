//! Split test module.

use crate::*;
use bridgevm_config::Guest;
use bridgevm_config::PortForward;
use bridgevm_config::VmManifest;
use bridgevm_config::VmMode;
use bridgevm_network::NetworkMode;
use bridgevm_network::NetworkPlanError;
use std::os::unix::net::UnixListener;
use std::path::Path;
use std::path::PathBuf;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::time::Duration;

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

/// A unique Unix socket path inside `SUN_LEN`. macOS caps `sun_path` at 104
/// bytes and a longer `TMPDIR` surfaces as an unrelated `QmpIo(InvalidInput)`,
/// so the length is asserted rather than assumed.
pub(super) fn temp_socket_path() -> PathBuf {
    let counter = TEMP_SOCKET_COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("bv-qmp-{}-{counter}.sock", std::process::id()));
    assert!(
        path.as_os_str().len() < 104,
        "path exceeds SUN_LEN: {path:?}"
    );
    let _ = std::fs::remove_file(&path);
    path
}

/// The client's one-second read timeout is a production budget, not a property
/// these tests assert: a slow server thread turned an expected protocol error
/// into a timeout with a different message.
pub(super) const TEST_READ_TIMEOUT: Duration = Duration::from_secs(30);

pub(super) fn bound_client_for_test(socket_path: &Path) -> (UnixListener, QmpClient) {
    let listener = UnixListener::bind(socket_path).unwrap();
    let client = QmpClient::connect_with_timeout(socket_path, TEST_READ_TIMEOUT)
        .unwrap_or_else(|error| panic!("connect to {} failed: {error}", socket_path.display()));
    (listener, client)
}
