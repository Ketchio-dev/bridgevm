//! Split test module.

use crate::*;
use bridgevm_api::BridgeVmRequest;
use bridgevm_api::BridgeVmResponse;
use bridgevm_config::BootMode;
use bridgevm_config::Guest;
use bridgevm_config::VmManifest;
use bridgevm_config::VmMode;
use bridgevm_storage::VmStore;
use std::env;
use std::fs;
use std::io::BufRead;
use std::io::BufReader;
use std::io::Read;
use std::io::Write;
use std::net::TcpListener;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::sync::Mutex;
use std::thread::JoinHandle;
use std::time::Duration;

pub(super) static TEST_ID: AtomicU64 = AtomicU64::new(0);
pub(super) static PROXY_WINDOW_ENV_LOCK: Mutex<()> = Mutex::new(());

#[test]
pub(super) fn bounded_command_output_captures_both_streams() {
    let mut command = Command::new("/bin/sh");
    command.args(["-c", "printf hello; printf warning >&2"]);

    let output = run_bounded_command_output(command, Duration::from_secs(1), 1024).unwrap();

    assert!(output.status.success());
    assert_eq!(output.stdout, b"hello");
    assert_eq!(output.stderr, b"warning");
}

pub(super) struct EnvVarGuard {
    pub(super) saved: Vec<(&'static str, Option<std::ffi::OsString>)>,
}

impl EnvVarGuard {
    pub(super) fn capture(keys: &[&'static str]) -> Self {
        Self {
            saved: keys.iter().map(|key| (*key, env::var_os(key))).collect(),
        }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        for (key, value) in self.saved.iter().rev() {
            if let Some(value) = value {
                env::set_var(key, value);
            } else {
                env::remove_var(key);
            }
        }
    }
}

pub(super) fn temp_store() -> VmStore {
    let mut path = PathBuf::from("/tmp");
    let id = TEST_ID.fetch_add(1, Ordering::Relaxed);
    path.push(format!("bvmd-{}-{}", std::process::id(), id));
    VmStore::new(path)
}

pub(super) fn compatibility_manifest(name: &str) -> VmManifest {
    VmManifest::new(
        name,
        VmMode::Compatibility,
        Guest {
            os: "ubuntu".to_string(),
            version: None,
            arch: "x86_64".to_string(),
        },
        "64GiB",
    )
}

pub(super) fn fast_manifest(name: &str) -> VmManifest {
    VmManifest::new(
        name,
        VmMode::Fast,
        Guest {
            os: "ubuntu".to_string(),
            version: None,
            arch: "arm64".to_string(),
        },
        "80GiB",
    )
}

pub(super) fn ready_fast_manifest(name: &str) -> VmManifest {
    let mut manifest = fast_manifest(name);
    manifest.storage.primary.path = "disks/root.raw".to_string();
    manifest.storage.primary.format = "raw".to_string();
    manifest.boot = Some(bridgevm_config::Boot {
        mode: BootMode::LinuxKernel,
        installer_image: None,
        kernel_path: Some("boot/vmlinuz".to_string()),
        initrd_path: None,
        kernel_command_line: Some("console=hvc0 root=/dev/vda rw".to_string()),
        macos_restore_image: None,
    });
    manifest
}

pub(super) fn write_executable(path: &Path, contents: &str) {
    fs::write(path, contents).unwrap();
    let mut permissions = fs::metadata(path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).unwrap();
}

pub(super) fn solid_rgba(width: u32, height: u32, rgba: [u8; 4]) -> Vec<u8> {
    rgba.repeat((width * height) as usize)
}

pub(super) fn daemon_request(store: VmStore, request: BridgeVmRequest) -> BridgeVmResponse {
    let (mut client, server) = UnixStream::pair().unwrap();
    serde_json::to_writer(&mut client, &request).unwrap();
    client.write_all(b"\n").unwrap();

    let mut state = DaemonState::new(store);
    handle_connection(&mut state, server).unwrap();

    let mut line = String::new();
    BufReader::new(client).read_line(&mut line).unwrap();
    serde_json::from_str(line.trim_end()).unwrap()
}

pub(super) fn serve_one_http_response(body: &'static [u8]) -> (String, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind local test server");
    let address = listener
        .local_addr()
        .expect("read local test server address");
    let handle = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept curl connection");
        let mut request = [0_u8; 1024];
        let _ = stream.read(&mut request);
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .expect("write http response headers");
        stream.write_all(body).expect("write http response body");
    });
    (format!("http://{address}/ubuntu.iso"), handle)
}

#[test]
fn bundled_helper_discovery_rejects_non_executable_siblings() {
    let store = temp_store();
    let helpers = store.root().join("BridgeVM.app/Contents/Helpers");
    fs::create_dir_all(&helpers).unwrap();
    let bridgevmd = helpers.join("bridgevmd");
    let apple_vz_runner = helpers.join("AppleVzRunner");
    write_executable(&bridgevmd, "#!/bin/sh\n");
    fs::write(&apple_vz_runner, b"not executable").unwrap();

    assert_eq!(
        bundled_helper_path_from_exe(&bridgevmd, "AppleVzRunner"),
        None
    );

    fs::remove_dir_all(store.root()).unwrap();
}

#[test]
fn fast_spawn_config_validate_rejects_non_executable_apple_vz_runner() {
    let store = temp_store();
    fs::create_dir_all(store.root()).unwrap();
    let lightvm_runner = store.root().join("fake-lightvm-runner");
    let apple_vz_runner = store.root().join("fake-AppleVzRunner");
    write_executable(&lightvm_runner, "#!/bin/sh\n");
    fs::write(&apple_vz_runner, b"not executable").unwrap();

    let error = FastModeSpawnConfig {
        lightvm_runner,
        apple_vz_runner: apple_vz_runner.clone(),
        stop_after_seconds: None,
        force_stop_grace_seconds: None,
        verify_apple_vz_runner_entitlement: false,
    }
    .validate()
    .unwrap_err();
    let message = format!("{error:#}");

    assert!(
        message.contains("BRIDGEVM_APPLE_VZ_RUNNER/AppleVzRunner is not executable"),
        "{message}"
    );
    assert!(message.contains(&apple_vz_runner.display().to_string()));

    fs::remove_dir_all(store.root()).unwrap();
}
