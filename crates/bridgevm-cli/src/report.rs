//! Split out of main.rs by responsibility.

use crate::*;

pub(crate) fn json_bool(value: &serde_json::Value, key: &str) -> Option<bool> {
    match value.get(key)? {
        serde_json::Value::Bool(value) => Some(*value),
        serde_json::Value::String(value) => match value.trim().to_ascii_lowercase().as_str() {
            "true" | "1" | "yes" | "on" => Some(true),
            "false" | "0" | "no" | "off" => Some(false),
            _ => None,
        },
        _ => None,
    }
}

pub(crate) fn json_u64(value: &serde_json::Value, key: &str) -> Option<u64> {
    match value.get(key)? {
        serde_json::Value::Number(value) => value
            .as_u64()
            .or_else(|| value.as_i64().and_then(|signed| signed.try_into().ok())),
        serde_json::Value::String(value) => {
            let value = value.trim();
            value
                .strip_prefix("0x")
                .or_else(|| value.strip_prefix("0X"))
                .map(|hex| u64::from_str_radix(hex, 16).ok())
                .unwrap_or_else(|| value.parse().ok())
        }
        _ => None,
    }
}

pub(crate) fn hex_option(value: Option<u64>) -> String {
    value
        .map(|value| format!("{value:#x}"))
        .unwrap_or_else(|| "missing".to_string())
}

pub(crate) fn env_truthy(name: &str) -> bool {
    match env::var(name) {
        Ok(value) => matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        ),
        Err(_) => false,
    }
}

pub(crate) fn print_mode_recommendation(rec: &ModeRecommendation, choice: Option<&GuestChoice>) {
    println!("Recommended mode: {}", rec.mode);
    print_recommendation_engine_context(rec, choice);
    println!("Expected performance: {}", rec.performance);
    println!("Battery impact: {}", rec.battery_impact);
    println!("Integration: {}", rec.integration);
    println!("{}", rec.message);
    if let Some(template) = &rec.boot_template {
        print_boot_template(template);
    }
}

pub(crate) fn print_recommendation_engine_context(
    rec: &ModeRecommendation,
    choice: Option<&GuestChoice>,
) {
    let current = current_engine_descriptor_for_mode(rec.mode);
    println!(
        "Current execution engine: {} ({})",
        current.label,
        current.lane.id()
    );
    println!("Current engine substrate: {}", current.substrate);
    println!("Current engine QEMU usage: {}", current.qemu_usage);
    if let Some(target) = choice.and_then(target_engine_descriptor_for_guest) {
        println!(
            "Target product engine: {} ({})",
            target.label,
            target.lane.id()
        );
        println!("Target engine substrate: {}", target.substrate);
        println!("Target engine QEMU usage: {}", target.qemu_usage);
        println!("Target engine state: {}", target.product_state_detail);
    }
}

pub(crate) fn print_boot_template(template: &BootTemplate) {
    println!("Boot template id: {}", template.id);
    println!("Guest: {} {}", template.guest_os, template.guest_arch);
    println!("Boot template: {}", template.mode);
    println!("Boot media: {}", template.media_label);
    println!("Boot media source: {}", template.source);
    if let Some(path) = &template.installer_image {
        println!("Installer image: {path}");
    }
    if let Some(path) = &template.kernel_path {
        println!("Kernel path: {path}");
    }
    if let Some(path) = &template.initrd_path {
        println!("Initrd path: {path}");
    }
    if let Some(command_line) = &template.kernel_command_line {
        println!("Kernel command line: {command_line}");
    }
    if let Some(path) = &template.macos_restore_image {
        println!("macOS restore image: {path}");
    }
    if let Some(storage) = &template.storage {
        println!("Primary disk path: {}", storage.primary.path);
        println!("Primary disk format: {}", storage.primary.format);
        println!("Primary disk size: {}", storage.primary.size);
    }
    println!("Boot note: {}", template.note);
}

pub(crate) fn print_boot_templates(templates: &[BootTemplate]) {
    if templates.is_empty() {
        println!("No boot templates available");
        return;
    }
    for (index, template) in templates.iter().enumerate() {
        if index > 0 {
            println!();
        }
        print_boot_template(template);
    }
}

pub(crate) fn doctor(store: &VmStore) -> Result<()> {
    store.ensure().context("failed to prepare BridgeVM store")?;
    println!("BridgeVM store: {}", store.root().display());
    println!("VM bundles: {}", store.vms_dir().display());
    print_doctor_audit(&doctor_audit_for_current_host(store));
    print_engine_catalog(available_engine_descriptors());
    print_parallels_class_progress(&parallels_class_progress());
    println!("Status: OK");
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DoctorCheckStatus {
    Ok,
    Warn,
    Missing,
}

impl DoctorCheckStatus {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            DoctorCheckStatus::Ok => "OK",
            DoctorCheckStatus::Warn => "WARN",
            DoctorCheckStatus::Missing => "MISSING",
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) struct DoctorCheck {
    pub(crate) status: DoctorCheckStatus,
    pub(crate) name: String,
    pub(crate) detail: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ProductTrackStatus {
    Proven,
    Partial,
    Planned,
}

impl ProductTrackStatus {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            ProductTrackStatus::Proven => "PROVEN",
            ProductTrackStatus::Partial => "PARTIAL",
            ProductTrackStatus::Planned => "PLANNED",
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) struct ProductTrackProgress {
    pub(crate) status: ProductTrackStatus,
    pub(crate) name: &'static str,
    pub(crate) implemented: &'static str,
    pub(crate) next: &'static str,
}

#[derive(Debug)]
pub(crate) struct DoctorAuditInput {
    pub(crate) store_root: PathBuf,
    pub(crate) vms_dir: PathBuf,
    pub(crate) path_dirs: Vec<PathBuf>,
    pub(crate) os: String,
    pub(crate) arch: String,
}

pub(crate) fn doctor_audit_for_current_host(store: &VmStore) -> Vec<DoctorCheck> {
    doctor_audit_for_paths(store.root(), &store.vms_dir())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_store(prefix: &str) -> VmStore {
        let mut root = std::env::temp_dir();
        root.push(format!(
            "{prefix}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        VmStore::new(root)
    }

    fn write_executable(dir: &Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        fs::write(&path, "#!/bin/sh\n").unwrap();
        let mut permissions = fs::metadata(&path).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&path, permissions).unwrap();
        path
    }

    #[test]
    fn doctor_audit_reports_ready_macos_apple_silicon_host() {
        let store = unique_store("bridgevm-cli-doctor-ready-test");
        store.ensure().unwrap();
        let bin_dir = store.root().join("bin");
        fs::create_dir_all(&bin_dir).unwrap();
        let qemu_img = write_executable(&bin_dir, "qemu-img");
        let qemu_system = write_executable(&bin_dir, "qemu-system-aarch64");
        let lightvm_runner = write_executable(&bin_dir, "lightvm-runner");
        let fullvm_runner = write_executable(&bin_dir, "fullvm-runner");
        let networkd = write_executable(&bin_dir, "networkd");

        let checks = doctor_audit(&DoctorAuditInput {
            store_root: store.root().to_path_buf(),
            vms_dir: store.vms_dir().to_path_buf(),
            path_dirs: vec![bin_dir],
            os: "macos".to_string(),
            arch: "aarch64".to_string(),
        });

        assert!(checks.contains(&DoctorCheck {
            status: DoctorCheckStatus::Ok,
            name: "Store root".to_string(),
            detail: format!("{} exists", store.root().display()),
        }));
        assert!(checks.contains(&DoctorCheck {
            status: DoctorCheckStatus::Ok,
            name: "qemu-img".to_string(),
            detail: format!("found at {}", qemu_img.display()),
        }));
        assert!(checks.contains(&DoctorCheck {
            status: DoctorCheckStatus::Ok,
            name: "QEMU system binary".to_string(),
            detail: format!("found qemu-system-aarch64 at {}", qemu_system.display()),
        }));
        assert!(checks.contains(&DoctorCheck {
            status: DoctorCheckStatus::Ok,
            name: "lightvm-runner".to_string(),
            detail: format!("found at {}", lightvm_runner.display()),
        }));
        assert!(checks.contains(&DoctorCheck {
            status: DoctorCheckStatus::Ok,
            name: "fullvm-runner".to_string(),
            detail: format!("found at {}", fullvm_runner.display()),
        }));
        assert!(checks.contains(&DoctorCheck {
            status: DoctorCheckStatus::Ok,
            name: "networkd".to_string(),
            detail: format!("found at {}", networkd.display()),
        }));
        assert!(checks.contains(&DoctorCheck {
            status: DoctorCheckStatus::Ok,
            name: "Fast Mode possibility".to_string(),
            detail: format!(
                "macOS Apple Silicon host with lightvm-runner at {}",
                lightvm_runner.display()
            ),
        }));
    }

    #[test]
    fn doctor_audit_reports_missing_tools_without_machine_dependencies() {
        let store = unique_store("bridgevm-cli-doctor-missing-test");
        let checks = doctor_audit(&DoctorAuditInput {
            store_root: store.root().to_path_buf(),
            vms_dir: store.vms_dir().to_path_buf(),
            path_dirs: Vec::new(),
            os: "linux".to_string(),
            arch: "x86_64".to_string(),
        });

        assert!(checks.contains(&DoctorCheck {
            status: DoctorCheckStatus::Missing,
            name: "Store root".to_string(),
            detail: format!("{} does not exist", store.root().display()),
        }));
        assert!(checks.contains(&DoctorCheck {
            status: DoctorCheckStatus::Missing,
            name: "qemu-img".to_string(),
            detail: "required for qcow2 disk creation and snapshot overlays".to_string(),
        }));
        assert!(checks.contains(&DoctorCheck {
            status: DoctorCheckStatus::Missing,
            name: "QEMU system binary".to_string(),
            detail: "qemu-system-aarch64 or qemu-system-x86_64 was not found on PATH".to_string(),
        }));
        assert!(checks.contains(&DoctorCheck {
            status: DoctorCheckStatus::Warn,
            name: "networkd".to_string(),
            detail: "network helper candidate was not found on PATH".to_string(),
        }));
        assert!(checks.contains(&DoctorCheck {
            status: DoctorCheckStatus::Warn,
            name: "macOS host".to_string(),
            detail: "current host reports linux; Apple Virtualization is macOS-only".to_string(),
        }));
        assert!(checks.contains(&DoctorCheck {
            status: DoctorCheckStatus::Missing,
            name: "Fast Mode possibility".to_string(),
            detail: "Fast Mode requires macOS with Apple Virtualization".to_string(),
        }));
    }

    #[test]
    fn parallels_class_progress_tracks_honest_product_scope() {
        let tracks = parallels_class_progress();

        assert!(tracks.contains(&ProductTrackProgress {
            status: ProductTrackStatus::Partial,
            name: "macOS-native integration / Coherence",
            implemented:
                "clipboard/display resize foundations plus preserved Linux .desktop/gio/gtk-launch/wmctrl live GUI proof and crop/proxy plumbing",
            next: "drive real guest-window crops from real framebuffer/proxy sessions, then move toward compositor-grade host-window integration",
        }));
        assert!(tracks.contains(&ProductTrackProgress {
            status: ProductTrackStatus::Proven,
            name: "Apple Silicon Fast Mode",
            implemented:
                "Apple Virtualization.framework path with live Linux Arm64 boot/suspend/resume and VZVirtualMachineView display",
            next: "broaden boot shapes and keep app/daemon/helper IPC tight",
        }));
        assert!(tracks.contains(&ProductTrackProgress {
            status: ProductTrackStatus::Partial,
            name: "intelligent resources / battery",
            implemented:
                "power-aware launch policy, display pacing consumption, and runtime policy IPC",
            next: "live Apple VZ CPU/RAM control must apply the policy to a running VM",
        }));
        assert!(tracks.contains(&ProductTrackProgress {
            status: ProductTrackStatus::Planned,
            name: "graphics acceleration / Metal",
            implemented: "native VZ GUI pixels are proven in an AppKit display window",
            next: "Metal compositor/frame pacing first; Direct3D-to-Metal or WDDM remains long-term R&D",
        }));
    }
}
