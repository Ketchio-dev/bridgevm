//! QemuCommand and the full qemu-system argv build for Compatibility Mode.

use crate::*;
use bridgevm_config::BootMode;
use bridgevm_config::VmManifest;
use bridgevm_config::VmMode;
use bridgevm_resource_manager::decide_from_manifest_profile;
use bridgevm_resource_manager::resolve_memory;
use bridgevm_resource_manager::resolve_vcpu;
use serde::Deserialize;
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QemuCommand {
    pub program: String,
    pub args: Vec<String>,
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

/// Block node name QEMU assigns to the primary virtio drive in
/// [`build_compatibility_command`] (the qcow2 that receives suspend snapshots).
pub const COMPAT_PRIMARY_BLOCK_NODE: &str = "bridgevm-root";

/// `-drive` id for the primary disk when attached as an NVMe target (Windows 11
/// full-install firmware). The QMP block node-name stays [`COMPAT_PRIMARY_BLOCK_NODE`]
/// so snapshot/suspend keep working regardless of the bus.
pub const COMPAT_PRIMARY_NVME_DRIVE: &str = "bridgevm-nvme";

impl QemuCommand {
    pub fn render_shell_words(&self) -> Vec<String> {
        std::iter::once(self.program.clone())
            .chain(self.args.iter().cloned())
            .collect()
    }
}
