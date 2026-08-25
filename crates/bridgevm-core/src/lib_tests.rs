//! Unit tests for the crate root.
//!
//! Extracted from `lib.rs` to keep that file inside its structural budget.

use crate::*;

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
fn engine_product_state_matches_the_capability_registry() {
    let windows = windows_11_arm_no_qemu_engine_descriptor();
    assert_eq!(
        windows.product_state,
        EngineProductState::EngineeringPreview
    );
    assert_eq!(windows.product_state.as_str(), "ENGINEERING_PREVIEW");
    let detail = windows.product_state_detail;
    assert!(detail.contains("Runs an installed Windows 11 Arm desktop"));
    assert!(detail.contains("Release-blocking evidence remains open"));
    assert!(!detail.contains("every release-blocking criterion"));

    assert_eq!(
        engine_descriptor(EngineLane::AppleVz).product_state,
        EngineProductState::Proven
    );
    assert_eq!(
        engine_descriptor(EngineLane::QemuCompatibility).product_state,
        EngineProductState::Compatibility
    );
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
