//! Split test module.

use crate::*;
use bridgevm_config::Boot;
use bridgevm_config::BootMode;
use bridgevm_config::Guest;
use bridgevm_config::PortForward;
use bridgevm_config::VmManifest;
use bridgevm_config::VmMode;
use bridgevm_network::NetworkPlanError;
use std::path::Path;

use super::helpers::*;

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
        .any(|arg| arg == "virtserialport,chardev=bridgevm-tools,name=org.bridgevm.guest-tools.0"));
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

#[test]
fn firmware_defaults_keep_bios_and_virtio_primary() {
    let command =
        build_compatibility_command(&win11_firmware_manifest(), Path::new("/tmp/win11.vmbridge"))
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
    assert!(command
        .args
        .windows(2)
        .any(|p| p[0] == "-device" && p[1] == "nvme,drive=bridgevm-nvme,serial=bridgevm-nvme0"));
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
