use bridgevm_config::{Boot, BootMode, VmMode};
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
    pub note: String,
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

    let windows_11_arm = os == "windows"
        && version.starts_with("11")
        && matches!(arch.as_str(), "arm64" | "aarch64");

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
            mode: VmMode::Fast,
            performance: "High for productivity workloads".to_string(),
            battery_impact: "Low to medium".to_string(),
            integration: "Experimental".to_string(),
            message: "Windows 11 Arm can use Fast Mode Experimental with a restricted backend. BridgeVM must not claim Microsoft-authorized status.".to_string(),
            fast_mode_available: true,
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
            note: "Place a macOS restore image at this path inside the .vmbridge bundle, or override it with --macos-restore-image.".to_string(),
        }),
        _ => None,
    }
}

pub fn available_boot_templates() -> Vec<BootTemplate> {
    [
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
    .collect()
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
                "fedora-arm64-installer",
                "debian-arm64-installer",
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
}
