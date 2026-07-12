use bridgevm_config::{BootMode, VmManifest, VmMode};
use bridgevm_network::{
    plan_network, NetworkBackend, NetworkMode, NetworkPlan, NetworkPlanError, PortForwardRule,
};
use bridgevm_resource_manager::{decide_from_manifest_profile, resolve_memory, resolve_vcpu};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    io::{BufRead, BufReader, ErrorKind, Read, Write},
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    str::FromStr,
    time::Duration,
};
use thiserror::Error;

const MAX_QMP_ENVELOPE_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Error)]
pub enum QemuError {
    #[error("QEMU command builder only supports Compatibility Mode manifests, got {0}")]
    UnsupportedMode(VmMode),
    #[error("QEMU launch does not support {0} networking yet")]
    UnsupportedNetworkMode(String),
    #[error(
        "QEMU launch blocker {blocker}: {mode} networking requires an advanced Compatibility Mode QEMU schema before args can be generated; requirement: {requirement}"
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

/// Build the `-drive` (and, for NVMe, `-device`) args for the primary disk. The
/// install target is virtio-blk by default; `firmware.nvmeTarget` (aarch64 only)
/// instead presents it as an NVMe device, which Windows 11 Setup recognizes
/// natively. Either way the QMP block node-name is [`COMPAT_PRIMARY_BLOCK_NODE`]
/// so snapshot/suspend are unaffected.
fn primary_disk_args(manifest: &VmManifest, disk_path: &Path, is_aarch64: bool) -> Vec<String> {
    let format = &manifest.storage.primary.format;
    let discard = if manifest.storage.primary.discard {
        "unmap"
    } else {
        "ignore"
    };
    if is_aarch64 && manifest.firmware.nvme_target {
        vec![
            "-drive".to_string(),
            format!(
                "file={},if=none,format={},discard={},node-name={},id={}",
                escape_qemu_opt(disk_path.display()),
                format,
                discard,
                COMPAT_PRIMARY_BLOCK_NODE,
                COMPAT_PRIMARY_NVME_DRIVE
            ),
            "-device".to_string(),
            format!(
                "nvme,drive={},serial=bridgevm-nvme0",
                COMPAT_PRIMARY_NVME_DRIVE
            ),
        ]
    } else {
        vec![
            "-drive".to_string(),
            format!(
                "file={},if=virtio,format={},discard={},node-name={}",
                escape_qemu_opt(disk_path.display()),
                format,
                discard,
                COMPAT_PRIMARY_BLOCK_NODE
            ),
        ]
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
    let is_aarch64 = arch == "arm64" || arch == "aarch64";
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
        "-netdev".to_string(),
        netdev_arg(manifest)?,
        "-device".to_string(),
        "virtio-net-pci,netdev=net0".to_string(),
        "-display".to_string(),
        display_arg(display_renderer).to_string(),
        "-qmp".to_string(),
        format!(
            "unix:{},server=on,wait=off",
            escape_qemu_opt(qmp_path.display())
        ),
        "-chardev".to_string(),
        format!(
            "socket,id=bridgevm-tools,path={},server=on,wait=off",
            escape_qemu_opt(guest_tools_path.display())
        ),
        "-device".to_string(),
        "virtio-serial-pci".to_string(),
        "-device".to_string(),
        "virtserialport,chardev=bridgevm-tools,name=org.bridgevm.guest-tools.0".to_string(),
        "-serial".to_string(),
        format!("file:{}", escape_qemu_opt(serial_log.display())),
    ];

    // Primary disk: virtio-blk by default, or an NVMe target for Windows 11.
    args.extend(primary_disk_args(manifest, &disk_path, is_aarch64));

    if is_aarch64 {
        args.extend(["-cpu".to_string(), "host".to_string()]);
        if manifest.firmware.secure_boot {
            // Secure Boot needs a persistent edk2 variable store (pflash) holding
            // the enrolled Microsoft keys instead of the ephemeral read-only
            // `-bios`. The code blob stays read-only; the per-bundle vars file
            // must be seeded from an edk2 secure-boot template (host resource).
            let vars_path = secure_boot_vars_path(bundle_path);
            args.extend([
                "-drive".to_string(),
                "if=pflash,format=raw,unit=0,readonly=on,file=edk2-aarch64-code.fd".to_string(),
                "-drive".to_string(),
                format!(
                    "if=pflash,format=raw,unit=1,file={}",
                    escape_qemu_opt(vars_path.display())
                ),
            ]);
        } else {
            args.extend(["-bios".to_string(), "edk2-aarch64-code.fd".to_string()]);
        }
        if manifest.firmware.tpm {
            // TPM 2.0 backed by an external swtpm process on a per-bundle socket
            // (the swtpm process must be launched separately — host dependency).
            let swtpm_sock = swtpm_socket_path(bundle_path);
            args.extend([
                "-chardev".to_string(),
                format!(
                    "socket,id=chrtpm,path={}",
                    escape_qemu_opt(swtpm_sock.display())
                ),
                "-tpmdev".to_string(),
                "emulator,id=tpm0,chardev=chrtpm".to_string(),
                "-device".to_string(),
                "tpm-tis-device,tpmdev=tpm0".to_string(),
            ]);
        }
    }

    if is_aarch64
        && manifest
            .boot
            .as_ref()
            .is_some_and(|boot| boot.mode == BootMode::WindowsInstaller)
    {
        let boot = manifest
            .boot
            .as_ref()
            .expect("windows-installer boot present");
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
                escape_qemu_opt(installer_path.display())
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

/// Socket an external `swtpm` process must listen on for the emulated TPM 2.0
/// (`-tpmdev emulator,...,chardev`). Per-bundle so concurrent VMs don't collide.
pub fn swtpm_socket_path(bundle_path: &Path) -> PathBuf {
    bundle_path.join("metadata").join("swtpm.sock")
}

/// Per-bundle writable edk2 UEFI variable store used when Secure Boot is enabled
/// (the `if=pflash,unit=1` device). Must be seeded from an edk2 secure-boot vars
/// template with Microsoft keys enrolled before first boot.
pub fn secure_boot_vars_path(bundle_path: &Path) -> PathBuf {
    bundle_path.join("metadata").join("edk2-vars.fd")
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

    /// Build the job-based `snapshot-save` command that writes a full internal
    /// VM snapshot (CPU + RAM + device state) into the qcow2 disk under `tag`.
    ///
    /// `job_id` lets the caller poll completion via `query-jobs`. `devices`
    /// lists the block node names whose qcow2 receives the snapshot;
    /// `vmstate` names the device that stores the machine state (RAM/CPU).
    pub fn snapshot_save(job_id: &str, tag: &str, vmstate: &str, devices: &[String]) -> Self {
        Self {
            execute: "snapshot-save".to_string(),
            arguments: Some(serde_json::json!({
                "job-id": job_id,
                "tag": tag,
                "vmstate": vmstate,
                "devices": devices,
            })),
        }
    }

    /// Build the job-based `snapshot-load` command that restores a full
    /// internal VM snapshot previously written by [`QmpCommand::snapshot_save`].
    pub fn snapshot_load(job_id: &str, tag: &str, vmstate: &str, devices: &[String]) -> Self {
        Self {
            execute: "snapshot-load".to_string(),
            arguments: Some(serde_json::json!({
                "job-id": job_id,
                "tag": tag,
                "vmstate": vmstate,
                "devices": devices,
            })),
        }
    }

    /// Build the `query-jobs` command used to poll job-based commands such as
    /// `snapshot-save`/`snapshot-load` to completion.
    pub fn query_jobs() -> Self {
        Self {
            execute: "query-jobs".to_string(),
            arguments: None,
        }
    }
}

/// Block node name QEMU assigns to the primary virtio drive in
/// [`build_compatibility_command`] (the qcow2 that receives suspend snapshots).
pub const COMPAT_PRIMARY_BLOCK_NODE: &str = "bridgevm-root";

/// `-drive` id for the primary disk when attached as an NVMe target (Windows 11
/// full-install firmware). The QMP block node-name stays [`COMPAT_PRIMARY_BLOCK_NODE`]
/// so snapshot/suspend keep working regardless of the bus.
pub const COMPAT_PRIMARY_NVME_DRIVE: &str = "bridgevm-nvme";

/// Internal snapshot tag used for Compatibility Mode suspend/resume.
pub const COMPAT_SUSPEND_SNAPSHOT_TAG: &str = "bridgevm-suspend";

/// Terminal states for a QEMU job (`query-jobs[].status`).
fn job_status_is_terminal(status: &str) -> bool {
    matches!(status, "concluded" | "aborting" | "null")
}

/// Poll `query-jobs` until `job_id` reaches a terminal status or `timeout`
/// elapses. Returns the job's `error` field if it concluded with one.
fn wait_for_job(client: &mut QmpClient, job_id: &str, timeout: Duration) -> Result<(), QemuError> {
    let deadline = std::time::Instant::now() + timeout;
    let mut observed = false;
    loop {
        let jobs = client.execute(QmpCommand::query_jobs())?;
        let job = jobs
            .as_array()
            .and_then(|jobs| {
                jobs.iter()
                    .find(|job| job.get("id").and_then(Value::as_str) == Some(job_id))
            })
            .cloned();

        match job {
            Some(job) => {
                observed = true;
                let status = job
                    .get("status")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown");
                if job_status_is_terminal(status) {
                    if let Some(error) = job.get("error").and_then(Value::as_str) {
                        return Err(QemuError::QmpProtocol(format!(
                            "snapshot job '{job_id}' failed: {error}"
                        )));
                    }
                    return Ok(());
                }
            }
            // QEMU drops concluded jobs from `query-jobs` after dismissal. Only
            // treat a vanished job as complete once we've actually OBSERVED it
            // running -- otherwise a snapshot-save that failed/was reaped before
            // we ever saw it would be silently reported as a successful snapshot
            // (and resume would later -loadvm a snapshot that does not exist).
            None if observed => return Ok(()),
            None => {}
        }

        if std::time::Instant::now() >= deadline {
            return Err(QemuError::QmpProtocol(if observed {
                format!("timed out waiting for snapshot job '{job_id}'")
            } else {
                format!("snapshot job '{job_id}' was never observed; snapshot-save likely failed")
            }));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// Pause the guest and save a full internal VM snapshot into the primary
/// qcow2, then leave QEMU paused. Used by Compatibility Mode suspend.
///
/// Sequence: negotiate -> `stop` (pause CPUs) -> `snapshot-save` (job) ->
/// wait for the job to conclude. The caller is responsible for `quit`ing QEMU
/// afterwards.
pub fn suspend_to_snapshot(socket_path: &Path, timeout: Duration) -> Result<(), QemuError> {
    let mut client = QmpClient::connect_with_timeout(socket_path, Duration::from_secs(2))?;
    client.negotiate()?;
    let _ = client.execute(QmpCommand::stop())?;
    let devices = vec![COMPAT_PRIMARY_BLOCK_NODE.to_string()];
    let _ = client.execute(QmpCommand::snapshot_save(
        COMPAT_SUSPEND_SNAPSHOT_TAG,
        COMPAT_SUSPEND_SNAPSHOT_TAG,
        COMPAT_PRIMARY_BLOCK_NODE,
        &devices,
    ))?;
    wait_for_job(&mut client, COMPAT_SUSPEND_SNAPSHOT_TAG, timeout)?;
    Ok(())
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
        let mut frame = Vec::new();
        if (&mut self.reader)
            .take(MAX_QMP_ENVELOPE_BYTES + 1)
            .read_until(b'\n', &mut frame)?
            == 0
        {
            return Err(QemuError::QmpIo(std::io::Error::new(
                ErrorKind::UnexpectedEof,
                "QMP stream closed",
            )));
        }
        if frame.len() as u64 > MAX_QMP_ENVELOPE_BYTES {
            return Err(QemuError::QmpProtocol(format!(
                "QMP envelope exceeded {MAX_QMP_ENVELOPE_BYTES} bytes"
            )));
        }
        if frame.last() != Some(&b'\n') {
            return Err(QemuError::QmpProtocol(
                "QMP stream returned an incomplete envelope".to_string(),
            ));
        }
        let value = serde_json::from_slice::<Value>(&frame)?;
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

/// Escape a value interpolated into a comma-delimited QEMU option string (e.g.
/// `-drive file=...`, `-chardev socket,path=...`). QEMU parses these option
/// strings on commas, so a literal comma in a (manifest-derived) path must be
/// doubled (`,,`) or it would inject additional QEMU options.
fn escape_qemu_opt(value: impl std::fmt::Display) -> String {
    value.to_string().replace(',', ",,")
}

fn memory_arg(value: &str) -> String {
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

/// The TCP port VNC display `:0` listens on; display `:N` listens on
/// `VNC_BASE_PORT + N`.
const VNC_BASE_PORT: u16 = 5900;
/// How many VNC display numbers to scan for a free one before giving up.
const VNC_DISPLAY_SCAN_LIMIT: u16 = 64;

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
fn lowest_free_vnc_display(scan_limit: u16, avoid: &[u16]) -> Option<u16> {
    use std::net::TcpListener;
    (0..scan_limit).find(|display| {
        !avoid.contains(display)
            && TcpListener::bind(("127.0.0.1", VNC_BASE_PORT + display)).is_ok()
    })
}

fn netdev_arg(manifest: &VmManifest) -> Result<String, QemuError> {
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
    fn assign_free_vnc_display_skips_displays_already_in_use() {
        // The avoid-set models displays already handed to other live VMs. Even
        // though those ports may not be bound yet (QEMU binds late in startup),
        // the helper must not hand out a display that is in the avoid-set --
        // this is what stops two back-to-back launches from both getting :0.
        let mut command = QemuCommand {
            program: "qemu-system-aarch64".to_string(),
            args: vec!["-display".to_string(), "vnc=:0".to_string()],
        };
        assign_free_vnc_display(&mut command, &[0, 1]).expect("a free display should be assigned");
        let value = arg_after(&command.args, "-display");
        assert!(value.starts_with("vnc=:"), "still a vnc display: {value}");
        assert_ne!(value, "vnc=:0", "must skip avoided display :0");
        assert_ne!(value, "vnc=:1", "must skip avoided display :1");
        assert!(vnc_display_in_command(&command.args).unwrap() >= 2);
    }

    #[test]
    fn assign_free_vnc_display_is_noop_for_non_vnc_display() {
        let mut command = QemuCommand {
            program: "qemu-system-aarch64".to_string(),
            args: vec!["-display".to_string(), "cocoa,gl=on".to_string()],
        };
        assign_free_vnc_display(&mut command, &[]).expect("non-vnc display is a no-op");
        assert_eq!(arg_after(&command.args, "-display"), "cocoa,gl=on");
    }

    #[test]
    fn assign_free_vnc_display_errors_when_no_display_is_free() {
        // Exhaustion must be a hard error, NOT a silent fallback to the colliding
        // vnc=:0 template.
        let avoid: Vec<u16> = (0..VNC_DISPLAY_SCAN_LIMIT).collect();
        let mut command = QemuCommand {
            program: "qemu-system-aarch64".to_string(),
            args: vec!["-display".to_string(), "vnc=:0".to_string()],
        };
        let result = assign_free_vnc_display(&mut command, &avoid);
        assert!(result.is_err(), "exhausted display range must error");
        // The command must NOT be left on the colliding :0 silently — the caller
        // will propagate the error and fail the spawn.
        assert_eq!(arg_after(&command.args, "-display"), "vnc=:0");
    }

    #[test]
    fn vnc_display_in_command_parses_display_number() {
        let args = vec!["-display".to_string(), "vnc=:7".to_string()];
        assert_eq!(vnc_display_in_command(&args), Some(7));
        let cocoa = vec!["-display".to_string(), "cocoa,gl=on".to_string()];
        assert_eq!(vnc_display_in_command(&cocoa), None);
        assert_eq!(vnc_display_in_command(&[]), None);
    }

    #[test]
    fn disk_path_commas_are_escaped_in_the_drive_option() {
        // A comma in the disk path must be doubled so it can't inject extra QEMU
        // -drive options (e.g. flip readonly / add a backing file).
        let mut manifest = VmManifest::new(
            "comma-disk",
            VmMode::Compatibility,
            Guest {
                os: "ubuntu".to_string(),
                version: None,
                arch: "arm64".to_string(),
            },
            "80GiB",
        );
        manifest.storage.primary.path = "disks/root,if=none.qcow2".to_string();
        let command = build_compatibility_command(&manifest, Path::new("/tmp/x.vmbridge"))
            .expect("compat command");
        let drive = arg_after(&command.args, "-drive");
        assert!(
            drive.contains("root,,if=none.qcow2"),
            "comma not doubled in drive option: {drive}"
        );
        assert!(!drive.contains("root,if=none.qcow2"));
    }

    #[test]
    fn memory_arg_does_not_panic_or_wrap_on_huge_values() {
        // 2^54 GiB * 1024 overflows u64; must pass through rather than panic/wrap.
        assert_eq!(memory_arg("18014398509481984GiB"), "18014398509481984GiB");
        assert_eq!(memory_arg("4GiB"), "4096");
        assert_eq!(memory_arg("auto"), "4096");
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
        assert!(command.args.windows(2).any(|pair| pair[0] == "-device"
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

    fn win11_firmware_manifest() -> VmManifest {
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
    fn firmware_defaults_keep_bios_and_virtio_primary() {
        let command = build_compatibility_command(
            &win11_firmware_manifest(),
            Path::new("/tmp/win11.vmbridge"),
        )
        .expect("compat command");
        // Default firmware: read-only -bios, virtio-blk primary, no nvme/tpm/pflash.
        assert!(command
            .args
            .windows(2)
            .any(|p| p[0] == "-bios" && p[1] == "edk2-aarch64-code.fd"));
        assert!(command
            .args
            .iter()
            .any(|a| a.contains("if=virtio") && a.contains("node-name=bridgevm-root")));
        assert!(!command.args.iter().any(|a| a.contains("nvme")));
        assert!(!command.args.iter().any(|a| a.contains("tpm")));
        assert!(!command.args.iter().any(|a| a.contains("pflash")));
    }

    #[test]
    fn firmware_nvme_target_attaches_nvme_device() {
        let mut manifest = win11_firmware_manifest();
        manifest.firmware.nvme_target = true;
        let command = build_compatibility_command(&manifest, Path::new("/tmp/win11.vmbridge"))
            .expect("compat command");
        // Primary disk presented as an NVMe device (no virtio-blk), QMP node-name
        // preserved so snapshot/suspend still target the same block node.
        assert!(
            command
                .args
                .windows(2)
                .any(|p| p[0] == "-device"
                    && p[1] == "nvme,drive=bridgevm-nvme,serial=bridgevm-nvme0")
        );
        assert!(command.args.iter().any(|a| a.contains("if=none")
            && a.contains("id=bridgevm-nvme")
            && a.contains("node-name=bridgevm-root")));
        assert!(!command.args.iter().any(|a| a.contains("if=virtio")));
    }

    #[test]
    fn firmware_tpm_wires_swtpm_emulator_device() {
        let mut manifest = win11_firmware_manifest();
        manifest.firmware.tpm = true;
        let command = build_compatibility_command(&manifest, Path::new("/tmp/win11.vmbridge"))
            .expect("compat command");
        assert!(command
            .args
            .windows(2)
            .any(|p| p[0] == "-tpmdev" && p[1] == "emulator,id=tpm0,chardev=chrtpm"));
        assert!(command
            .args
            .windows(2)
            .any(|p| p[0] == "-device" && p[1] == "tpm-tis-device,tpmdev=tpm0"));
        assert!(command
            .args
            .iter()
            .any(|a| a.contains("socket,id=chrtpm,path=") && a.contains("swtpm.sock")));
    }

    #[test]
    fn firmware_secure_boot_swaps_bios_for_pflash_varstore() {
        let mut manifest = win11_firmware_manifest();
        manifest.firmware.secure_boot = true;
        let command = build_compatibility_command(&manifest, Path::new("/tmp/win11.vmbridge"))
            .expect("compat command");
        // Read-only code blob + writable per-bundle vars store via pflash; the
        // plain -bios is gone.
        assert!(command
            .args
            .iter()
            .any(|a| a == "if=pflash,format=raw,unit=0,readonly=on,file=edk2-aarch64-code.fd"));
        assert!(command.args.iter().any(|a| a.contains("if=pflash")
            && a.contains("unit=1")
            && a.contains("/tmp/win11.vmbridge/metadata/edk2-vars.fd")));
        assert!(!command.args.iter().any(|a| a == "-bios"));
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
    fn qemu_netdev_maps_bridged_mode_to_vmnet_bridged_with_default_interface() {
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

        // Bridged now emits real vmnet-bridged args attached to the default host
        // interface; it is no longer an "unimplemented" hard error. The runtime
        // privilege requirement (root / com.apple.vm.networking) is surfaced via
        // the network plan, not by failing arg generation.
        assert_eq!(
            netdev_arg(&manifest).expect("planned bridged netdev"),
            "vmnet-bridged,id=net0,ifname=en0"
        );

        // The full command builds successfully (and wires the netdev onto the
        // virtio-net device) instead of erroring out.
        let command = build_compatibility_command(&manifest, Path::new("/tmp/legacy.vmbridge"))
            .expect("bridged compat command builds");
        assert_eq!(
            arg_after(&command.args, "-netdev"),
            "vmnet-bridged,id=net0,ifname=en0"
        );
        assert!(command
            .args
            .iter()
            .any(|arg| arg == "virtio-net-pci,netdev=net0"));
    }

    #[test]
    fn qemu_netdev_honors_configured_bridge_interface() {
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
        manifest.network.bridge_interface = Some("en7".to_string());

        assert_eq!(
            netdev_arg(&manifest).expect("planned bridged netdev"),
            "vmnet-bridged,id=net0,ifname=en7"
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
            .expect_err("advanced QEMU launcher wiring requires a schema");

        assert!(
            matches!(
                &error,
                QemuError::UnsupportedNetworkRequirement {
                    mode,
                    blocker,
                    requirement
                } if mode == "advanced"
                    && blocker == "qemu-advanced-network-requires-schema"
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
    fn compatibility_command_names_primary_block_node() {
        let manifest = VmManifest::new(
            "compat",
            VmMode::Compatibility,
            Guest {
                os: "ubuntu".to_string(),
                version: None,
                arch: "x86_64".to_string(),
            },
            "40GiB",
        );
        let command =
            build_compatibility_command(&manifest, Path::new("/tmp/compat.vmbridge")).unwrap();
        let drive = command
            .args
            .iter()
            .find(|arg| arg.contains("if=virtio"))
            .expect("primary drive arg present");
        assert!(
            drive.contains(&format!("node-name={COMPAT_PRIMARY_BLOCK_NODE}")),
            "drive arg should name the primary block node: {drive}"
        );
    }

    #[test]
    fn builds_snapshot_save_command_for_suspend() {
        let devices = vec![COMPAT_PRIMARY_BLOCK_NODE.to_string()];
        let command = QmpCommand::snapshot_save(
            COMPAT_SUSPEND_SNAPSHOT_TAG,
            COMPAT_SUSPEND_SNAPSHOT_TAG,
            COMPAT_PRIMARY_BLOCK_NODE,
            &devices,
        );
        assert_eq!(
            serde_json::to_value(&command).unwrap(),
            json!({
                "execute": "snapshot-save",
                "arguments": {
                    "job-id": "bridgevm-suspend",
                    "tag": "bridgevm-suspend",
                    "vmstate": "bridgevm-root",
                    "devices": ["bridgevm-root"],
                }
            })
        );
    }

    #[test]
    fn builds_snapshot_load_command_for_resume() {
        let devices = vec![COMPAT_PRIMARY_BLOCK_NODE.to_string()];
        let command = QmpCommand::snapshot_load(
            COMPAT_SUSPEND_SNAPSHOT_TAG,
            COMPAT_SUSPEND_SNAPSHOT_TAG,
            COMPAT_PRIMARY_BLOCK_NODE,
            &devices,
        );
        assert_eq!(
            serde_json::to_value(&command).unwrap(),
            json!({
                "execute": "snapshot-load",
                "arguments": {
                    "job-id": "bridgevm-suspend",
                    "tag": "bridgevm-suspend",
                    "vmstate": "bridgevm-root",
                    "devices": ["bridgevm-root"],
                }
            })
        );
    }

    #[test]
    fn builds_query_jobs_command() {
        assert_eq!(
            serde_json::to_value(QmpCommand::query_jobs()).unwrap(),
            json!({ "execute": "query-jobs" })
        );
    }

    #[test]
    fn job_status_terminal_classification() {
        assert!(job_status_is_terminal("concluded"));
        assert!(job_status_is_terminal("aborting"));
        assert!(!job_status_is_terminal("running"));
        assert!(!job_status_is_terminal("created"));
    }

    #[test]
    fn wait_for_job_returns_when_job_concludes() {
        let socket_path = temp_socket_path();
        let listener = UnixListener::bind(&socket_path).unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .write_all(br#"{"QMP":{"version":{"qemu":{"major":8,"minor":2,"micro":0}}}}"#)
                .unwrap();
            stream.write_all(b"\n").unwrap();

            let mut reader = BufReader::new(stream.try_clone().unwrap());
            // capabilities
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            assert!(line.contains("qmp_capabilities"));
            stream.write_all(br#"{"return":{}}"#).unwrap();
            stream.write_all(b"\n").unwrap();

            // first query-jobs: still running
            line.clear();
            reader.read_line(&mut line).unwrap();
            assert!(line.contains("query-jobs"));
            stream
                .write_all(br#"{"return":[{"id":"bridgevm-suspend","status":"running"}]}"#)
                .unwrap();
            stream.write_all(b"\n").unwrap();

            // second query-jobs: concluded with no error
            line.clear();
            reader.read_line(&mut line).unwrap();
            assert!(line.contains("query-jobs"));
            stream
                .write_all(br#"{"return":[{"id":"bridgevm-suspend","status":"concluded"}]}"#)
                .unwrap();
            stream.write_all(b"\n").unwrap();
        });

        let mut client = QmpClient::connect(&socket_path).unwrap();
        client.negotiate().unwrap();
        wait_for_job(
            &mut client,
            COMPAT_SUSPEND_SNAPSHOT_TAG,
            Duration::from_secs(2),
        )
        .unwrap();

        server.join().unwrap();
        fs::remove_file(socket_path).unwrap();
    }

    #[test]
    fn wait_for_job_surfaces_job_error() {
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
            assert!(line.contains("query-jobs"));
            stream
                .write_all(
                    br#"{"return":[{"id":"bridgevm-suspend","status":"concluded","error":"disk full"}]}"#,
                )
                .unwrap();
            stream.write_all(b"\n").unwrap();
        });

        let mut client = QmpClient::connect(&socket_path).unwrap();
        client.negotiate().unwrap();
        let error = wait_for_job(
            &mut client,
            COMPAT_SUSPEND_SNAPSHOT_TAG,
            Duration::from_secs(2),
        )
        .unwrap_err();
        assert!(error.to_string().contains("disk full"));

        server.join().unwrap();
        fs::remove_file(socket_path).unwrap();
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
    fn qmp_client_rejects_oversized_envelope() {
        let socket_path = temp_socket_path();
        let listener = UnixListener::bind(&socket_path).unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let oversized = vec![b'x'; MAX_QMP_ENVELOPE_BYTES as usize + 1];
            let _ = stream.write_all(&oversized);
        });

        let mut client = QmpClient::connect(&socket_path).unwrap();
        let error = client.read_envelope().unwrap_err();
        assert!(error.to_string().contains("exceeded 1048576 bytes"));

        server.join().unwrap();
        fs::remove_file(socket_path).unwrap();
    }

    #[test]
    fn qmp_client_rejects_incomplete_envelope() {
        let socket_path = temp_socket_path();
        let listener = UnixListener::bind(&socket_path).unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream.write_all(br#"{"event":"SHUTDOWN"}"#).unwrap();
        });

        let mut client = QmpClient::connect(&socket_path).unwrap();
        let error = client.read_envelope().unwrap_err();
        assert!(error.to_string().contains("incomplete envelope"));

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
