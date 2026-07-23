//! Split out of lib.rs to keep files under 800 lines.

use crate::*;
use bridgevm_config::BootMode;
use bridgevm_config::VmManifest;
use bridgevm_config::VmMode;
use bridgevm_network::NetworkPlanError;
use bridgevm_resource_manager::decide_from_manifest_profile;
use bridgevm_resource_manager::resolve_memory;
use bridgevm_resource_manager::resolve_vcpu;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use std::io::BufRead;
use std::io::BufReader;
use std::io::ErrorKind;
use std::io::Read;
use std::io::Write;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;
use thiserror::Error;

pub(crate) const MAX_QMP_ENVELOPE_BYTES: u64 = 1024 * 1024;
pub(crate) const MAX_QMP_SKIPPED_ENVELOPES: usize = 1024;

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
pub(crate) fn primary_disk_args(
    manifest: &VmManifest,
    disk_path: &Path,
    is_aarch64: bool,
) -> Vec<String> {
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
pub(crate) fn job_status_is_terminal(status: &str) -> bool {
    matches!(status, "concluded" | "aborting" | "null")
}

/// Poll `query-jobs` until `job_id` reaches a terminal status or `timeout`
/// elapses. Returns the job's `error` field if it concluded with one.
pub(crate) fn wait_for_job(
    client: &mut QmpClient,
    job_id: &str,
    timeout: Duration,
) -> Result<(), QemuError> {
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
    pub(crate) fn empty() -> Self {
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
    pub(crate) reader: BufReader<UnixStream>,
    pub(crate) writer: UnixStream,
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
        for _ in 0..MAX_QMP_SKIPPED_ENVELOPES {
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
        Err(QemuError::QmpProtocol(format!(
            "QMP command skipped more than {MAX_QMP_SKIPPED_ENVELOPES} event envelopes"
        )))
    }

    pub fn read_event(&mut self) -> Result<QmpEvent, QemuError> {
        for _ in 0..MAX_QMP_SKIPPED_ENVELOPES {
            let envelope = self.read_envelope()?;
            if let Some(event) = envelope.event {
                return Ok(event);
            }
        }
        Err(QemuError::QmpProtocol(format!(
            "QMP event wait skipped more than {MAX_QMP_SKIPPED_ENVELOPES} non-event envelopes"
        )))
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
