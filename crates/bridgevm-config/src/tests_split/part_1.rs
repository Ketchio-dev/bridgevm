//! Split test module.

use crate::*;
use std::fs;

#[test]
fn creates_fast_manifest_defaults() {
    let manifest = VmManifest::new(
        "Ubuntu Dev",
        VmMode::Fast,
        Guest {
            os: "ubuntu".to_string(),
            version: None,
            arch: "arm64".to_string(),
        },
        "80GiB",
    );
    assert_eq!(manifest.network.hostname, "ubuntu-dev.bridgevm.local");
    assert_eq!(manifest.backend.engine, "lightvm");
    assert_eq!(
        manifest.boot.as_ref().map(|boot| boot.mode),
        Some(BootMode::ExistingDisk)
    );
    assert!(manifest.integration.drag_drop);
}

#[test]
fn reads_legacy_manifest_without_boot_section() {
    let yaml = r#"
schemaVersion: bridgevm.io/v1
name: old-fast
mode: fast
guest:
  os: ubuntu
  arch: arm64
backend:
  engine: lightvm
resources:
  profile: automatic
  memory: auto
  cpu: auto
display:
  renderer: metal
  framePolicy: adaptive
  retina: true
storage:
  primary:
    path: disks/root.qcow2
    size: 80GiB
    format: qcow2
    discard: true
network:
  mode: nat
  hostname: old-fast.bridgevm.local
integration:
  tools: required
  clipboard: true
  dragDrop: true
  dynamicResolution: true
  sharedFolders: true
security:
  sharedFolderApproval: required
  guestCommandExecution: false
  signedAgentUpdates: true
"#;
    let manifest = serde_yaml::from_str::<VmManifest>(yaml).unwrap();

    assert!(manifest.boot.is_none());
    assert!(manifest.shared_folders.is_empty());
    assert!(!manifest.integration.applications);
    assert!(!manifest.integration.windows);
}

#[test]
fn parses_and_validates_shared_folder_entries() {
    let yaml = r#"
schemaVersion: bridgevm.io/v1
name: shares
mode: fast
guest:
  os: ubuntu
  arch: arm64
backend:
  engine: lightvm
resources:
  profile: automatic
  memory: auto
  cpu: auto
display:
  renderer: metal
  framePolicy: adaptive
  retina: true
storage:
  primary:
    path: disks/root.qcow2
    size: 80GiB
    format: qcow2
    discard: true
network:
  mode: nat
  hostname: shares.bridgevm.local
integration:
  tools: required
  clipboard: true
  dragDrop: true
  dynamicResolution: true
  sharedFolders: true
  applications: true
  windows: true
security:
  sharedFolderApproval: required
  guestCommandExecution: false
  signedAgentUpdates: true
sharedFolders:
  - name: workspace
    hostPath: /Users/me/project
    readOnly: false
    hostPathToken: share-token-workspace
  - name: downloads
    hostPath: /Users/me/Downloads
    readOnly: true
"#;
    let manifest = serde_yaml::from_str::<VmManifest>(yaml).unwrap();
    manifest.validate().unwrap();

    assert!(manifest.integration.applications);
    assert!(manifest.integration.windows);
    assert_eq!(manifest.shared_folders.len(), 2);
    assert_eq!(
        manifest.shared_folders[0].resolved_host_path_token(),
        "share-token-workspace"
    );
    assert!(manifest.shared_folders[1]
        .resolved_host_path_token()
        .starts_with("share-"));
}

#[test]
fn rejects_invalid_shared_folder_entries() {
    let mut manifest = VmManifest::new(
        "shares",
        VmMode::Fast,
        Guest {
            os: "ubuntu".to_string(),
            version: None,
            arch: "arm64".to_string(),
        },
        "80GiB",
    );
    manifest.shared_folders = vec![SharedFolder {
        name: "".to_string(),
        host_path: "/Users/me/project".to_string(),
        read_only: false,
        host_path_token: None,
    }];
    assert!(matches!(
        manifest.validate(),
        Err(ConfigError::EmptySharedFolderField { field: "name", .. })
    ));

    manifest.shared_folders = vec![SharedFolder {
        name: "   ".to_string(),
        host_path: "/Users/me/project".to_string(),
        read_only: false,
        host_path_token: None,
    }];
    assert!(matches!(
        manifest.validate(),
        Err(ConfigError::EmptySharedFolderField { field: "name", .. })
    ));

    manifest.shared_folders = vec![SharedFolder {
        name: "workspace".to_string(),
        host_path: "".to_string(),
        read_only: false,
        host_path_token: None,
    }];
    assert!(matches!(
        manifest.validate(),
        Err(ConfigError::EmptySharedFolderField {
            field: "hostPath",
            ..
        })
    ));

    manifest.shared_folders = vec![SharedFolder {
        name: "workspace".to_string(),
        host_path: "   ".to_string(),
        read_only: false,
        host_path_token: None,
    }];
    assert!(matches!(
        manifest.validate(),
        Err(ConfigError::EmptySharedFolderField {
            field: "hostPath",
            ..
        })
    ));

    manifest.shared_folders = vec![SharedFolder {
        name: "workspace".to_string(),
        host_path: "/Users/me/project".to_string(),
        read_only: false,
        host_path_token: Some("".to_string()),
    }];
    assert!(matches!(
        manifest.validate(),
        Err(ConfigError::EmptySharedFolderField {
            field: "hostPathToken",
            ..
        })
    ));

    manifest.shared_folders = vec![SharedFolder {
        name: "workspace".to_string(),
        host_path: "/Users/me/project".to_string(),
        read_only: false,
        host_path_token: Some("   ".to_string()),
    }];
    assert!(matches!(
        manifest.validate(),
        Err(ConfigError::EmptySharedFolderField {
            field: "hostPathToken",
            ..
        })
    ));

    manifest.shared_folders = vec![
        SharedFolder {
            name: "workspace".to_string(),
            host_path: "/Users/me/project".to_string(),
            read_only: false,
            host_path_token: Some("share-token".to_string()),
        },
        SharedFolder {
            name: "workspace".to_string(),
            host_path: "/Users/me/other".to_string(),
            read_only: false,
            host_path_token: Some("share-token-2".to_string()),
        },
    ];
    assert!(matches!(
        manifest.validate(),
        Err(ConfigError::DuplicateSharedFolderName { .. })
    ));

    manifest.shared_folders[1].name = "other".to_string();
    manifest.shared_folders[1].host_path_token = Some("share-token".to_string());
    assert!(matches!(
        manifest.validate(),
        Err(ConfigError::DuplicateSharedFolderToken { .. })
    ));
}

#[test]
fn rejects_boot_mode_missing_required_media() {
    let mut manifest = VmManifest::new(
        "Ubuntu Installer",
        VmMode::Fast,
        Guest {
            os: "ubuntu".to_string(),
            version: None,
            arch: "arm64".to_string(),
        },
        "80GiB",
    );
    manifest.boot = Some(Boot {
        mode: BootMode::LinuxInstaller,
        installer_image: None,
        kernel_path: None,
        initrd_path: None,
        kernel_command_line: None,
        macos_restore_image: None,
    });

    let error = manifest.validate().unwrap_err();

    assert!(matches!(
        error,
        ConfigError::MissingBootInput {
            mode: BootMode::LinuxInstaller,
            field: "installerImage"
        }
    ));
}

#[test]
fn firmware_defaults_disabled_and_omitted_from_serialization() {
    let manifest = VmManifest::new(
        "win11",
        VmMode::Compatibility,
        Guest {
            os: "windows".to_string(),
            version: Some("11".to_string()),
            arch: "arm64".to_string(),
        },
        "80GiB",
    );
    assert!(manifest.firmware.is_default());
    assert!(
        !manifest.firmware.nvme_target && !manifest.firmware.tpm && !manifest.firmware.secure_boot
    );
    // Legacy manifests never carried a firmware section, so a default one
    // must be omitted from output and deserialize back to the default.
    let yaml = serde_yaml::to_string(&manifest).unwrap();
    assert!(
        !yaml.contains("firmware"),
        "default firmware leaked: {yaml}"
    );
    let decoded: VmManifest = serde_yaml::from_str(&yaml).unwrap();
    assert_eq!(decoded.firmware, Firmware::default());
}

#[test]
fn firmware_round_trips_when_enabled() {
    let mut manifest = VmManifest::new(
        "win11",
        VmMode::Compatibility,
        Guest {
            os: "windows".to_string(),
            version: Some("11".to_string()),
            arch: "arm64".to_string(),
        },
        "80GiB",
    );
    manifest.firmware = Firmware {
        nvme_target: true,
        tpm: true,
        secure_boot: true,
    };
    let yaml = serde_yaml::to_string(&manifest).unwrap();
    assert!(yaml.contains("nvmeTarget"), "{yaml}");
    assert!(yaml.contains("secureBoot"), "{yaml}");
    let decoded: VmManifest = serde_yaml::from_str(&yaml).unwrap();
    assert_eq!(decoded.firmware, manifest.firmware);
    assert!(!decoded.firmware.is_default());
}

#[test]
fn manifest_json_schema_allows_firmware_section() {
    let schema = manifest_json_schema_v1();

    assert!(schema.contains("\"firmware\""), "{schema}");
    assert!(schema.contains("\"nvmeTarget\""), "{schema}");
    assert!(schema.contains("\"tpm\""), "{schema}");
    assert!(schema.contains("\"secureBoot\""), "{schema}");
}

#[test]
fn windows_installer_requires_installer_image() {
    let mut manifest = VmManifest::new(
        "Windows 11 Arm",
        VmMode::Compatibility,
        Guest {
            os: "windows".to_string(),
            version: Some("11".to_string()),
            arch: "arm64".to_string(),
        },
        "80GiB",
    );
    manifest.boot = Some(Boot {
        mode: BootMode::WindowsInstaller,
        installer_image: None,
        kernel_path: None,
        initrd_path: None,
        kernel_command_line: None,
        macos_restore_image: None,
    });

    assert!(matches!(
        manifest.validate().unwrap_err(),
        ConfigError::MissingBootInput {
            mode: BootMode::WindowsInstaller,
            field: "installerImage"
        }
    ));

    manifest.boot = Some(Boot {
        mode: BootMode::WindowsInstaller,
        installer_image: Some("media/win11.iso".to_string()),
        kernel_path: None,
        initrd_path: None,
        kernel_command_line: None,
        macos_restore_image: None,
    });
    assert!(manifest.validate().is_ok());
}

#[test]
fn rejects_empty_boot_media_path() {
    let mut manifest = VmManifest::new(
        "Ubuntu Kernel",
        VmMode::Fast,
        Guest {
            os: "ubuntu".to_string(),
            version: None,
            arch: "arm64".to_string(),
        },
        "80GiB",
    );
    manifest.boot = Some(Boot {
        mode: BootMode::LinuxKernel,
        installer_image: None,
        kernel_path: Some(" ".to_string()),
        initrd_path: None,
        kernel_command_line: None,
        macos_restore_image: None,
    });

    let error = manifest.validate().unwrap_err();

    assert!(matches!(
        error,
        ConfigError::EmptyBootInput {
            field: "kernelPath"
        }
    ));
}

#[test]
fn rejects_absolute_or_escaping_primary_disk_path() {
    let mut manifest = VmManifest::new(
        "disk-escape",
        VmMode::Compatibility,
        Guest {
            os: "ubuntu".to_string(),
            version: None,
            arch: "arm64".to_string(),
        },
        "80GiB",
    );
    // A legitimate bundle-relative path validates.
    assert!(manifest.validate().is_ok());
    for bad in ["/etc/passwd", "../../etc/shadow", "disks/../../../tmp/x"] {
        manifest.storage.primary.path = bad.to_string();
        assert!(
            matches!(
                manifest.validate(),
                Err(ConfigError::UnsafePath {
                    field: "storage.primary.path",
                    ..
                })
            ),
            "expected UnsafePath for {bad}"
        );
    }
}

#[test]
fn network_defaults_bridge_interface_for_legacy_manifests() {
    // A manifest that never carried `bridgeInterface` must still parse, and
    // the helper must fall back to the default host interface.
    let yaml = r#"
schemaVersion: bridgevm.io/v1
name: legacy-net
mode: compatibility
guest:
  os: ubuntu
  arch: x86_64
backend:
  engine: qemu
resources:
  profile: automatic
  memory: auto
  cpu: auto
display:
  renderer: spice-or-vnc
  framePolicy: adaptive
  retina: true
storage:
  primary:
    path: disks/root.qcow2
    size: 64GiB
    format: qcow2
    discard: false
network:
  mode: bridged
  hostname: legacy-net.bridgevm.local
integration:
  tools: optional
  clipboard: true
  dragDrop: false
  dynamicResolution: true
  sharedFolders: true
security:
  sharedFolderApproval: required
  guestCommandExecution: false
  signedAgentUpdates: true
"#;
    let manifest = serde_yaml::from_str::<VmManifest>(yaml).unwrap();
    manifest.validate().unwrap();

    assert_eq!(manifest.network.bridge_interface, None);
    assert_eq!(
        manifest.network.bridge_interface(),
        DEFAULT_BRIDGE_INTERFACE
    );
}

#[test]
fn network_uses_configured_bridge_interface_when_present() {
    let mut manifest = VmManifest::new(
        "bridged",
        VmMode::Compatibility,
        Guest {
            os: "ubuntu".to_string(),
            version: None,
            arch: "x86_64".to_string(),
        },
        "64GiB",
    );
    manifest.network.bridge_interface = Some("en7".to_string());
    assert_eq!(manifest.network.bridge_interface(), "en7");

    // A blank/whitespace override falls back to the default rather than
    // emitting an empty ifname.
    manifest.network.bridge_interface = Some("   ".to_string());
    assert_eq!(
        manifest.network.bridge_interface(),
        DEFAULT_BRIDGE_INTERFACE
    );
}

#[test]
fn rejects_name_that_slugs_to_empty() {
    let manifest = VmManifest::new(
        "!!!",
        VmMode::Fast,
        Guest {
            os: "ubuntu".to_string(),
            version: None,
            arch: "arm64".to_string(),
        },
        "80GiB",
    );
    assert!(matches!(
        manifest.validate(),
        Err(ConfigError::UnusableName { .. })
    ));
}

#[test]
fn read_rejects_oversized_manifest_before_yaml_decode() {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "bridgevm-config-oversized-{}-{nanos}.yaml",
        std::process::id()
    ));
    fs::write(&path, vec![b'x'; MAX_MANIFEST_BYTES as usize + 1]).unwrap();

    let error = VmManifest::read(&path).unwrap_err();
    assert!(matches!(
        error,
        ConfigError::ManifestTooLarge {
            actual,
            maximum: MAX_MANIFEST_BYTES
        } if actual == MAX_MANIFEST_BYTES + 1
    ));

    fs::remove_file(path).unwrap();
}

#[test]
fn read_rejects_sparse_oversized_manifest_before_allocation() {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "bridgevm-config-sparse-oversized-{}-{nanos}.yaml",
        std::process::id()
    ));
    let file = fs::File::create(&path).unwrap();
    file.set_len(512 * 1024 * 1024).unwrap();

    let error = read_manifest_bytes(&path).unwrap_err();
    let _ = fs::remove_file(&path);

    assert!(matches!(
        error,
        ConfigError::ManifestTooLarge {
            actual: 536_870_912,
            maximum: MAX_MANIFEST_BYTES
        }
    ));
}
