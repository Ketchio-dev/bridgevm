use bridgevm_config::{Boot, BootMode, PrimaryDisk, VmMode};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuestChoice {
    pub os: String,
    pub version: Option<String>,
    pub arch: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModeRecommendation {
    pub mode: VmMode,
    pub performance: String,
    pub battery_impact: String,
    pub integration: String,
    pub message: String,
    pub fast_mode_available: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub boot_template: Option<BootTemplate>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineLane {
    AppleVz,
    BridgeHvf,
    QemuCompatibility,
}

impl EngineLane {
    pub fn id(self) -> &'static str {
        match self {
            EngineLane::AppleVz => "apple-vz",
            EngineLane::BridgeHvf => "bridge-hvf",
            EngineLane::QemuCompatibility => "qemu-compatibility",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineProductState {
    Proven,
    Compatibility,
    Research,
}

impl EngineProductState {
    pub fn as_str(self) -> &'static str {
        match self {
            EngineProductState::Proven => "PROVEN",
            EngineProductState::Compatibility => "COMPATIBILITY",
            EngineProductState::Research => "RESEARCH",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VmEngineDescriptor {
    pub lane: EngineLane,
    pub label: &'static str,
    pub substrate: &'static str,
    pub guest_scope: &'static str,
    pub windows_11_arm_role: &'static str,
    pub qemu_usage: &'static str,
    pub product_state: EngineProductState,
    pub product_state_detail: &'static str,
}

static ENGINE_DESCRIPTORS: [VmEngineDescriptor; 3] = [
    VmEngineDescriptor {
        lane: EngineLane::AppleVz,
        label: "Apple VZ Engine",
        substrate: "Apple Virtualization.framework",
        guest_scope: "Linux/macOS Arm Fast Mode",
        windows_11_arm_role: "not used for Windows 11 Arm Fast Mode in this project",
        qemu_usage: "not used",
        product_state: EngineProductState::Proven,
        product_state_detail: "live Linux Arm boot/display/suspend/resume evidence exists",
    },
    VmEngineDescriptor {
        lane: EngineLane::BridgeHvf,
        label: "BridgeVM HVF Engine",
        substrate: "Apple Hypervisor.framework plus BridgeVM VMM/device stack",
        guest_scope: "Windows 11 Arm no-QEMU fast path",
        windows_11_arm_role: "primary Parallels-like Windows 11 Arm target",
        qemu_usage: "not used",
        product_state: EngineProductState::Research,
        product_state_detail: "firmware/VMM probes exist, but Windows is not bootable yet",
    },
    VmEngineDescriptor {
        lane: EngineLane::QemuCompatibility,
        label: "QEMU Compatibility Engine",
        substrate: "QEMU with HVF/TCG acceleration when available",
        guest_scope: "broad guest compatibility and current Windows setup evidence",
        windows_11_arm_role: "compatibility fallback, not the Parallels-like target",
        qemu_usage: "required",
        product_state: EngineProductState::Compatibility,
        product_state_detail: "useful fallback with a UTM-class performance ceiling",
    },
];

pub fn available_engine_descriptors() -> &'static [VmEngineDescriptor] {
    &ENGINE_DESCRIPTORS
}

pub fn engine_descriptor(lane: EngineLane) -> &'static VmEngineDescriptor {
    available_engine_descriptors()
        .iter()
        .find(|descriptor| descriptor.lane == lane)
        .expect("all engine lanes must have descriptors")
}

pub fn windows_11_arm_no_qemu_engine_descriptor() -> &'static VmEngineDescriptor {
    engine_descriptor(EngineLane::BridgeHvf)
}

pub fn current_engine_descriptor_for_mode(mode: VmMode) -> &'static VmEngineDescriptor {
    match mode {
        VmMode::Fast => engine_descriptor(EngineLane::AppleVz),
        VmMode::Compatibility => engine_descriptor(EngineLane::QemuCompatibility),
    }
}

pub fn target_engine_descriptor_for_guest(
    choice: &GuestChoice,
) -> Option<&'static VmEngineDescriptor> {
    is_windows_11_arm(choice).then(windows_11_arm_no_qemu_engine_descriptor)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BootTemplate {
    pub id: String,
    pub guest_os: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guest_version: Option<String>,
    pub guest_arch: String,
    pub mode: BootMode,
    pub media_label: String,
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub installer_image: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kernel_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initrd_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kernel_command_line: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub macos_restore_image: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage: Option<BootTemplateStorageDefaults>,
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BootTemplateStorageDefaults {
    pub primary: BootTemplatePrimaryDiskDefaults,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BootTemplatePrimaryDiskDefaults {
    pub path: String,
    pub size: String,
    pub format: String,
}

impl BootTemplate {
    pub fn as_boot(&self) -> Boot {
        Boot {
            mode: self.mode,
            installer_image: self.installer_image.clone(),
            kernel_path: self.kernel_path.clone(),
            initrd_path: self.initrd_path.clone(),
            kernel_command_line: self.kernel_command_line.clone(),
            macos_restore_image: self.macos_restore_image.clone(),
        }
    }

    pub fn primary_disk_size(&self) -> Option<&str> {
        self.storage
            .as_ref()
            .map(|storage| storage.primary.size.as_str())
    }

    pub fn apply_storage_defaults(&self, primary: &mut PrimaryDisk) {
        if let Some(storage) = &self.storage {
            primary.path = storage.primary.path.clone();
            primary.size = storage.primary.size.clone();
            primary.format = storage.primary.format.clone();
        }
    }
}

pub fn recommend_mode(choice: &GuestChoice) -> ModeRecommendation {
    let os = choice.os.to_ascii_lowercase();
    let arch = choice.arch.to_ascii_lowercase();
    let version = choice
        .version
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase();

    let fast = matches!(os.as_str(), "ubuntu" | "fedora" | "debian" | "macos")
        && matches!(arch.as_str(), "arm64" | "aarch64");

    let windows_11_arm = is_windows_11_arm_parts(&os, &version, &arch);

    if fast {
        ModeRecommendation {
            mode: VmMode::Fast,
            performance: "High".to_string(),
            battery_impact: "Low".to_string(),
            integration: "Full when BridgeVM Tools are installed".to_string(),
            message: "Native optimized path available on Apple Silicon.".to_string(),
            fast_mode_available: true,
            boot_template: recommend_boot_template(choice),
        }
    } else if windows_11_arm {
        ModeRecommendation {
            mode: VmMode::Compatibility,
            performance: "Medium; restricted QEMU/HVF path today".to_string(),
            battery_impact: "Higher than Apple VZ Fast Mode".to_string(),
            integration: "Windows beta; not Apple VZ Fast Mode".to_string(),
            message: "Windows 11 Arm uses Compatibility Mode with a restricted QEMU/HVF backend today. Apple VZ Fast Mode is Linux/macOS Arm only; BridgeVM must not claim Microsoft-authorized or Parallels-class Windows support.".to_string(),
            fast_mode_available: false,
            boot_template: None,
        }
    } else {
        ModeRecommendation {
            mode: VmMode::Compatibility,
            performance: if arch == "x86_64" {
                "Medium to low on Apple Silicon"
            } else {
                "Medium"
            }
            .to_string(),
            battery_impact: "Higher".to_string(),
            integration: "Limited or partial".to_string(),
            message: "Fast Mode is not available for this operating system. Use Compatibility Mode instead.".to_string(),
            fast_mode_available: false,
            boot_template: None,
        }
    }
}

fn is_windows_11_arm(choice: &GuestChoice) -> bool {
    let os = choice.os.to_ascii_lowercase();
    let arch = choice.arch.to_ascii_lowercase();
    let version = choice
        .version
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase();
    is_windows_11_arm_parts(&os, &version, &arch)
}

fn is_windows_11_arm_parts(os: &str, version: &str, arch: &str) -> bool {
    os == "windows" && version.starts_with("11") && matches!(arch, "arm64" | "aarch64")
}

pub fn recommend_boot_template(choice: &GuestChoice) -> Option<BootTemplate> {
    let os = choice.os.to_ascii_lowercase();
    let arch = choice.arch.to_ascii_lowercase();
    if !matches!(arch.as_str(), "arm64" | "aarch64") {
        return None;
    }

    match os.as_str() {
        "ubuntu" | "fedora" | "debian" | "linux" => {
            let family = if os == "linux" { "linux" } else { os.as_str() };
            Some(BootTemplate {
                id: format!("{family}-arm64-installer"),
                guest_os: family.to_string(),
                guest_version: choice.version.clone(),
                guest_arch: "arm64".to_string(),
                mode: BootMode::LinuxInstaller,
                media_label: format!("{family} arm64 installer image"),
                source: "manual".to_string(),
                installer_image: Some(format!("installers/{family}-arm64.iso")),
                kernel_path: None,
                initrd_path: None,
                kernel_command_line: None,
                macos_restore_image: None,
                storage: None,
                note: "Place the installer image at this path inside the .vmbridge bundle, or override it with --installer-image.".to_string(),
            })
        }
        "macos" => Some(BootTemplate {
            id: "macos-restore".to_string(),
            guest_os: "macos".to_string(),
            guest_version: choice.version.clone(),
            guest_arch: "arm64".to_string(),
            mode: BootMode::MacosRestore,
            media_label: "macOS restore image".to_string(),
            source: "manual".to_string(),
            installer_image: None,
            kernel_path: None,
            initrd_path: None,
            kernel_command_line: None,
            macos_restore_image: Some("installers/macos-restore.ipsw".to_string()),
            storage: None,
            note: "Place a macOS restore image at this path inside the .vmbridge bundle, or override it with --macos-restore-image.".to_string(),
        }),
        _ => None,
    }
}

pub fn debian_apple_vz_linux_kernel_raw_template() -> BootTemplate {
    BootTemplate {
        id: "debian-arm64-apple-vz-linux-kernel-raw".to_string(),
        guest_os: "debian".to_string(),
        guest_version: None,
        guest_arch: "arm64".to_string(),
        mode: BootMode::LinuxKernel,
        media_label: "Debian arm64 Apple VZ linux-kernel raw-disk demo".to_string(),
        source: "bridgevm-live-fixture".to_string(),
        installer_image: None,
        kernel_path: Some("boot/vmlinuz".to_string()),
        initrd_path: Some("boot/initrd".to_string()),
        kernel_command_line: Some("console=hvc0 priority=low".to_string()),
        macos_restore_image: None,
        storage: Some(BootTemplateStorageDefaults {
            primary: BootTemplatePrimaryDiskDefaults {
                path: "disks/root.raw".to_string(),
                size: "64MiB".to_string(),
                format: "raw".to_string(),
            },
        }),
        note: "Place Debian Apple VZ demo fixtures at boot/vmlinuz, boot/initrd, and disks/root.raw inside the .vmbridge bundle, or override the boot fields when creating.".to_string(),
    }
}

pub fn ubuntu_apple_vz_linux_kernel_raw_template() -> BootTemplate {
    BootTemplate {
        id: "ubuntu-arm64-apple-vz-linux-kernel-raw".to_string(),
        guest_os: "ubuntu".to_string(),
        guest_version: None,
        guest_arch: "arm64".to_string(),
        mode: BootMode::LinuxKernel,
        media_label: "Ubuntu arm64 Apple VZ linux-kernel raw-disk desktop path".to_string(),
        source: "manual-direct-kernel".to_string(),
        installer_image: None,
        kernel_path: Some("boot/vmlinuz".to_string()),
        initrd_path: Some("boot/initrd".to_string()),
        kernel_command_line: Some(
            "console=hvc0 root=/dev/vda2 rw systemd.unit=graphical.target".to_string(),
        ),
        macos_restore_image: None,
        storage: Some(BootTemplateStorageDefaults {
            primary: BootTemplatePrimaryDiskDefaults {
                path: "disks/root.raw".to_string(),
                size: "32GiB".to_string(),
                format: "raw".to_string(),
            },
        }),
        note: "Place a matching Ubuntu arm64 kernel, initrd, and bootable raw root disk at boot/vmlinuz, boot/initrd, and disks/root.raw. This is the current no-QEMU Apple VZ live shape; adjust the kernel command line if the root partition is not /dev/vda2.".to_string(),
    }
}

pub fn available_boot_templates() -> Vec<BootTemplate> {
    let mut templates = [
        GuestChoice {
            os: "ubuntu".to_string(),
            version: None,
            arch: "arm64".to_string(),
        },
        GuestChoice {
            os: "fedora".to_string(),
            version: None,
            arch: "arm64".to_string(),
        },
        GuestChoice {
            os: "debian".to_string(),
            version: None,
            arch: "arm64".to_string(),
        },
        GuestChoice {
            os: "macos".to_string(),
            version: None,
            arch: "arm64".to_string(),
        },
    ]
    .iter()
    .filter_map(recommend_boot_template)
    .collect::<Vec<_>>();
    templates.insert(1, ubuntu_apple_vz_linux_kernel_raw_template());
    templates.insert(4, debian_apple_vz_linux_kernel_raw_template());
    templates
}

pub fn boot_template_by_id(id: &str) -> Option<BootTemplate> {
    available_boot_templates()
        .into_iter()
        .find(|template| template.id == id)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmState {
    Running,
    Suspended,
    Stopped,
}

pub trait VmEngine {
    fn name(&self) -> &'static str;
    fn start(&self, vm_name: &str) -> Result<VmState, String>;
    fn stop(&self, vm_name: &str) -> Result<VmState, String>;
    fn suspend(&self, vm_name: &str) -> Result<VmState, String>;
    fn resume(&self, vm_name: &str) -> Result<VmState, String>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recommends_fast_mode_for_ubuntu_arm64() {
        let rec = recommend_mode(&GuestChoice {
            os: "ubuntu".to_string(),
            version: None,
            arch: "arm64".to_string(),
        });
        assert_eq!(rec.mode, VmMode::Fast);
        assert_eq!(
            rec.boot_template.as_ref().map(|template| template.mode),
            Some(BootMode::LinuxInstaller)
        );
        assert_eq!(
            rec.boot_template
                .as_ref()
                .and_then(|template| template.installer_image.as_deref()),
            Some("installers/ubuntu-arm64.iso")
        );
    }

    #[test]
    fn recommends_compatibility_for_x86_guest() {
        let rec = recommend_mode(&GuestChoice {
            os: "ubuntu".to_string(),
            version: None,
            arch: "x86_64".to_string(),
        });
        assert_eq!(rec.mode, VmMode::Compatibility);
        assert!(rec.boot_template.is_none());
    }

    #[test]
    fn recommends_compatibility_for_windows_11_arm() {
        let rec = recommend_mode(&GuestChoice {
            os: "windows".to_string(),
            version: Some("11".to_string()),
            arch: "arm64".to_string(),
        });

        assert_eq!(rec.mode, VmMode::Compatibility);
        assert!(!rec.fast_mode_available);
        assert!(rec.boot_template.is_none());
        assert!(rec
            .message
            .contains("Apple VZ Fast Mode is Linux/macOS Arm only"));
    }

    #[test]
    fn engine_descriptors_keep_windows_no_qemu_target_separate() {
        let descriptors = available_engine_descriptors();

        assert_eq!(
            descriptors
                .iter()
                .map(|engine| engine.lane)
                .collect::<Vec<_>>(),
            vec![
                EngineLane::AppleVz,
                EngineLane::BridgeHvf,
                EngineLane::QemuCompatibility,
            ]
        );

        let windows_fast_path = windows_11_arm_no_qemu_engine_descriptor();
        assert_eq!(windows_fast_path.lane, EngineLane::BridgeHvf);
        assert_eq!(
            windows_fast_path.substrate,
            "Apple Hypervisor.framework plus BridgeVM VMM/device stack"
        );
        assert_eq!(windows_fast_path.qemu_usage, "not used");
        assert!(windows_fast_path
            .windows_11_arm_role
            .contains("primary Parallels-like Windows 11 Arm target"));

        let apple_vz = engine_descriptor(EngineLane::AppleVz);
        assert!(apple_vz
            .windows_11_arm_role
            .contains("not used for Windows 11 Arm Fast Mode"));

        let qemu = engine_descriptor(EngineLane::QemuCompatibility);
        assert_eq!(qemu.qemu_usage, "required");
    }

    #[test]
    fn engine_routing_distinguishes_current_mode_from_windows_target() {
        assert_eq!(
            current_engine_descriptor_for_mode(VmMode::Fast).lane,
            EngineLane::AppleVz
        );
        assert_eq!(
            current_engine_descriptor_for_mode(VmMode::Compatibility).lane,
            EngineLane::QemuCompatibility
        );

        let windows_target = target_engine_descriptor_for_guest(&GuestChoice {
            os: "windows".to_string(),
            version: Some("11".to_string()),
            arch: "arm64".to_string(),
        })
        .expect("Windows 11 Arm has a no-QEMU target engine");
        assert_eq!(windows_target.lane, EngineLane::BridgeHvf);

        let linux_target = target_engine_descriptor_for_guest(&GuestChoice {
            os: "ubuntu".to_string(),
            version: None,
            arch: "arm64".to_string(),
        });
        assert_eq!(linux_target, None);
    }

    #[test]
    fn recommends_macos_restore_template() {
        let template = recommend_boot_template(&GuestChoice {
            os: "macos".to_string(),
            version: None,
            arch: "arm64".to_string(),
        })
        .unwrap();

        assert_eq!(template.mode, BootMode::MacosRestore);
        assert_eq!(template.id, "macos-restore");
        assert_eq!(template.guest_os, "macos");
        assert_eq!(template.guest_arch, "arm64");
        assert_eq!(
            template.macos_restore_image.as_deref(),
            Some("installers/macos-restore.ipsw")
        );
    }

    #[test]
    fn lists_available_boot_templates() {
        let templates = available_boot_templates();
        let ids = templates
            .iter()
            .map(|template| template.id.as_str())
            .collect::<Vec<_>>();

        assert_eq!(
            ids,
            vec![
                "ubuntu-arm64-installer",
                "ubuntu-arm64-apple-vz-linux-kernel-raw",
                "fedora-arm64-installer",
                "debian-arm64-installer",
                "debian-arm64-apple-vz-linux-kernel-raw",
                "macos-restore"
            ]
        );
    }

    #[test]
    fn finds_boot_template_by_id() {
        let template = boot_template_by_id("fedora-arm64-installer").unwrap();

        assert_eq!(template.guest_os, "fedora");
        assert_eq!(template.guest_arch, "arm64");
        assert_eq!(
            template.installer_image.as_deref(),
            Some("installers/fedora-arm64.iso")
        );
    }

    #[test]
    fn finds_debian_apple_vz_linux_kernel_raw_template() {
        let template = boot_template_by_id("debian-arm64-apple-vz-linux-kernel-raw").unwrap();

        assert_eq!(template.guest_os, "debian");
        assert_eq!(template.guest_arch, "arm64");
        assert_eq!(template.mode, BootMode::LinuxKernel);
        assert_eq!(template.kernel_path.as_deref(), Some("boot/vmlinuz"));
        assert_eq!(template.initrd_path.as_deref(), Some("boot/initrd"));
        assert_eq!(
            template.kernel_command_line.as_deref(),
            Some("console=hvc0 priority=low")
        );

        let storage = template.storage.expect("storage defaults");
        assert_eq!(storage.primary.path, "disks/root.raw");
        assert_eq!(storage.primary.format, "raw");
        assert_eq!(storage.primary.size, "64MiB");
    }

    #[test]
    fn finds_ubuntu_apple_vz_linux_kernel_raw_template() {
        let template = boot_template_by_id("ubuntu-arm64-apple-vz-linux-kernel-raw").unwrap();

        assert_eq!(template.guest_os, "ubuntu");
        assert_eq!(template.guest_arch, "arm64");
        assert_eq!(template.mode, BootMode::LinuxKernel);
        assert_eq!(template.kernel_path.as_deref(), Some("boot/vmlinuz"));
        assert_eq!(template.initrd_path.as_deref(), Some("boot/initrd"));
        assert_eq!(
            template.kernel_command_line.as_deref(),
            Some("console=hvc0 root=/dev/vda2 rw systemd.unit=graphical.target")
        );

        let storage = template.storage.expect("storage defaults");
        assert_eq!(storage.primary.path, "disks/root.raw");
        assert_eq!(storage.primary.format, "raw");
        assert_eq!(storage.primary.size, "32GiB");
    }
}
