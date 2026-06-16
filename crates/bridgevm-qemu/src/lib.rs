use bridgevm_config::{BootMode, VmManifest, VmMode};
use bridgevm_network::{
    plan_network, NetworkBackend, NetworkMode, NetworkPlan, NetworkPlanError, PortForwardRule,
};
use bridgevm_resource_manager::{decide_from_manifest_profile, resolve_memory, resolve_vcpu};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    io::{BufRead, BufReader, ErrorKind, Write},
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    str::FromStr,
    time::Duration,
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum QemuError {
    #[error("QEMU command builder only supports Compatibility Mode manifests, got {0}")]
    UnsupportedMode(VmMode),
    #[error("QEMU launch does not support {0} networking yet")]
    UnsupportedNetworkMode(String),
    #[error(
        "QEMU launch blocker {blocker}: {mode} networking is not implemented for Compatibility Mode QEMU args yet; requirement: {requirement}"
    )]
    UnsupportedNetworkRequirement {
        mode: String,
        blocker: String,
        requirement: String,
    },
    #[error("QMP I/O error: {0}")]
    QmpIo(#[from] std::io::Error),
    #[error("QMP JSON error: {0}")]
    QmpJson(#[from] serde_json::Error),
    #[error("QMP response did not include a return value: {0}")]
    QmpProtocol(String),
    #[error("QEMU network plan rejected: {0}")]
    NetworkPlan(#[from] NetworkPlanError),
    #[error("windows-installer boot mode requires boot.installerImage")]
    MissingInstallerImage,
}

impl QemuError {
    pub fn is_qmp_idle(&self) -> bool {
        matches!(
            self,
            QemuError::QmpIo(error)
                if matches!(
                    error.kind(),
                    ErrorKind::WouldBlock | ErrorKind::TimedOut | ErrorKind::UnexpectedEof
                )
        )
    }
}

pub fn is_qmp_status_unavailable(error: &QemuError) -> bool {
    matches!(
        error,
        QemuError::QmpIo(error)
            if matches!(
                error.kind(),
                ErrorKind::NotFound
                    | ErrorKind::ConnectionRefused
                    | ErrorKind::ConnectionReset
                    | ErrorKind::WouldBlock
                    | ErrorKind::TimedOut
                    | ErrorKind::UnexpectedEof
            )
    )
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QemuProfile {
    pub accelerator: String,
    pub machine: String,
    pub display: String,
    pub extra_args: Vec<String>,
}

impl QemuProfile {
    pub fn restricted_windows_arm() -> Self {
        Self {
            accelerator: "hvf".to_string(),
            machine: "virt".to_string(),
            display: "metal-adapter-preferred".to_string(),
            extra_args: vec!["-device".to_string(), "virtio-rng-pci".to_string()],
        }
    }

    pub fn compatibility_default() -> Self {
        Self {
            accelerator: "hvf-or-tcg".to_string(),
            machine: "auto".to_string(),
            display: "spice-or-vnc".to_string(),
            extra_args: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QemuCommand {
    pub program: String,
    pub args: Vec<String>,
}

impl QemuCommand {
    pub fn render_shell_words(&self) -> Vec<String> {
        std::iter::once(self.program.clone())
            .chain(self.args.iter().cloned())
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QemuImgCommand {
    pub program: String,
    pub args: Vec<String>,
}

impl QemuImgCommand {
    pub fn create_disk(path: &Path, format: impl Into<String>, size: impl Into<String>) -> Self {
        Self {
            program: "qemu-img".to_string(),
            args: vec![
                "create".to_string(),
                "-f".to_string(),
                format.into(),
                path.display().to_string(),
                size.into(),
            ],
        }
    }

    pub fn create_backed_disk(
        path: &Path,
        format: impl Into<String>,
        backing_format: impl Into<String>,
        backing_file: &Path,
    ) -> Self {
        Self {
            program: "qemu-img".to_string(),
            args: vec![
                "create".to_string(),
                "-f".to_string(),
                format.into(),
                "-F".to_string(),
                backing_format.into(),
                "-b".to_string(),
                backing_file.display().to_string(),
                path.display().to_string(),
            ],
        }
    }

    pub fn info_json(path: &Path) -> Self {
        Self {
            program: "qemu-img".to_string(),
            args: vec![
                "info".to_string(),
                "--output=json".to_string(),
                path.display().to_string(),
            ],
        }
    }

    pub fn check_json(path: &Path) -> Self {
        Self {
            program: "qemu-img".to_string(),
            args: vec![
                "check".to_string(),
                "--output=json".to_string(),
                path.display().to_string(),
            ],
        }
    }

    pub fn convert_compact(source: &Path, output: &Path, format: impl Into<String>) -> Self {
        Self {
            program: "qemu-img".to_string(),
            args: vec![
                "convert".to_string(),
                "-O".to_string(),
                format.into(),
                source.display().to_string(),
                output.display().to_string(),
            ],
        }
    }

    pub fn render_shell_words(&self) -> Vec<String> {
        std::iter::once(self.program.clone())
            .chain(self.args.iter().cloned())
            .collect()
    }
}

pub fn build_compatibility_command(
    manifest: &VmManifest,
    bundle_path: &Path,
) -> Result<QemuCommand, QemuError> {
    if manifest.mode != VmMode::Compatibility {
        return Err(QemuError::UnsupportedMode(manifest.mode));
    }

    let arch = manifest.guest.arch.to_ascii_lowercase();
    let program = match arch.as_str() {
        "x86_64" | "amd64" => "qemu-system-x86_64",
        "arm64" | "aarch64" => "qemu-system-aarch64",
        "riscv64" => "qemu-system-riscv64",
        other => {
            if other.contains("i386") || other.contains("x86") {
                "qemu-system-i386"
            } else {
                "qemu-system-x86_64"
            }
        }
    }
    .to_string();

    let disk_path = resolve_bundle_path(bundle_path, &manifest.storage.primary.path);
    let qmp_path = qmp_socket_path(bundle_path);
    let guest_tools_path = guest_tools_socket_path(bundle_path);
    let serial_log = bundle_path.join("logs").join("serial.log");
    let resource_decision = decide_from_manifest_profile(&manifest.resources.profile);
    let memory = resolve_memory(&manifest.resources.memory, &resource_decision);
    let cpu = resolve_vcpu(&manifest.resources.cpu, &resource_decision);
    let profile = qemu_profile_for_manifest(manifest);
    let machine = if profile.machine == "auto" {
        machine_for_arch(&arch).to_string()
    } else {
        profile.machine.clone()
    };
    let accelerator = manifest
        .backend
        .accelerator
        .clone()
        .unwrap_or_else(|| accelerator_arg(&profile).to_string());
    let display_renderer = if is_windows_11_arm(manifest)
        && matches!(manifest.display.renderer.as_str(), "spice" | "spice-or-vnc")
    {
        profile.display.as_str()
    } else {
        manifest.display.renderer.as_str()
    };

    let mut args = vec![
        "-name".to_string(),
        manifest.name.clone(),
        "-machine".to_string(),
        machine,
        "-accel".to_string(),
        accelerator,
        "-m".to_string(),
        memory_arg(&memory),
        "-smp".to_string(),
        cpu_arg(&cpu),
        "-drive".to_string(),
        format!(
            "file={},if=virtio,format={},discard={}",
            disk_path.display(),
            manifest.storage.primary.format,
            if manifest.storage.primary.discard {
                "unmap"
            } else {
                "ignore"
            }
        ),
        "-netdev".to_string(),
        netdev_arg(manifest)?,
        "-device".to_string(),
        "virtio-net-pci,netdev=net0".to_string(),
        "-display".to_string(),
        display_arg(display_renderer).to_string(),
        "-qmp".to_string(),
        format!("unix:{},server=on,wait=off", qmp_path.display()),
        "-chardev".to_string(),
        format!(
            "socket,id=bridgevm-tools,path={},server=on,wait=off",
            guest_tools_path.display()
        ),
        "-device".to_string(),
        "virtio-serial-pci".to_string(),
        "-device".to_string(),
        "virtserialport,chardev=bridgevm-tools,name=org.bridgevm.guest-tools.0".to_string(),
        "-serial".to_string(),
        format!("file:{}", serial_log.display()),
    ];

    if arch == "arm64" || arch == "aarch64" {
        args.extend([
            "-cpu".to_string(),
            "host".to_string(),
            "-bios".to_string(),
            "edk2-aarch64-code.fd".to_string(),
        ]);
    }

    if (arch == "arm64" || arch == "aarch64")
        && manifest
            .boot
            .as_ref()
            .is_some_and(|boot| boot.mode == BootMode::WindowsInstaller)
    {
        let boot = manifest.boot.as_ref().expect("windows-installer boot present");
        let installer = boot
            .installer_image
            .as_deref()
            .ok_or(QemuError::MissingInstallerImage)?;
        let installer_path = resolve_bundle_path(bundle_path, installer);
        // Verified device shape for booting the Windows 11 Arm installer ISO under
        // QEMU virt + edk2: a GOP framebuffer (ramfb) the installer can render to, a
        // USB HID stack so the "Press any key to boot" prompt can be answered, and the
        // ISO presented as a bootable USB CD-ROM (WinPE includes USB mass-storage
        // drivers). The primary disk stays attached as the install target.
        args.extend([
            "-device".to_string(),
            "ramfb".to_string(),
            "-device".to_string(),
            "qemu-xhci,id=usb".to_string(),
            "-device".to_string(),
            "usb-kbd,bus=usb.0".to_string(),
            "-device".to_string(),
            "usb-tablet,bus=usb.0".to_string(),
            "-drive".to_string(),
            format!(
                "if=none,id=installer,file={},media=cdrom,readonly=on",
                installer_path.display()
            ),
            "-device".to_string(),
            "usb-storage,bus=usb.0,drive=installer,bootindex=0".to_string(),
        ]);
    }

    args.extend(profile.extra_args);

    Ok(QemuCommand { program, args })
}

pub fn qmp_socket_path(bundle_path: &Path) -> PathBuf {
    bundle_path.join("metadata").join("qmp.sock")
}

pub fn guest_tools_socket_path(bundle_path: &Path) -> PathBuf {
    bundle_path.join("metadata").join("guest-tools.sock")
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QmpCommand {
    pub execute: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arguments: Option<Value>,
}

impl QmpCommand {
    pub fn capabilities() -> Self {
        Self {
            execute: "qmp_capabilities".to_string(),
            arguments: None,
        }
    }

    pub fn query_status() -> Self {
        Self {
            execute: "query-status".to_string(),
            arguments: None,
        }
    }

    pub fn stop() -> Self {
        Self {
            execute: "stop".to_string(),
            arguments: None,
        }
    }

    pub fn cont() -> Self {
        Self {
            execute: "cont".to_string(),
            arguments: None,
        }
    }

    pub fn quit() -> Self {
        Self {
            execute: "quit".to_string(),
            arguments: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QmpEnvelope {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub greeting: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event: Option<QmpEvent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QmpEvent {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl QmpEvent {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self.name.as_str(),
            "SHUTDOWN" | "RESET" | "STOP" | "GUEST_PANICKED" | "WATCHDOG"
        )
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QmpEventDrain {
    pub events: Vec<QmpEvent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal_event: Option<QmpEvent>,
    pub envelopes_read: usize,
    pub limit_reached: bool,
}

impl QmpEventDrain {
    fn empty() -> Self {
        Self {
            events: Vec::new(),
            terminal_event: None,
            envelopes_read: 0,
            limit_reached: false,
        }
    }

    pub fn has_terminal_event(&self) -> bool {
        self.terminal_event.is_some()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QmpStatus {
    pub status: String,
    pub running: bool,
}

impl QmpStatus {
    pub fn is_terminal(&self) -> bool {
        !self.running
            && matches!(
                self.status.as_str(),
                "shutdown" | "internal-error" | "guest-panicked"
            )
    }
}

pub fn query_status(socket_path: &Path) -> Result<QmpStatus, QemuError> {
    let mut client = QmpClient::connect(socket_path)?;
    client.negotiate()?;
    let value = client.execute(QmpCommand::query_status())?;
    Ok(QmpStatus {
        status: value
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string(),
        running: value
            .get("running")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

pub fn quit(socket_path: &Path) -> Result<(), QemuError> {
    let mut client = QmpClient::connect(socket_path)?;
    client.negotiate()?;
    let _ = client.execute(QmpCommand::quit())?;
    Ok(())
}

pub fn stop(socket_path: &Path) -> Result<(), QemuError> {
    let mut client = QmpClient::connect(socket_path)?;
    client.negotiate()?;
    let _ = client.execute(QmpCommand::stop())?;
    Ok(())
}

pub fn cont(socket_path: &Path) -> Result<(), QemuError> {
    let mut client = QmpClient::connect(socket_path)?;
    client.negotiate()?;
    let _ = client.execute(QmpCommand::cont())?;
    Ok(())
}

pub struct QmpClient {
    reader: BufReader<UnixStream>,
    writer: UnixStream,
}

impl QmpClient {
    pub fn connect(socket_path: &Path) -> Result<Self, QemuError> {
        Self::connect_with_timeout(socket_path, Duration::from_secs(1))
    }

    pub fn connect_with_timeout(socket_path: &Path, timeout: Duration) -> Result<Self, QemuError> {
        let stream = UnixStream::connect(socket_path)?;
        stream.set_read_timeout(Some(timeout))?;
        stream.set_write_timeout(Some(timeout))?;
        let writer = stream.try_clone()?;
        Ok(Self {
            reader: BufReader::new(stream),
            writer,
        })
    }

    pub fn negotiate(&mut self) -> Result<(), QemuError> {
        let _ = self.read_envelope()?;
        let _ = self.execute(QmpCommand::capabilities())?;
        Ok(())
    }

    pub fn execute(&mut self, command: QmpCommand) -> Result<Value, QemuError> {
        serde_json::to_writer(&mut self.writer, &command)?;
        self.writer.write_all(b"\n")?;
        loop {
            let envelope = self.read_envelope()?;
            if envelope.event.is_some() {
                continue;
            }
            if let Some(error) = envelope.error {
                return Err(QemuError::QmpProtocol(error.to_string()));
            }
            return envelope
                .result
                .ok_or_else(|| QemuError::QmpProtocol("missing return".to_string()));
        }
    }

    pub fn read_event(&mut self) -> Result<QmpEvent, QemuError> {
        loop {
            let envelope = self.read_envelope()?;
            if let Some(event) = envelope.event {
                return Ok(event);
            }
        }
    }

    pub fn drain_events(&mut self, max_envelopes: usize) -> Result<QmpEventDrain, QemuError> {
        let mut drain = QmpEventDrain::empty();

        for _ in 0..max_envelopes {
            match self.read_envelope() {
                Ok(envelope) => {
                    drain.envelopes_read += 1;
                    if let Some(event) = envelope.event {
                        if event.is_terminal() {
                            drain.terminal_event = Some(event.clone());
                        }
                        drain.events.push(event);

                        if drain.terminal_event.is_some() {
                            return Ok(drain);
                        }
                    }
                }
                Err(error) if error.is_qmp_idle() => return Ok(drain),
                Err(error) => return Err(error),
            }
        }

        drain.limit_reached = max_envelopes > 0;
        Ok(drain)
    }

    pub fn read_envelope(&mut self) -> Result<QmpEnvelope, QemuError> {
        let mut line = String::new();
        if self.reader.read_line(&mut line)? == 0 {
            return Err(QemuError::QmpIo(std::io::Error::new(
                ErrorKind::UnexpectedEof,
                "QMP stream closed",
            )));
        }
        let value = serde_json::from_str::<Value>(&line)?;
        Ok(QmpEnvelope {
            greeting: value.get("QMP").cloned(),
            event: value
                .get("event")
                .and_then(Value::as_str)
                .map(|name| QmpEvent {
                    name: name.to_string(),
                    data: value.get("data").cloned(),
                }),
            result: value.get("return").cloned(),
            error: value.get("error").cloned(),
        })
    }
}

fn resolve_bundle_path(bundle_path: &Path, relative_or_absolute: &str) -> PathBuf {
    let path = PathBuf::from(relative_or_absolute);
    if path.is_absolute() {
        path
    } else {
        bundle_path.join(path)
    }
}

fn machine_for_arch(arch: &str) -> &'static str {
    match arch {
        "arm64" | "aarch64" | "riscv64" => "virt",
        _ => "q35",
    }
}

fn qemu_profile_for_manifest(manifest: &VmManifest) -> QemuProfile {
    if is_windows_11_arm(manifest) {
        QemuProfile::restricted_windows_arm()
    } else {
        QemuProfile::compatibility_default()
    }
}

fn is_windows_11_arm(manifest: &VmManifest) -> bool {
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

fn accelerator_arg(profile: &QemuProfile) -> &str {
    match profile.accelerator.as_str() {
        "hvf-or-tcg" => "hvf",
        accelerator => accelerator,
    }
}

fn memory_arg(value: &str) -> String {
    if value == "auto" {
        "4096".to_string()
    } else if value.ends_with("GiB") {
        value
            .trim_end_matches("GiB")
            .parse::<u64>()
            .map(|gib| (gib * 1024).to_string())
            .unwrap_or_else(|_| value.to_string())
    } else if value.ends_with("MiB") {
        value.trim_end_matches("MiB").to_string()
    } else {
        value.to_string()
    }
}

fn cpu_arg(value: &str) -> String {
    if value == "auto" {
        "2".to_string()
    } else {
        value.to_string()
    }
}

fn display_arg(renderer: &str) -> &'static str {
    match renderer {
        "spice" => "default,show-cursor=on",
        "spice-or-vnc" => "vnc=:0",
        "vnc" => "vnc=:0",
        "metal-adapter-preferred" => "cocoa,gl=on",
        _ => "default,show-cursor=on",
    }
}

fn netdev_arg(manifest: &VmManifest) -> Result<String, QemuError> {
    let plan = qemu_network_plan(manifest)?;
    let mut arg = match plan.mode {
        NetworkMode::Nat => "user,id=net0".to_string(),
        NetworkMode::HostOnly => "vmnet-host,id=net0".to_string(),
        NetworkMode::Isolated => "user,id=net0,restrict=on".to_string(),
        NetworkMode::Bridged | NetworkMode::Advanced => {
            let requirement = plan.requirements.first().cloned().unwrap_or_else(|| {
                bridgevm_network::NetworkRequirement {
                    blocker: format!("qemu-{}-network-unimplemented", plan.mode),
                    requirement: format!(
                        "Compatibility Mode QEMU requires {} network launcher wiring before launch",
                        plan.mode
                    ),
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

fn qemu_network_plan(manifest: &VmManifest) -> Result<NetworkPlan, QemuError> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use bridgevm_config::{Boot, BootMode, Guest, PortForward, VmManifest, VmMode};
    use serde_json::json;
    use std::{
        fs,
        io::{BufRead, BufReader, Write},
        os::unix::net::UnixListener,
        sync::atomic::{AtomicU64, Ordering},
        thread,
        time::{SystemTime, UNIX_EPOCH},
    };

    static TEMP_SOCKET_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn arg_after<'a>(args: &'a [String], flag: &str) -> &'a str {
        args.windows(2)
            .find_map(|pair| (pair[0] == flag).then_some(pair[1].as_str()))
            .unwrap_or_else(|| panic!("missing {flag} argument"))
    }

    #[test]
    fn builds_compatibility_qemu_args() {
        let manifest = VmManifest::new(
            "legacy",
            VmMode::Compatibility,
            Guest {
                os: "ubuntu".to_string(),
                version: None,
                arch: "x86_64".to_string(),
            },
            "64GiB",
        );
        let command = build_compatibility_command(&manifest, Path::new("/tmp/legacy.vmbridge"))
            .expect("compat command");

        assert_eq!(command.program, "qemu-system-x86_64");
        assert!(command.args.contains(&"-qmp".to_string()));
        assert!(command.args.contains(&"-chardev".to_string()));
        assert!(command.args.iter().any(|arg| arg
            == "socket,id=bridgevm-tools,path=/tmp/legacy.vmbridge/metadata/guest-tools.sock,server=on,wait=off"));
        assert!(command.args.contains(&"-device".to_string()));
        assert!(command.args.iter().any(|arg| arg == "virtio-serial-pci"));
        assert!(command
            .args
            .iter()
            .any(|arg| arg
                == "virtserialport,chardev=bridgevm-tools,name=org.bridgevm.guest-tools.0"));
        assert!(command
            .args
            .iter()
            .any(|arg| arg.contains("/tmp/legacy.vmbridge/disks/root.qcow2")));
        assert_eq!(
            arg_after(&command.args, "-serial"),
            "file:/tmp/legacy.vmbridge/logs/serial.log"
        );
        assert_eq!(arg_after(&command.args, "-display"), "vnc=:0");
        assert!(!command
            .args
            .join(" ")
            .contains("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"));
    }

    #[test]
    fn windows_11_arm_uses_restricted_planning_profile() {
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
        manifest.backend.accelerator = None;

        let command = build_compatibility_command(&manifest, Path::new("/tmp/win11.vmbridge"))
            .expect("windows arm compat command");

        assert_eq!(command.program, "qemu-system-aarch64");
        assert_eq!(arg_after(&command.args, "-machine"), "virt");
        assert_eq!(arg_after(&command.args, "-accel"), "hvf");
        assert_eq!(arg_after(&command.args, "-display"), "cocoa,gl=on");
        assert!(command
            .args
            .windows(2)
            .any(|pair| pair[0] == "-device" && pair[1] == "virtio-rng-pci"));
        assert_eq!(arg_after(&command.args, "-cpu"), "host");
        assert_eq!(arg_after(&command.args, "-bios"), "edk2-aarch64-code.fd");
    }

    #[test]
    fn explicit_windows_11_arm_display_renderer_is_preserved() {
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

        let command = build_compatibility_command(&manifest, Path::new("/tmp/win11.vmbridge"))
            .expect("windows arm compat command");

        assert_eq!(arg_after(&command.args, "-display"), "vnc=:0");
    }

    #[test]
    fn windows_installer_boot_mode_wires_installer_media() {
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
        manifest.boot = Some(Boot {
            mode: BootMode::WindowsInstaller,
            installer_image: Some("media/win11.iso".to_string()),
            kernel_path: None,
            initrd_path: None,
            kernel_command_line: None,
            macos_restore_image: None,
        });

        let command = build_compatibility_command(&manifest, Path::new("/tmp/win11.vmbridge"))
            .expect("windows installer compat command");

        // GOP framebuffer the installer renders to.
        assert!(command
            .args
            .windows(2)
            .any(|pair| pair[0] == "-device" && pair[1] == "ramfb"));
        // USB HID stack so the "Press any key to boot" prompt can be answered.
        assert!(command
            .args
            .windows(2)
            .any(|pair| pair[0] == "-device" && pair[1] == "usb-kbd,bus=usb.0"));
        // Installer ISO as a bootable USB CD-ROM, preferred via bootindex.
        assert!(command
            .args
            .windows(2)
            .any(|pair| pair[0] == "-device"
                && pair[1] == "usb-storage,bus=usb.0,drive=installer,bootindex=0"));
        assert!(command.args.iter().any(|arg| arg.contains(
            "if=none,id=installer,file=/tmp/win11.vmbridge/media/win11.iso,media=cdrom,readonly=on"
        )));
    }

    #[test]
    fn windows_installer_boot_mode_requires_installer_image() {
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
        manifest.boot = Some(Boot {
            mode: BootMode::WindowsInstaller,
            installer_image: None,
            kernel_path: None,
            initrd_path: None,
            kernel_command_line: None,
            macos_restore_image: None,
        });

        let error = build_compatibility_command(&manifest, Path::new("/tmp/win11.vmbridge"))
            .expect_err("missing installer image must fail");
        assert!(matches!(error, QemuError::MissingInstallerImage));
    }

    #[test]
    fn qemu_netdev_preserves_hostfwd_output_after_network_planning() {
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
        manifest.network.forwards.push(PortForward {
            host: 2222,
            guest: 22,
        });

        assert_eq!(
            netdev_arg(&manifest).expect("planned netdev"),
            "user,id=net0,hostfwd=tcp::2222-:22"
        );
    }

    #[test]
    fn qemu_network_planner_rejects_duplicate_host_forwards() {
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
        manifest.network.forwards.push(PortForward {
            host: 2222,
            guest: 22,
        });
        manifest.network.forwards.push(PortForward {
            host: 2222,
            guest: 8080,
        });

        let error = build_compatibility_command(&manifest, Path::new("/tmp/legacy.vmbridge"))
            .expect_err("duplicate host ports must be rejected by the network planner");

        assert!(matches!(
            error,
            QemuError::NetworkPlan(NetworkPlanError::DuplicateHostPort { host: 2222 })
        ));
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

    #[test]
    fn qemu_netdev_maps_host_only_mode_from_network_plan() {
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

        assert_eq!(
            netdev_arg(&manifest).expect("planned host-only netdev"),
            "vmnet-host,id=net0"
        );
    }

    #[test]
    fn qemu_netdev_reports_bridged_launch_requirement_after_planning() {
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
        manifest.network.mode = "bridged".to_string();

        let error = build_compatibility_command(&manifest, Path::new("/tmp/legacy.vmbridge"))
            .expect_err("bridged QEMU launcher wiring is not implemented yet");

        assert!(
            matches!(
                &error,
                QemuError::UnsupportedNetworkRequirement {
                    mode,
                    blocker,
                    requirement
                } if mode == "bridged"
                    && blocker == "qemu-bridged-network-unimplemented"
                    && requirement.contains("bridge or tap helper")
            ),
            "{error}"
        );
    }

    #[test]
    fn qemu_netdev_reports_advanced_launch_requirement_after_planning() {
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
        manifest.network.mode = "advanced".to_string();

        let error = build_compatibility_command(&manifest, Path::new("/tmp/legacy.vmbridge"))
            .expect_err("advanced QEMU launcher wiring is not implemented yet");

        assert!(
            matches!(
                &error,
                QemuError::UnsupportedNetworkRequirement {
                    mode,
                    blocker,
                    requirement
                } if mode == "advanced"
                    && blocker == "qemu-advanced-network-unimplemented"
                    && requirement.contains("advanced network schema")
            ),
            "{error}"
        );
    }

    #[test]
    fn qemu_network_planner_rejects_unknown_mode_names() {
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
        manifest.network.mode = "slirp".to_string();

        let error = build_compatibility_command(&manifest, Path::new("/tmp/legacy.vmbridge"))
            .expect_err("unknown modes must be rejected by the network planner");

        assert!(matches!(
            error,
            QemuError::NetworkPlan(NetworkPlanError::UnsupportedModeName(mode)) if mode == "slirp"
        ));
    }

    #[test]
    fn resource_profile_applies_to_auto_values_and_preserves_manual_overrides() {
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
        manifest.resources.profile = "performance".to_string();

        let command = build_compatibility_command(&manifest, Path::new("/tmp/perf.vmbridge"))
            .expect("compat command");
        assert_eq!(arg_after(&command.args, "-m"), "6144");
        assert_eq!(arg_after(&command.args, "-smp"), "4");

        manifest.resources.memory = "8192".to_string();
        manifest.resources.cpu = "6".to_string();
        let command = build_compatibility_command(&manifest, Path::new("/tmp/manual.vmbridge"))
            .expect("compat command");
        assert_eq!(arg_after(&command.args, "-m"), "8192");
        assert_eq!(arg_after(&command.args, "-smp"), "6");
    }

    #[test]
    fn builds_qemu_img_create_disk_command() {
        let command = QemuImgCommand::create_disk(Path::new("/tmp/root.qcow2"), "qcow2", "80GiB");

        assert_eq!(command.program, "qemu-img");
        assert_eq!(
            command.args,
            ["create", "-f", "qcow2", "/tmp/root.qcow2", "80GiB"]
        );
        assert_eq!(
            command.render_shell_words(),
            [
                "qemu-img",
                "create",
                "-f",
                "qcow2",
                "/tmp/root.qcow2",
                "80GiB"
            ]
        );
    }

    #[test]
    fn builds_qemu_img_info_json_command() {
        let command = QemuImgCommand::info_json(Path::new("/tmp/root.qcow2"));

        assert_eq!(command.program, "qemu-img");
        assert_eq!(command.args, ["info", "--output=json", "/tmp/root.qcow2"]);
        assert_eq!(
            command.render_shell_words(),
            ["qemu-img", "info", "--output=json", "/tmp/root.qcow2"]
        );
    }

    #[test]
    fn builds_qemu_img_check_json_command() {
        let command = QemuImgCommand::check_json(Path::new("/tmp/root.qcow2"));

        assert_eq!(command.program, "qemu-img");
        assert_eq!(command.args, ["check", "--output=json", "/tmp/root.qcow2"]);
        assert_eq!(
            command.render_shell_words(),
            ["qemu-img", "check", "--output=json", "/tmp/root.qcow2"]
        );
    }

    #[test]
    fn builds_qemu_img_compact_convert_command() {
        let command = QemuImgCommand::convert_compact(
            Path::new("/tmp/root.qcow2"),
            Path::new("/tmp/root.qcow2.compact.tmp"),
            "qcow2",
        );

        assert_eq!(command.program, "qemu-img");
        assert_eq!(
            command.args,
            [
                "convert",
                "-O",
                "qcow2",
                "/tmp/root.qcow2",
                "/tmp/root.qcow2.compact.tmp"
            ]
        );
        assert_eq!(
            command.render_shell_words(),
            [
                "qemu-img",
                "convert",
                "-O",
                "qcow2",
                "/tmp/root.qcow2",
                "/tmp/root.qcow2.compact.tmp"
            ]
        );
    }

    #[test]
    fn builds_qemu_img_backed_disk_command() {
        let command = QemuImgCommand::create_backed_disk(
            Path::new("/tmp/snap.qcow2"),
            "qcow2",
            "qcow2",
            Path::new("/tmp/root.qcow2"),
        );

        assert_eq!(command.program, "qemu-img");
        assert_eq!(
            command.args,
            [
                "create",
                "-f",
                "qcow2",
                "-F",
                "qcow2",
                "-b",
                "/tmp/root.qcow2",
                "/tmp/snap.qcow2"
            ]
        );
    }

    #[test]
    fn rejects_fast_mode_manifest() {
        let manifest = VmManifest::new(
            "fast",
            VmMode::Fast,
            Guest {
                os: "ubuntu".to_string(),
                version: None,
                arch: "arm64".to_string(),
            },
            "80GiB",
        );
        assert!(build_compatibility_command(&manifest, Path::new("/tmp/fast.vmbridge")).is_err());
    }

    #[test]
    fn serializes_qmp_commands() {
        assert_eq!(
            serde_json::to_value(QmpCommand::query_status()).unwrap(),
            json!({ "execute": "query-status" })
        );
        assert_eq!(
            serde_json::to_value(QmpCommand::stop()).unwrap(),
            json!({ "execute": "stop" })
        );
        assert_eq!(
            serde_json::to_value(QmpCommand::cont()).unwrap(),
            json!({ "execute": "cont" })
        );
        assert_eq!(
            serde_json::to_value(QmpCommand::quit()).unwrap(),
            json!({ "execute": "quit" })
        );
    }

    #[test]
    fn exposes_qmp_socket_path() {
        assert_eq!(
            qmp_socket_path(Path::new("/tmp/example.vmbridge")),
            PathBuf::from("/tmp/example.vmbridge/metadata/qmp.sock")
        );
    }

    #[test]
    fn exposes_guest_tools_socket_path() {
        assert_eq!(
            guest_tools_socket_path(Path::new("/tmp/example.vmbridge")),
            PathBuf::from("/tmp/example.vmbridge/metadata/guest-tools.sock")
        );
    }

    #[test]
    fn qmp_status_ignores_async_events_before_command_return() {
        let socket_path = temp_socket_path();
        let listener = UnixListener::bind(&socket_path).unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .write_all(br#"{"QMP":{"version":{"qemu":{"major":8,"minor":2,"micro":0}}}}"#)
                .unwrap();
            stream.write_all(b"\n").unwrap();

            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            assert!(line.contains("qmp_capabilities"));
            stream.write_all(br#"{"return":{}}"#).unwrap();
            stream.write_all(b"\n").unwrap();

            line.clear();
            reader.read_line(&mut line).unwrap();
            assert!(line.contains("query-status"));
            stream
                .write_all(br#"{"event":"SHUTDOWN","data":{"guest":true}}"#)
                .unwrap();
            stream.write_all(b"\n").unwrap();
            stream
                .write_all(br#"{"return":{"status":"shutdown","running":false}}"#)
                .unwrap();
            stream.write_all(b"\n").unwrap();
        });

        let status = query_status(&socket_path).unwrap();
        assert_eq!(status.status, "shutdown");
        assert!(!status.running);
        assert!(status.is_terminal());

        server.join().unwrap();
        fs::remove_file(socket_path).unwrap();
    }

    #[test]
    fn qmp_stop_and_cont_round_trip_over_fake_socket() {
        for (command_name, execute) in [
            (
                "stop",
                stop as fn(&Path) -> std::result::Result<(), QemuError>,
            ),
            (
                "cont",
                cont as fn(&Path) -> std::result::Result<(), QemuError>,
            ),
        ] {
            let socket_path = temp_socket_path();
            let listener = UnixListener::bind(&socket_path).unwrap();
            let server = thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .write_all(br#"{"QMP":{"version":{"qemu":{"major":8,"minor":2,"micro":0}}}}"#)
                    .unwrap();
                stream.write_all(b"\n").unwrap();

                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                assert!(line.contains("qmp_capabilities"));
                stream.write_all(br#"{"return":{}}"#).unwrap();
                stream.write_all(b"\n").unwrap();

                line.clear();
                reader.read_line(&mut line).unwrap();
                assert!(line.contains(command_name));
                stream.write_all(br#"{"return":{}}"#).unwrap();
                stream.write_all(b"\n").unwrap();
            });

            execute(&socket_path).unwrap();

            server.join().unwrap();
            fs::remove_file(socket_path).unwrap();
        }
    }

    #[test]
    fn qmp_client_can_read_terminal_event() {
        let socket_path = temp_socket_path();
        let listener = UnixListener::bind(&socket_path).unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .write_all(br#"{"QMP":{"version":{"qemu":{"major":8,"minor":2,"micro":0}}}}"#)
                .unwrap();
            stream.write_all(b"\n").unwrap();
            stream
                .write_all(br#"{"event":"SHUTDOWN","data":{"guest":true}}"#)
                .unwrap();
            stream.write_all(b"\n").unwrap();
        });

        let mut client = QmpClient::connect(&socket_path).unwrap();
        let event = client.read_event().unwrap();

        assert_eq!(event.name, "SHUTDOWN");
        assert_eq!(event.data.as_ref().unwrap(), &json!({ "guest": true }));
        assert!(event.is_terminal());

        server.join().unwrap();
        fs::remove_file(socket_path).unwrap();
    }

    #[test]
    fn qmp_client_drains_available_events_until_terminal() {
        let socket_path = temp_socket_path();
        let listener = UnixListener::bind(&socket_path).unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .write_all(br#"{"QMP":{"version":{"qemu":{"major":8,"minor":2,"micro":0}}}}"#)
                .unwrap();
            stream.write_all(b"\n").unwrap();

            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            assert!(line.contains("qmp_capabilities"));
            stream.write_all(br#"{"return":{}}"#).unwrap();
            stream.write_all(b"\n").unwrap();

            stream.write_all(br#"{"event":"RESUME"}"#).unwrap();
            stream.write_all(b"\n").unwrap();
            stream
                .write_all(br#"{"event":"SHUTDOWN","data":{"guest":true}}"#)
                .unwrap();
            stream.write_all(b"\n").unwrap();
            stream.write_all(br#"{"event":"RESUME"}"#).unwrap();
            stream.write_all(b"\n").unwrap();
        });

        let mut client =
            QmpClient::connect_with_timeout(&socket_path, Duration::from_millis(25)).unwrap();
        client.negotiate().unwrap();
        let drain = client.drain_events(8).unwrap();

        assert_eq!(drain.envelopes_read, 2);
        assert_eq!(
            drain
                .events
                .iter()
                .map(|event| event.name.as_str())
                .collect::<Vec<_>>(),
            ["RESUME", "SHUTDOWN"]
        );
        assert_eq!(
            drain
                .terminal_event
                .as_ref()
                .unwrap()
                .data
                .as_ref()
                .unwrap(),
            &json!({ "guest": true })
        );
        assert!(drain.has_terminal_event());
        assert!(!drain.limit_reached);

        server.join().unwrap();
        fs::remove_file(socket_path).unwrap();
    }

    #[test]
    fn qmp_client_drain_treats_idle_socket_as_empty() {
        let socket_path = temp_socket_path();
        let listener = UnixListener::bind(&socket_path).unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .write_all(br#"{"QMP":{"version":{"qemu":{"major":8,"minor":2,"micro":0}}}}"#)
                .unwrap();
            stream.write_all(b"\n").unwrap();

            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            assert!(line.contains("qmp_capabilities"));
            stream.write_all(br#"{"return":{}}"#).unwrap();
            stream.write_all(b"\n").unwrap();

            thread::sleep(Duration::from_millis(100));
        });

        let mut client =
            QmpClient::connect_with_timeout(&socket_path, Duration::from_millis(25)).unwrap();
        client.negotiate().unwrap();
        let drain = client.drain_events(8).unwrap();

        assert!(drain.events.is_empty());
        assert_eq!(drain.envelopes_read, 0);
        assert!(!drain.has_terminal_event());
        assert!(!drain.limit_reached);

        server.join().unwrap();
        fs::remove_file(socket_path).unwrap();
    }

    fn temp_socket_path() -> PathBuf {
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
}
