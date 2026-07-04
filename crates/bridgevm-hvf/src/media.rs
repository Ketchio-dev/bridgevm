//! Host-file media plumbing for the Path A `virt` platform.
//!
//! The live probes and the eventual engine-facing VM configuration both need the
//! same boring but important behavior: load bounded firmware/media files and
//! persist writable guest state (UEFI vars, raw disks) either to an explicit
//! snapshot path or back to the input file.

use std::{
    env, fs, io,
    path::{Path, PathBuf},
};

use crate::platform_virt::VirtPlatformDeviceConfig;

// The default firmware code volume is vendored in-repo: a current
// tianocore/edk2 ArmVirtQemu build. Homebrew's stale qemu-11.0.1
// `edk2-aarch64-code.fd` does NOT bind our NVMe endpoint (its older
// NvmExpressDxe/PciBus never reads the controller registers), whereas a
// firmware built from current edk2 binds it and boots Windows from NVMe.
// The variable store stays on the (version-insensitive) Homebrew template.
pub const DEFAULT_QEMU_AARCH64_CODE: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/firmware/edk2-aarch64-code.fd");
pub const DEFAULT_QEMU_AARCH64_VARS: &str =
    "/opt/homebrew/Cellar/qemu/11.0.1/share/qemu/edk2-arm-vars.fd";
pub const DEFAULT_LINUX_CMDLINE: &str = "console=ttyAMA0 earlycon=pl011,0x09000000 acpi=force";
pub const DEFAULT_RAM_MIB: u64 = 512;
pub const MIB: u64 = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaWriteKind {
    Snapshot,
    WriteBack,
}

impl MediaWriteKind {
    pub fn label(self, subject: &str) -> String {
        match self {
            Self::Snapshot => format!("{subject} snapshot written"),
            Self::WriteBack => format!("{subject} written back"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaWrite {
    pub kind: MediaWriteKind,
    pub path: PathBuf,
    pub bytes: usize,
}

/// A writable host-file media slot with optional snapshot/writeback policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WritableMedia {
    pub path: PathBuf,
    pub snapshot_path: Option<PathBuf>,
    pub write_back: bool,
}

impl WritableMedia {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            snapshot_path: None,
            write_back: false,
        }
    }

    pub fn with_snapshot_path(mut self, path: Option<impl Into<PathBuf>>) -> Self {
        self.snapshot_path = path.map(Into::into);
        self
    }

    pub fn with_write_back(mut self, write_back: bool) -> Self {
        self.write_back = write_back;
        self
    }

    pub fn read_bounded(&self, max_bytes: usize) -> io::Result<Vec<u8>> {
        read_bounded_file(&self.path, max_bytes)
    }

    pub fn persist(&self, bytes: &[u8]) -> io::Result<Vec<MediaWrite>> {
        let mut writes = Vec::new();
        if let Some(path) = self.snapshot_path.as_ref() {
            fs::write(path, bytes)?;
            writes.push(MediaWrite {
                kind: MediaWriteKind::Snapshot,
                path: path.clone(),
                bytes: bytes.len(),
            });
        }
        if self.write_back {
            fs::write(&self.path, bytes)?;
            writes.push(MediaWrite {
                kind: MediaWriteKind::WriteBack,
                path: self.path.clone(),
                bytes: bytes.len(),
            });
        }
        Ok(writes)
    }
}

/// QEMU-style direct Linux boot inputs exposed through fixed fw_cfg selectors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinuxBootMedia {
    pub kernel_path: PathBuf,
    pub initrd_path: Option<PathBuf>,
    pub cmdline: String,
}

impl LinuxBootMedia {
    pub fn new(kernel_path: impl Into<PathBuf>) -> Self {
        Self {
            kernel_path: kernel_path.into(),
            initrd_path: None,
            cmdline: DEFAULT_LINUX_CMDLINE.to_string(),
        }
    }

    pub fn with_initrd_path(mut self, path: Option<impl Into<PathBuf>>) -> Self {
        self.initrd_path = path.map(Into::into);
        self
    }

    pub fn with_cmdline(mut self, cmdline: impl Into<String>) -> Self {
        self.cmdline = cmdline.into();
        self
    }

    pub fn read_kernel_bounded(&self, max_bytes: usize) -> io::Result<Vec<u8>> {
        read_bounded_file(&self.kernel_path, max_bytes)
    }

    pub fn read_initrd_bounded(&self, max_bytes: usize) -> io::Result<Option<Vec<u8>>> {
        self.initrd_path
            .as_ref()
            .map(|path| read_bounded_file(path, max_bytes))
            .transpose()
    }

    /// EDK2's generic QEMU loader requires the command line blob to include its
    /// terminating NUL. Environment variables cannot carry NULs, so append one.
    pub fn cmdline_bytes(&self) -> Vec<u8> {
        let mut bytes = self.cmdline.as_bytes().to_vec();
        bytes.push(0);
        bytes
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallerIsoTransport {
    Pci,
    Mmio,
}

impl InstallerIsoTransport {
    pub fn from_env_value(value: Option<&str>) -> Self {
        let Some(value) = value else {
            return Self::Pci;
        };
        let value = value.trim();
        if value.eq_ignore_ascii_case("pci") {
            Self::Pci
        } else if value.eq_ignore_ascii_case("mmio") {
            Self::Mmio
        } else {
            panic!("BRIDGEVM_INSTALLER_ISO_TRANSPORT must be 'pci' or 'mmio'");
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Pci => "pci",
            Self::Mmio => "mmio",
        }
    }
}

/// Path A boot media selected for a live run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VirtBootMediaConfig {
    pub ram_size: u64,
    pub firmware_code_path: PathBuf,
    pub flash_vars: WritableMedia,
    pub installer_iso_path: Option<PathBuf>,
    pub installer_iso_transport: InstallerIsoTransport,
    pub nvme_disk: Option<WritableMedia>,
    /// Optional blank NSID-2 target namespace (Windows install destination),
    /// backed by a host raw file so a large sparse target avoids resident RAM.
    pub nvme_target: Option<WritableMedia>,
    pub linux_boot: Option<LinuxBootMedia>,
    pub platform_devices: VirtPlatformDeviceConfig,
}

impl VirtBootMediaConfig {
    pub fn qemu_defaults() -> Self {
        Self {
            ram_size: DEFAULT_RAM_MIB * MIB,
            firmware_code_path: PathBuf::from(DEFAULT_QEMU_AARCH64_CODE),
            flash_vars: WritableMedia::new(DEFAULT_QEMU_AARCH64_VARS),
            installer_iso_path: None,
            installer_iso_transport: InstallerIsoTransport::Pci,
            nvme_disk: None,
            nvme_target: None,
            linux_boot: None,
            platform_devices: VirtPlatformDeviceConfig::default(),
        }
    }

    pub fn from_probe_env() -> Self {
        let mut cfg = Self::qemu_defaults();
        if let Ok(mib) = env::var("BRIDGEVM_RAM_MIB") {
            let mib = mib
                .parse::<u64>()
                .expect("BRIDGEVM_RAM_MIB must be a positive integer");
            assert!(mib > 0, "BRIDGEVM_RAM_MIB must be non-zero");
            cfg.ram_size = mib.checked_mul(MIB).expect("BRIDGEVM_RAM_MIB overflow");
        }
        if let Ok(path) = env::var("BRIDGEVM_AARCH64_UEFI_CODE") {
            cfg.firmware_code_path = PathBuf::from(path);
        }
        if let Ok(path) = env::var("BRIDGEVM_AARCH64_UEFI_VARS") {
            cfg.flash_vars.path = PathBuf::from(path);
        }
        cfg.flash_vars.snapshot_path = env::var("BRIDGEVM_AARCH64_UEFI_VARS_OUT")
            .ok()
            .map(PathBuf::from);
        cfg.flash_vars.write_back = env_flag("BRIDGEVM_AARCH64_UEFI_VARS_WRITABLE");

        cfg.installer_iso_path = env::var("BRIDGEVM_INSTALLER_ISO").ok().map(PathBuf::from);
        cfg.installer_iso_transport = InstallerIsoTransport::from_env_value(
            env::var("BRIDGEVM_INSTALLER_ISO_TRANSPORT").ok().as_deref(),
        );

        cfg.nvme_disk = env::var("BRIDGEVM_NVME_DISK").ok().map(|path| {
            WritableMedia::new(path)
                .with_snapshot_path(env::var("BRIDGEVM_NVME_DISK_OUT").ok())
                .with_write_back(env_flag("BRIDGEVM_NVME_DISK_WRITABLE"))
        });

        cfg.nvme_target = env::var("BRIDGEVM_NVME_DISK2").ok().map(|path| {
            WritableMedia::new(path)
                .with_snapshot_path(env::var("BRIDGEVM_NVME_DISK2_OUT").ok())
                .with_write_back(env_flag("BRIDGEVM_NVME_DISK2_WRITABLE"))
        });

        cfg.linux_boot = env::var("BRIDGEVM_LINUX_KERNEL").ok().map(|path| {
            let cmdline =
                env::var("BRIDGEVM_LINUX_CMDLINE").unwrap_or_else(|_| DEFAULT_LINUX_CMDLINE.into());
            LinuxBootMedia::new(path)
                .with_initrd_path(env::var("BRIDGEVM_LINUX_INITRD").ok())
                .with_cmdline(cmdline)
        });

        cfg.platform_devices.xhci_present = !env_flag("BRIDGEVM_DISABLE_XHCI");
        let virtio_iso_present = !env_flag("BRIDGEVM_DISABLE_VIRTIO_ISO");
        cfg.platform_devices.virtio_boot_media_present = virtio_iso_present;
        cfg.platform_devices.legacy_virtio_mmio_present = virtio_iso_present;
        cfg.platform_devices.ramfb_present =
            env_flag("BRIDGEVM_RAMFB") && !env_flag("BRIDGEVM_DISABLE_RAMFB_DEVICE");
        cfg
    }
}

pub fn read_bounded_file(path: impl AsRef<Path>, max_bytes: usize) -> io::Result<Vec<u8>> {
    let path = path.as_ref();
    let data = fs::read(path)?;
    if data.len() > max_bytes {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "{} is {} bytes, larger than the {} byte region",
                path.display(),
                data.len(),
                max_bytes
            ),
        ));
    }
    Ok(data)
}

fn env_flag(name: &str) -> bool {
    let Ok(value) = env::var(name) else {
        return false;
    };
    let value = value.trim();
    value == "1"
        || value.eq_ignore_ascii_case("true")
        || value.eq_ignore_ascii_case("yes")
        || value.eq_ignore_ascii_case("on")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::time::{SystemTime, UNIX_EPOCH};

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn temp_path(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        env::temp_dir().join(format!(
            "bridgevm-hvf-media-{name}-{}-{nanos}",
            std::process::id()
        ))
    }

    fn clear_probe_disable_env() {
        for name in [
            "BRIDGEVM_DISABLE_XHCI",
            "BRIDGEVM_DISABLE_VIRTIO_ISO",
            "BRIDGEVM_RAMFB",
            "BRIDGEVM_DISABLE_RAMFB_DEVICE",
        ] {
            env::remove_var(name);
        }
    }

    #[test]
    fn bounded_read_rejects_oversized_file() {
        let path = temp_path("oversized");
        fs::write(&path, [1, 2, 3, 4]).unwrap();
        let err = read_bounded_file(&path, 3).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        fs::remove_file(path).ok();
    }

    #[test]
    fn writable_media_persists_snapshot_and_writeback() {
        let source = temp_path("source");
        let snapshot = temp_path("snapshot");
        fs::write(&source, [0xaa, 0xbb]).unwrap();

        let media = WritableMedia::new(&source)
            .with_snapshot_path(Some(&snapshot))
            .with_write_back(true);
        let writes = media.persist(&[0x11, 0x22, 0x33]).unwrap();
        assert_eq!(writes.len(), 2);
        assert_eq!(writes[0].kind, MediaWriteKind::Snapshot);
        assert_eq!(writes[0].path, snapshot);
        assert_eq!(writes[0].bytes, 3);
        assert_eq!(writes[1].kind, MediaWriteKind::WriteBack);
        assert_eq!(writes[1].path, source);
        assert_eq!(fs::read(&writes[0].path).unwrap(), [0x11, 0x22, 0x33]);
        assert_eq!(fs::read(&writes[1].path).unwrap(), [0x11, 0x22, 0x33]);

        fs::remove_file(&writes[0].path).ok();
        fs::remove_file(&writes[1].path).ok();
    }

    #[test]
    fn vendored_firmware_code_volume_exists() {
        // The default firmware code volume must ship in-repo so NVMe boot works
        // out of the box (the Homebrew firmware cannot bind our NVMe endpoint).
        let fw = PathBuf::from(DEFAULT_QEMU_AARCH64_CODE);
        let bytes = fs::read(&fw)
            .unwrap_or_else(|e| panic!("vendored firmware {} missing: {e}", fw.display()));
        // Sanity-check it is a real tianocore firmware volume by finding the
        // "_FVH" firmware-volume-header signature (ArmVirtQemu carries it a
        // little way into the image, not at a fixed offset).
        assert!(bytes.len() > 0x2000, "firmware volume truncated");
        assert!(
            bytes[..0x4000].windows(4).any(|w| w == b"_FVH"),
            "not an EDK2 firmware volume"
        );
    }

    #[test]
    fn qemu_defaults_are_the_stock_armvirtqemu_paths() {
        let cfg = VirtBootMediaConfig::qemu_defaults();
        assert_eq!(
            cfg.firmware_code_path,
            PathBuf::from(DEFAULT_QEMU_AARCH64_CODE)
        );
        assert_eq!(
            cfg.flash_vars.path,
            PathBuf::from(DEFAULT_QEMU_AARCH64_VARS)
        );
        assert_eq!(cfg.ram_size, DEFAULT_RAM_MIB * MIB);
        assert!(cfg.installer_iso_path.is_none());
        assert_eq!(cfg.installer_iso_transport, InstallerIsoTransport::Pci);
        assert!(cfg.nvme_disk.is_none());
        assert!(cfg.linux_boot.is_none());
        assert_eq!(
            cfg.platform_devices,
            crate::platform_virt::VirtPlatformDeviceConfig::default()
        );
    }

    #[test]
    fn probe_disable_env_parses_device_omission_switches() {
        let _guard = ENV_LOCK.lock().unwrap();
        clear_probe_disable_env();
        env::set_var("BRIDGEVM_DISABLE_XHCI", "true");
        env::set_var("BRIDGEVM_DISABLE_VIRTIO_ISO", "yes");
        env::set_var("BRIDGEVM_RAMFB", "1");
        env::set_var("BRIDGEVM_DISABLE_RAMFB_DEVICE", "on");

        let cfg = VirtBootMediaConfig::from_probe_env();

        assert!(!cfg.platform_devices.xhci_present);
        assert!(!cfg.platform_devices.virtio_boot_media_present);
        assert!(!cfg.platform_devices.legacy_virtio_mmio_present);
        assert!(!cfg.platform_devices.ramfb_present);
        clear_probe_disable_env();
    }

    #[test]
    fn probe_disable_env_keeps_devices_for_falsey_values() {
        let _guard = ENV_LOCK.lock().unwrap();
        clear_probe_disable_env();
        env::set_var("BRIDGEVM_DISABLE_XHCI", "0");
        env::set_var("BRIDGEVM_DISABLE_VIRTIO_ISO", "false");
        env::set_var("BRIDGEVM_RAMFB", "1");
        env::set_var("BRIDGEVM_DISABLE_RAMFB_DEVICE", "no");

        let cfg = VirtBootMediaConfig::from_probe_env();

        assert!(cfg.platform_devices.xhci_present);
        assert!(cfg.platform_devices.virtio_boot_media_present);
        assert!(cfg.platform_devices.legacy_virtio_mmio_present);
        assert!(cfg.platform_devices.ramfb_present);
        clear_probe_disable_env();
    }

    #[test]
    fn installer_iso_transport_defaults_to_pci_and_allows_mmio_fallback() {
        // Given no explicit installer ISO transport selector.
        let default_transport = InstallerIsoTransport::from_env_value(None);

        // Then PCI boot media is selected by default.
        assert_eq!(default_transport, InstallerIsoTransport::Pci);

        // Given the legacy virtio-mmio fallback selector.
        let fallback_transport = InstallerIsoTransport::from_env_value(Some("mmio"));

        // Then legacy virtio-mmio is selected explicitly.
        assert_eq!(fallback_transport, InstallerIsoTransport::Mmio);
    }

    #[test]
    fn linux_cmdline_bytes_are_nul_terminated() {
        let media = LinuxBootMedia::new("/tmp/Image").with_cmdline("console=ttyAMA0");
        assert_eq!(media.cmdline_bytes(), b"console=ttyAMA0\0");
    }

    #[test]
    fn linux_boot_media_reads_optional_initrd() {
        let kernel = temp_path("kernel");
        let initrd = temp_path("initrd");
        fs::write(&kernel, [0x4d, 0x5a]).unwrap();
        fs::write(&initrd, [1, 2, 3]).unwrap();

        let media = LinuxBootMedia::new(&kernel).with_initrd_path(Some(&initrd));
        assert_eq!(media.read_kernel_bounded(4).unwrap(), [0x4d, 0x5a]);
        assert_eq!(media.read_initrd_bounded(4).unwrap().unwrap(), [1, 2, 3]);

        fs::remove_file(kernel).ok();
        fs::remove_file(initrd).ok();
    }
}
