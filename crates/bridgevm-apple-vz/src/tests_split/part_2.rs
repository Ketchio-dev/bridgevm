//! Split test module.

use super::helpers::*;
use crate::*;
use bridgevm_config::Boot;
use bridgevm_config::BootMode;
use std::path::Path;

#[test]
fn launch_readiness_reports_runner_limits_for_installer_qcow2() {
    let temp = std::env::temp_dir().join(format!(
        "bridgevm-apple-vz-readiness-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&temp);
    std::fs::create_dir_all(temp.join("disks")).unwrap();
    std::fs::create_dir_all(temp.join("installers")).unwrap();
    std::fs::write(temp.join("disks/root.qcow2"), b"disk").unwrap();
    std::fs::write(temp.join("installers/ubuntu-arm64.iso"), b"iso").unwrap();

    let mut manifest = valid_fast_manifest();
    manifest.boot = Some(Boot {
        mode: BootMode::LinuxInstaller,
        installer_image: Some("installers/ubuntu-arm64.iso".to_string()),
        kernel_path: None,
        initrd_path: None,
        kernel_command_line: None,
        macos_restore_image: None,
    });

    let plan = build_fast_plan(&manifest, &temp).unwrap();

    assert!(!plan.launch_spec.readiness.ready);
    assert!(plan
        .launch_spec
        .readiness
        .blockers
        .iter()
        .any(|blocker| blocker.code == "unsupported-live-boot-mode"));
    assert!(plan
        .launch_spec
        .readiness
        .blockers
        .iter()
        .any(|blocker| blocker.code == "unsupported-live-disk-format"));

    let _ = std::fs::remove_dir_all(&temp);
}

#[test]
fn launch_readiness_is_ready_for_linux_kernel_raw_when_required_paths_exist() {
    let temp = std::env::temp_dir().join(format!(
        "bridgevm-apple-vz-linux-kernel-readiness-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&temp);
    std::fs::create_dir_all(temp.join("disks")).unwrap();
    std::fs::create_dir_all(temp.join("boot")).unwrap();
    std::fs::write(temp.join("disks/root.raw"), b"disk").unwrap();
    std::fs::write(temp.join("boot/vmlinuz"), b"kernel").unwrap();

    let mut manifest = valid_fast_manifest();
    manifest.storage.primary.format = "raw".to_string();
    manifest.storage.primary.path = "disks/root.raw".to_string();
    manifest.boot = Some(Boot {
        mode: BootMode::LinuxKernel,
        installer_image: None,
        kernel_path: Some("boot/vmlinuz".to_string()),
        initrd_path: None,
        kernel_command_line: Some("console=hvc0 root=/dev/vda".to_string()),
        macos_restore_image: None,
    });

    let plan = build_fast_plan(&manifest, &temp).unwrap();

    if AppleVzHostCapability::current().is_macos()
        && AppleVzHostCapability::current().is_apple_silicon()
    {
        assert!(plan.launch_spec.readiness.ready);
        assert!(plan.launch_spec.readiness.blockers.is_empty());
    } else {
        assert!(!plan.launch_spec.readiness.ready);
        assert!(plan
            .launch_spec
            .readiness
            .blockers
            .iter()
            .all(|blocker| blocker.path.is_none()));
    }

    let _ = std::fs::remove_dir_all(&temp);
}

#[test]
fn launch_readiness_reports_unsupported_host_capabilities() {
    let boot = AppleVzBootSpec {
        mode: BootMode::ExistingDisk,
        installer_image: None,
        kernel: None,
        initrd: None,
        kernel_command_line: None,
        macos_restore_image: None,
    };
    let host = AppleVzHostCapability {
        os: "linux".to_string(),
        arch: "x86_64".to_string(),
    };
    let readiness = build_readiness_spec(&boot, "/tmp/root.qcow2", "raw", &host);

    assert!(!readiness.ready);
    assert!(readiness.blockers.iter().any(|blocker| {
        blocker.code == "unsupported-host-os"
            && blocker.capability.as_deref() == Some("apple-virtualization-framework")
    }));
    assert!(readiness.blockers.iter().any(|blocker| {
        blocker.code == "unsupported-host-arch"
            && blocker.capability.as_deref() == Some("apple-silicon")
    }));
}

#[test]
fn rejects_windows_guest_for_apple_vz() {
    let mut manifest = valid_fast_manifest();
    manifest.guest.os = "windows".to_string();

    let error = build_fast_plan(&manifest, Path::new("/tmp/windows.vmbridge"))
        .expect_err("Windows no-QEMU fast path is not Apple VZ");

    assert!(matches!(error, AppleVzError::UnsupportedGuestOs(os) if os == "windows"));
}

#[test]
fn rejects_missing_linux_installer_image() {
    let mut manifest = valid_fast_manifest();
    manifest.boot = Some(Boot {
        mode: BootMode::LinuxInstaller,
        installer_image: None,
        kernel_path: None,
        initrd_path: None,
        kernel_command_line: None,
        macos_restore_image: None,
    });

    let error = build_fast_plan(&manifest, Path::new("/tmp/ubuntu-fast.vmbridge"))
        .expect_err("linux-installer boot requires installerImage");

    assert!(matches!(
        error,
        AppleVzError::MissingBootInput {
            mode: BootMode::LinuxInstaller,
            field: "installerImage"
        }
    ));
}

#[test]
fn rejects_macos_restore_for_linux_guest() {
    let mut manifest = valid_fast_manifest();
    manifest.boot = Some(Boot {
        mode: BootMode::MacosRestore,
        installer_image: None,
        kernel_path: None,
        initrd_path: None,
        kernel_command_line: None,
        macos_restore_image: Some("restore.ipsw".to_string()),
    });

    let error = build_fast_plan(&manifest, Path::new("/tmp/ubuntu-fast.vmbridge"))
        .expect_err("macOS restore mode must not be accepted for Linux guests");

    assert!(
        matches!(error, AppleVzError::InvalidBootModeForGuest { guest_os, mode: BootMode::MacosRestore } if guest_os == "ubuntu")
    );
}

#[test]
fn rejects_unsupported_network_mode() {
    let mut manifest = valid_fast_manifest();
    manifest.network.mode = "bridged".to_string();

    let error = build_fast_plan(&manifest, Path::new("/tmp/ubuntu-fast.vmbridge"))
        .expect_err("non-nat networking must be rejected");

    assert!(matches!(error, AppleVzError::UnsupportedNetworkMode(mode) if mode == "bridged"));
}

#[test]
fn rejects_planned_host_only_network_mode_until_apple_vz_support_exists() {
    let mut manifest = valid_fast_manifest();
    manifest.network.mode = "host-only".to_string();

    let error = build_fast_plan(&manifest, Path::new("/tmp/ubuntu-fast.vmbridge"))
        .expect_err("Apple VZ launch boundary still accepts only NAT");

    assert!(matches!(error, AppleVzError::UnsupportedNetworkMode(mode) if mode == "host-only"));
}

#[test]
fn rejects_unknown_network_mode_name_through_planner_parsing() {
    let mut manifest = valid_fast_manifest();
    manifest.network.mode = "slirp".to_string();

    let error = build_fast_plan(&manifest, Path::new("/tmp/ubuntu-fast.vmbridge"))
        .expect_err("unknown network modes must be rejected");

    assert!(matches!(error, AppleVzError::UnsupportedNetworkMode(mode) if mode == "slirp"));
}

#[test]
fn accepts_raw_primary_disk() {
    let mut manifest = valid_fast_manifest();
    manifest.storage.primary.format = "raw".to_string();
    manifest.storage.primary.path = "disks/root.raw".to_string();

    let plan = build_fast_plan(&manifest, Path::new("/tmp/ubuntu-fast.vmbridge")).unwrap();

    assert!(plan.config.disk_path.ends_with("disks/root.raw"));
}

#[test]
fn rejects_unsupported_primary_disk_format() {
    let mut manifest = valid_fast_manifest();
    manifest.storage.primary.format = "vmdk".to_string();

    let error = build_fast_plan(&manifest, Path::new("/tmp/ubuntu-fast.vmbridge"))
        .expect_err("non-raw/qcow2 primary disks must be rejected");

    assert!(
        matches!(error, AppleVzError::UnsupportedPrimaryDiskFormat(format) if format == "vmdk")
    );
}

#[test]
fn build_fast_plan_populates_share_from_first_approved_folder() {
    let mut manifest = valid_fast_manifest();
    manifest.integration.shared_folders = true;
    manifest.shared_folders = vec![shared_folder("workspace", "/Users/me/work", true)];

    let plan = build_fast_plan(&manifest, Path::new("/tmp/ubuntu-fast.vmbridge")).unwrap();
    assert_eq!(plan.launch_spec.shares.len(), 1);
    let share = &plan.launch_spec.shares[0];

    assert_eq!(share.host_path, "/Users/me/work");
    assert_eq!(share.tag, "workspace");
    assert!(share.read_only);
}

#[test]
fn build_fast_plan_populates_all_approved_folders() {
    let mut manifest = valid_fast_manifest();
    manifest.integration.shared_folders = true;
    manifest.shared_folders = vec![
        shared_folder("first", "/Users/me/first", false),
        shared_folder("second", "/Users/me/second", true),
        shared_folder("third", "/Users/me/third", false),
    ];

    let plan = build_fast_plan(&manifest, Path::new("/tmp/ubuntu-fast.vmbridge")).unwrap();

    assert_eq!(plan.launch_spec.shares.len(), 3);
    assert_eq!(plan.launch_spec.shares[0].tag, "first");
    assert_eq!(plan.launch_spec.shares[0].host_path, "/Users/me/first");
    assert!(!plan.launch_spec.shares[0].read_only);
    assert_eq!(plan.launch_spec.shares[1].tag, "second");
    assert_eq!(plan.launch_spec.shares[1].host_path, "/Users/me/second");
    assert!(plan.launch_spec.shares[1].read_only);
    assert_eq!(plan.launch_spec.shares[2].tag, "third");
    assert_eq!(plan.launch_spec.shares[2].host_path, "/Users/me/third");
}

#[test]
fn build_fast_plan_omits_shares_when_no_folders_present() {
    let mut manifest = valid_fast_manifest();
    manifest.integration.shared_folders = true;
    manifest.shared_folders = Vec::new();

    let plan = build_fast_plan(&manifest, Path::new("/tmp/ubuntu-fast.vmbridge")).unwrap();

    assert!(plan.launch_spec.shares.is_empty());
}

#[test]
fn build_fast_plan_omits_shares_when_integration_flag_off() {
    let mut manifest = valid_fast_manifest();
    manifest.integration.shared_folders = false;
    manifest.shared_folders = vec![shared_folder("workspace", "/Users/me/work", false)];

    let plan = build_fast_plan(&manifest, Path::new("/tmp/ubuntu-fast.vmbridge")).unwrap();

    assert!(plan.launch_spec.shares.is_empty());
}

#[test]
fn build_share_specs_defaults_tag_to_share_for_empty_name() {
    let shares = build_share_specs(true, &[shared_folder("", "/Users/me/work", false)]);
    assert_eq!(shares.len(), 1);

    assert_eq!(shares[0].tag, "share");
    assert_eq!(shares[0].host_path, "/Users/me/work");
    assert!(!shares[0].read_only);
}

#[test]
fn build_share_specs_disambiguates_duplicate_and_empty_tags() {
    // Two unnamed folders both default to "share"; a third explicitly named
    // "share" also collides. Tags must stay unique for VZ.
    let folders = vec![
        shared_folder("", "/Users/me/a", false),
        shared_folder("", "/Users/me/b", true),
        shared_folder("share", "/Users/me/c", false),
        shared_folder("docs", "/Users/me/d", false),
        shared_folder("docs", "/Users/me/e", false),
    ];
    let shares = build_share_specs(true, &folders);

    let tags: Vec<&str> = shares.iter().map(|s| s.tag.as_str()).collect();
    assert_eq!(tags, ["share", "share-2", "share-3", "docs", "docs-2"]);
    // Host paths and read-only flags stay paired to their folders.
    assert_eq!(shares[0].host_path, "/Users/me/a");
    assert_eq!(shares[1].host_path, "/Users/me/b");
    assert!(shares[1].read_only);
}

#[test]
fn encode_share_flag_value_round_trips_tag_path_and_read_only() {
    let writable = AppleVzShareSpec {
        host_path: "/Users/me/work".to_string(),
        tag: "workspace".to_string(),
        read_only: false,
    };
    assert_eq!(
        encode_share_flag_value(&writable),
        "workspace=/Users/me/work"
    );

    let read_only = AppleVzShareSpec {
        host_path: "/Users/me/with=equals and spaces".to_string(),
        tag: "weird".to_string(),
        read_only: true,
    };
    // Split-on-first-`=` keeps the host path (with its own `=`/spaces) intact.
    let value = encode_share_flag_value(&read_only);
    assert_eq!(value, "ro:weird=/Users/me/with=equals and spaces");
    let stripped = value.strip_prefix("ro:").unwrap();
    let (tag, path) = stripped.split_once('=').unwrap();
    assert_eq!(tag, "weird");
    assert_eq!(path, "/Users/me/with=equals and spaces");
}

#[test]
fn render_runner_words_for_launch_spec_emits_exact_handoff_command() {
    let mut manifest = valid_fast_manifest();
    manifest.integration.shared_folders = true;
    manifest.shared_folders = vec![
        shared_folder("workspace", "/Users/me/work", true),
        shared_folder("docs", "/Users/me/docs", false),
    ];

    let plan = build_fast_plan(&manifest, Path::new("/tmp/ubuntu-fast.vmbridge")).unwrap();
    let launch_spec_path = Path::new("/tmp/ubuntu-fast.vmbridge/metadata/apple-vz-launch.json");
    let words = plan.render_runner_words_for_launch_spec(launch_spec_path);

    assert_eq!(
        words,
        vec![
            "lightvm-runner".to_string(),
            "--launch-spec".to_string(),
            launch_spec_path.display().to_string(),
        ]
    );
    assert_eq!(plan.launch_spec.shares.len(), 2);
    assert_eq!(plan.launch_spec.shares[0].tag, "workspace");
    assert_eq!(plan.launch_spec.shares[0].host_path, "/Users/me/work");
    assert!(plan.launch_spec.shares[0].read_only);
    assert_eq!(plan.launch_spec.shares[1].tag, "docs");
    assert_eq!(plan.launch_spec.shares[1].host_path, "/Users/me/docs");
    assert!(!plan.launch_spec.shares[1].read_only);
    assert!(!words.iter().any(|w| w == "--apple-vz"));
    assert!(!words.iter().any(|w| w == "--disk"));
    assert!(!words.iter().any(|w| w == "--memory"));
    assert!(!words.iter().any(|w| w == "--cpu"));
    assert!(!words.iter().any(|w| w == "--share"));
    assert!(!words.iter().any(|w| w == "--share-dir"));
}

#[test]
fn build_launch_handoff_omits_shares_when_none_planned() {
    let manifest = valid_fast_manifest();
    let plan = build_fast_plan(&manifest, Path::new("/tmp/ubuntu-fast.vmbridge")).unwrap();
    let handoff = build_launch_handoff(plan.launch_spec(), None);

    assert!(handoff.shares.is_empty());
}
