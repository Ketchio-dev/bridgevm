//! Shared fixtures for the crate's unit tests, factored out when the single
//! test module was split across the extracted modules.

#![cfg(test)]

pub(crate) use crate::*;
pub(crate) use bridgevm_agent_protocol::{AgentAuth, PROTOCOL_VERSION};
pub(crate) use bridgevm_storage::GuestToolsIpAddressMetadata;
pub(crate) use std::net::TcpListener;
pub(crate) use std::thread::JoinHandle;

// Serialize tests that mutate the process-global BRIDGEVM_APPLE_VZ_RUNNER
// env var so parallel test execution does not race on the gate.
pub(crate) static APPLE_VZ_RUNNER_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub(crate) struct EnvVarGuard {
    key: &'static str,
    previous: Option<std::ffi::OsString>,
}

impl EnvVarGuard {
    pub(crate) fn capture(key: &'static str) -> Self {
        Self {
            key,
            previous: std::env::var_os(key),
        }
    }

    pub(crate) fn set(key: &'static str, value: &str) -> Self {
        let guard = Self::capture(key);
        std::env::set_var(key, value);
        guard
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        match &self.previous {
            Some(value) => std::env::set_var(self.key, value),
            None => std::env::remove_var(self.key),
        }
    }
}

pub(crate) fn unique_runtime_control_test_socket(label: &str) -> PathBuf {
    let mut path = PathBuf::from("/tmp");
    path.push(format!(
        "bridgevm-api-rc-{label}-{}-{}.sock",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    path
}

pub(crate) fn fast_test_store(test: &str) -> (VmStore, String) {
    let mut root = std::env::temp_dir();
    root.push(format!(
        "bridgevm-api-{test}-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let store = VmStore::new(root);
    let name = "fast-cold".to_string();
    let manifest = VmManifest::new(
        &name,
        VmMode::Fast,
        Guest {
            os: "ubuntu".to_string(),
            version: None,
            arch: "arm64".to_string(),
        },
        "80GiB",
    );
    handle_request(&store, BridgeVmRequest::create_vm(manifest))
        .into_result()
        .unwrap();
    (store, name)
}

pub(crate) fn serve_one_http_response(body: &'static [u8]) -> (String, JoinHandle<()>) {
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

pub(crate) fn assert_measurement(
    measurements: &[PerformanceMeasurementRecord],
    name: &str,
    value: u64,
    unit: &str,
) {
    let measurement = measurements
        .iter()
        .find(|measurement| measurement.name == name)
        .unwrap_or_else(|| panic!("missing performance measurement {name}"));
    assert_eq!(measurement.value, value);
    assert_eq!(measurement.unit, unit);
    assert!(measurement.metadata_only);
}

pub(crate) fn assert_non_metadata_measurement(
    measurements: &[PerformanceMeasurementRecord],
    name: &str,
    value: u64,
    unit: &str,
) {
    let measurement = measurements
        .iter()
        .find(|measurement| measurement.name == name)
        .unwrap_or_else(|| panic!("missing performance measurement {name}"));
    assert_eq!(measurement.value, value);
    assert_eq!(measurement.unit, unit);
    assert!(!measurement.metadata_only);
}

pub(crate) fn assert_non_metadata_measurement_exists(
    measurements: &[PerformanceMeasurementRecord],
    name: &str,
    unit: &str,
) {
    let measurement = measurements
        .iter()
        .find(|measurement| measurement.name == name)
        .unwrap_or_else(|| panic!("missing performance measurement {name}"));
    assert_eq!(measurement.unit, unit);
    assert!(!measurement.metadata_only);
}

pub(crate) fn stage_ready_fast_linux_kernel_vm(store: &VmStore, name: &str) -> PathBuf {
    let (bundle, mut manifest) = store.get_vm(name).unwrap();
    std::fs::create_dir_all(bundle.join("disks")).unwrap();
    std::fs::create_dir_all(bundle.join("boot")).unwrap();
    std::fs::write(bundle.join("disks/root.raw"), b"raw disk placeholder").unwrap();
    std::fs::write(bundle.join("boot/vmlinuz"), b"kernel placeholder").unwrap();

    manifest.storage.primary.path = "disks/root.raw".to_string();
    manifest.storage.primary.format = "raw".to_string();
    manifest.boot = Some(bridgevm_config::Boot {
        mode: BootMode::LinuxKernel,
        installer_image: None,
        kernel_path: Some("boot/vmlinuz".to_string()),
        initrd_path: None,
        kernel_command_line: Some("console=hvc0 root=/dev/vda".to_string()),
        macos_restore_image: None,
    });
    manifest.write(&bundle.join("manifest.yaml")).unwrap();
    bundle
}

pub(crate) fn write_executable(path: &Path, contents: &str) {
    use std::os::unix::fs::PermissionsExt;

    std::fs::write(path, contents).unwrap();
    let mut permissions = std::fs::metadata(path).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(path, permissions).unwrap();
}

pub(crate) fn process_group_id(pid: u32) -> Option<u32> {
    let output = Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "pgid="])
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout).trim().parse().ok()
}

pub(crate) fn unique_test_root(label: &str) -> PathBuf {
    let mut root = std::env::temp_dir();
    root.push(format!(
        "bridgevm-api-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    root
}

pub(crate) fn spawn_detached_sleep() -> u32 {
    let pid_file = unique_test_root("detached-sleep-pid");
    // Shell double-fork: the outer `sh` exits immediately, the backgrounded
    // subshell's `sleep` gets reparented to init, and we record its pid.
    let script = format!(
        "( sleep 300 </dev/null >/dev/null 2>&1 & printf '%d' \"$!\" > '{}' ) &",
        pid_file.display()
    );
    let status = Command::new("sh")
        .arg("-c")
        .arg(&script)
        .status()
        .expect("failed to spawn detached sleep");
    assert!(status.success());

    // The pid file is written by the backgrounded subshell; poll for it.
    let mut pid = None;
    for _ in 0..200 {
        if let Ok(contents) = std::fs::read_to_string(&pid_file) {
            if let Ok(parsed) = contents.trim().parse::<u32>() {
                if parsed != 0 {
                    pid = Some(parsed);
                    break;
                }
            }
        }
        thread::sleep(Duration::from_millis(10));
    }
    let _ = std::fs::remove_file(&pid_file);
    let pid = pid.expect("detached sleep pid was not recorded");
    // Wait until the detached process is actually alive before returning.
    for _ in 0..200 {
        if process_is_alive(pid) {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    pid
}

pub(crate) fn valid_guest_hello(token: &str, capabilities: &[&str]) -> AgentMessage {
    AgentMessage::GuestHello {
        version: PROTOCOL_VERSION,
        guest_os: "linux".to_string(),
        agent_version: Some("1.0.0".to_string()),
        capabilities: capabilities
            .iter()
            .map(|name| AgentCapability {
                name: (*name).to_string(),
                version: 1,
            })
            .collect(),
        auth: Some(AgentAuth::ToolsToken {
            token: token.to_string(),
        }),
    }
}
