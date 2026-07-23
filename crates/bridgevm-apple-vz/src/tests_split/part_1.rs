//! Split test module.

use super::helpers::*;
use crate::*;
use bridgevm_config::Boot;
use bridgevm_config::BootMode;
use bridgevm_config::Guest;
use bridgevm_config::VmManifest;
use bridgevm_config::VmMode;
use std::path::Path;

#[test]
fn builds_fast_mode_plan() {
    let manifest = valid_fast_manifest();
    let plan = build_fast_plan(&manifest, Path::new("/tmp/ubuntu-fast.vmbridge")).unwrap();
    assert_eq!(plan.vm_name, "Ubuntu Fast");
    assert!(plan.config.disk_path.ends_with("disks/root.qcow2"));
    assert_eq!(plan.launch_spec.vm_name, "Ubuntu Fast");
    assert_eq!(plan.launch_spec.guest.os, "ubuntu");
    assert_eq!(plan.launch_spec.guest.arch, "arm64");
    assert_eq!(plan.launch_spec.boot.mode, BootMode::ExistingDisk);
    assert_eq!(plan.launch_spec.disk.format, "qcow2");
    assert!(plan.launch_spec.disk.path.ends_with("disks/root.qcow2"));
    assert_eq!(plan.launch_spec.resources.memory, "4096");
    assert_eq!(plan.launch_spec.resources.cpu, "2");
    assert_eq!(plan.launch_spec.resources.display_fps_cap, "adaptive");
    assert_eq!(
        plan.launch_spec.resources.rationale,
        "Automatic balanced policy."
    );
    assert!(plan.launch_spec.resources.balloon_device);
    assert_eq!(plan.launch_spec.devices.network, "nat");
    assert!(plan
        .launch_spec
        .devices
        .serial_log_path
        .ends_with("logs/serial.log"));
    assert!(plan.launch_spec.integration.clipboard);
    assert!(plan.launch_spec.integration.dynamic_resolution);
    assert!(plan.launch_spec.integration.shared_folders);
    assert!(plan.launch_spec.integration.virtiofs);
    assert!(plan
        .launch_spec
        .logs
        .runner_log_path
        .ends_with("logs/lightvm.log"));
    assert!(!plan.launch_spec.readiness.ready);
    assert!(plan
        .launch_spec
        .readiness
        .blockers
        .iter()
        .any(|blocker| blocker.code == "missing-primary-disk"));
    assert_eq!(
        plan.render_runner_words().first().unwrap(),
        "lightvm-runner"
    );
}

#[test]
fn launch_spec_round_trips_as_json() {
    let manifest = valid_fast_manifest();
    let plan = build_fast_plan(&manifest, Path::new("/tmp/ubuntu-fast.vmbridge")).unwrap();

    let json = serde_json::to_string(&plan.launch_spec).expect("serialize launch spec");
    let decoded: AppleVzLaunchSpec = serde_json::from_str(&json).expect("deserialize launch spec");

    assert_eq!(decoded, plan.launch_spec);
}

#[test]
fn writes_launch_spec_artifact_to_metadata_directory() {
    let temp = std::env::temp_dir().join(format!(
        "bridgevm-apple-vz-launch-spec-artifact-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&temp);
    let manifest = valid_fast_manifest();
    let plan = build_fast_plan(&manifest, &temp).unwrap();

    let path = write_launch_spec_artifact(&temp, plan.launch_spec()).unwrap();
    let decoded = read_launch_spec_artifact(&path).unwrap();

    assert_eq!(path, launch_spec_path(&temp));
    assert_eq!(decoded, *plan.launch_spec());

    let _ = std::fs::remove_dir_all(&temp);
}

#[test]
fn rejects_oversized_launch_spec_artifact_before_decode() {
    let temp = std::env::temp_dir().join(format!(
        "bridgevm-apple-vz-oversized-launch-spec-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&temp);
    std::fs::create_dir_all(&temp).unwrap();
    let path = temp.join("launch.json");
    std::fs::write(&path, vec![b'x'; 1024 * 1024 + 1]).unwrap();

    let error = read_launch_spec_artifact(&path).unwrap_err();
    assert!(matches!(
        error,
        AppleVzLaunchSpecArtifactError::TooLarge {
            path: error_path,
            maximum: 1_048_576
        } if error_path == path
    ));

    let _ = std::fs::remove_dir_all(&temp);
}

#[test]
fn builds_launch_handoff_from_artifact_spec() {
    let temp = std::env::temp_dir().join(format!(
        "bridgevm-apple-vz-launch-handoff-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&temp);
    let manifest = valid_fast_manifest();
    let plan = build_fast_plan(&manifest, &temp).unwrap();
    let path = write_launch_spec_artifact(&temp, plan.launch_spec()).unwrap();
    let decoded = read_launch_spec_artifact(&path).unwrap();

    let handoff = build_launch_handoff(&decoded, Some(&path));

    assert_eq!(handoff.backend, "apple-virtualization-framework");
    assert_eq!(handoff.vm_name, "Ubuntu Fast");
    assert_eq!(
        handoff.launch_spec_path.as_deref(),
        Some(path.to_str().unwrap())
    );
    assert_eq!(handoff.guest, decoded.guest);
    assert_eq!(handoff.boot_mode, decoded.boot.mode);
    assert_eq!(handoff.disk, decoded.disk);
    assert_eq!(handoff.resources, decoded.resources);
    assert_eq!(handoff.runner_log_path, decoded.logs.runner_log_path);
    assert_eq!(handoff.serial_log_path, decoded.devices.serial_log_path);
    assert_eq!(handoff.integration, decoded.integration);
    assert_eq!(handoff.readiness, decoded.readiness);

    let _ = std::fs::remove_dir_all(&temp);
}

#[test]
fn unsupported_launcher_consumes_handoff_without_claiming_launch() {
    let manifest = valid_fast_manifest();
    let plan = build_fast_plan(&manifest, Path::new("/tmp/ubuntu-fast.vmbridge")).unwrap();
    let mut handoff = build_launch_handoff(plan.launch_spec(), None);
    handoff.readiness = AppleVzReadinessSpec {
        ready: true,
        blockers: Vec::new(),
    };

    let error = launch_with_apple_vz(&UnsupportedAppleVzLauncher, handoff.clone())
        .expect_err("default Apple VZ launcher must require the Swift helper");

    match error {
        AppleVzLaunchError::Unsupported {
            message,
            handoff: returned_handoff,
        } => {
            assert!(message.contains("--apple-vz-runner"));
            assert!(message.contains("signed AppleVzRunner"));
            assert_eq!(*returned_handoff, handoff);
        }
        other => panic!("expected unsupported launch error, got {other:?}"),
    }
}

#[cfg(unix)]
#[test]
fn command_launcher_sends_handoff_to_helper_stdin() {
    use std::os::unix::fs::PermissionsExt;

    let temp = std::env::temp_dir().join(format!(
        "bridgevm-apple-vz-command-launcher-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&temp);
    std::fs::create_dir_all(&temp).unwrap();
    let helper = temp.join("helper.sh");
    let captured = temp.join("handoff.json");
    std::fs::write(
        &helper,
        format!("#!/bin/sh\ncat > '{}'\n", captured.display()),
    )
    .unwrap();
    let mut permissions = std::fs::metadata(&helper).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&helper, permissions).unwrap();

    let manifest = valid_fast_manifest();
    let plan = build_fast_plan(&manifest, Path::new("/tmp/ubuntu-fast.vmbridge")).unwrap();
    let mut handoff = build_launch_handoff(plan.launch_spec(), None);
    handoff.readiness = AppleVzReadinessSpec {
        ready: true,
        blockers: Vec::new(),
    };

    let attempt = launch_with_apple_vz(&AppleVzCommandLauncher::new(&helper), handoff).unwrap();

    assert_eq!(attempt.backend, "apple-virtualization-framework");
    assert_eq!(attempt.vm_name, "Ubuntu Fast");
    assert_eq!(attempt.stdout, "");
    assert_eq!(attempt.stderr, "");
    let captured_json = std::fs::read_to_string(&captured).unwrap();
    assert!(captured_json.contains("\"vm_name\":\"Ubuntu Fast\""));

    let _ = std::fs::remove_dir_all(&temp);
}

#[cfg(unix)]
#[test]
fn command_launcher_sends_configured_helper_arguments_and_environment() {
    use std::os::unix::fs::PermissionsExt;

    let temp = std::env::temp_dir().join(format!(
        "bridgevm-apple-vz-command-launcher-args-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&temp);
    std::fs::create_dir_all(&temp).unwrap();
    let helper = temp.join("helper.sh");
    let captured_args = temp.join("args.txt");
    let captured_env = temp.join("env.txt");
    std::fs::write(
        &helper,
        format!(
            "#!/bin/sh\ncat >/dev/null\nprintf '%s\\n' \"$@\" > '{}'\nprintf '%s\\n' \"$BRIDGEVM_APPLE_VZ_ALLOW_REAL_START\" > '{}'\n",
            captured_args.display(),
            captured_env.display()
        ),
    )
    .unwrap();
    let mut permissions = std::fs::metadata(&helper).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&helper, permissions).unwrap();

    let manifest = valid_fast_manifest();
    let plan = build_fast_plan(&manifest, Path::new("/tmp/ubuntu-fast.vmbridge")).unwrap();
    let mut handoff = build_launch_handoff(plan.launch_spec(), None);
    handoff.readiness = AppleVzReadinessSpec {
        ready: true,
        blockers: Vec::new(),
    };

    launch_with_apple_vz(
        &AppleVzCommandLauncher::new(&helper)
            .arg("--allow-real-vz-start")
            .env("BRIDGEVM_APPLE_VZ_ALLOW_REAL_START", "1"),
        handoff,
    )
    .unwrap();

    let captured_args = std::fs::read_to_string(&captured_args).unwrap();
    assert_eq!(captured_args.trim(), "--allow-real-vz-start");
    let captured_env = std::fs::read_to_string(&captured_env).unwrap();
    assert_eq!(captured_env.trim(), "1");

    let _ = std::fs::remove_dir_all(&temp);
}

#[cfg(unix)]
#[test]
fn command_launcher_preserves_successful_helper_output() {
    use std::os::unix::fs::PermissionsExt;

    let temp = std::env::temp_dir().join(format!(
        "bridgevm-apple-vz-command-launcher-output-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&temp);
    std::fs::create_dir_all(&temp).unwrap();
    let helper = temp.join("helper.sh");
    std::fs::write(
        &helper,
        "#!/bin/sh\ncat >/dev/null\necho 'AppleVzRunner starting VM: Ubuntu Fast'\necho 'AppleVzRunner VM finished: Ubuntu Fast (stopped)'\necho 'runner diagnostic on stderr' >&2\n",
    )
    .unwrap();
    let mut permissions = std::fs::metadata(&helper).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&helper, permissions).unwrap();

    let manifest = valid_fast_manifest();
    let plan = build_fast_plan(&manifest, Path::new("/tmp/ubuntu-fast.vmbridge")).unwrap();
    let mut handoff = build_launch_handoff(plan.launch_spec(), None);
    handoff.readiness = AppleVzReadinessSpec {
        ready: true,
        blockers: Vec::new(),
    };

    let attempt = launch_with_apple_vz(&AppleVzCommandLauncher::new(&helper), handoff).unwrap();

    assert!(attempt
        .stdout
        .contains("AppleVzRunner starting VM: Ubuntu Fast"));
    assert!(attempt
        .stdout
        .contains("AppleVzRunner VM finished: Ubuntu Fast (stopped)"));
    assert_eq!(attempt.stderr, "runner diagnostic on stderr");

    let _ = std::fs::remove_dir_all(&temp);
}

#[cfg(unix)]
#[test]
fn command_launcher_rejects_oversized_stdout_without_pipe_deadlock() {
    use std::os::unix::fs::PermissionsExt;

    let temp = std::env::temp_dir().join(format!(
        "bridgevm-apple-vz-command-launcher-oversized-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&temp);
    std::fs::create_dir_all(&temp).unwrap();
    let helper = temp.join("helper.sh");
    std::fs::write(
        &helper,
        format!(
            "#!/bin/sh\ncat >/dev/null\nhead -c {} /dev/zero\n",
            MAX_LAUNCHER_STREAM_BYTES + 1
        ),
    )
    .unwrap();
    let mut permissions = std::fs::metadata(&helper).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&helper, permissions).unwrap();

    let manifest = valid_fast_manifest();
    let plan = build_fast_plan(&manifest, Path::new("/tmp/ubuntu-fast.vmbridge")).unwrap();
    let mut handoff = build_launch_handoff(plan.launch_spec(), None);
    handoff.readiness = AppleVzReadinessSpec {
        ready: true,
        blockers: Vec::new(),
    };

    let error = launch_with_apple_vz(&AppleVzCommandLauncher::new(&helper), handoff)
        .expect_err("oversized helper output must fail closed");

    assert!(matches!(
        error,
        AppleVzLaunchError::LauncherOutputTooLarge {
            stream: "stdout",
            maximum: MAX_LAUNCHER_STREAM_BYTES,
            ..
        }
    ));
    let _ = std::fs::remove_dir_all(&temp);
}

#[test]
fn launcher_stream_drain_caps_capture_and_consumes_to_eof() {
    let input = vec![0x41; LAUNCHER_DRAIN_CHUNK_BYTES * 3];
    let mut reader = std::io::Cursor::new(input.clone());

    let (captured, exceeded) =
        drain_launcher_stream(&mut reader, LAUNCHER_DRAIN_CHUNK_BYTES + 7).unwrap();

    assert!(exceeded);
    assert_eq!(captured, input[..LAUNCHER_DRAIN_CHUNK_BYTES + 7]);
    assert_eq!(reader.position(), input.len() as u64);
}

#[test]
fn launch_handoff_readiness_blocks_before_launcher_runs() {
    let manifest = valid_fast_manifest();
    let plan = build_fast_plan(&manifest, Path::new("/tmp/ubuntu-fast.vmbridge")).unwrap();
    let handoff = build_launch_handoff(plan.launch_spec(), None);

    let error = launch_with_apple_vz(&UnsupportedAppleVzLauncher, handoff.clone())
        .expect_err("not-ready handoff must be rejected before launch");

    match error {
        AppleVzLaunchError::NotReady { blockers } => {
            assert_eq!(blockers, handoff.readiness.blockers);
        }
        other => panic!("expected readiness launch error, got {other:?}"),
    }
}

#[test]
fn rejects_compatibility_manifest() {
    let manifest = VmManifest::new(
        "Legacy",
        VmMode::Compatibility,
        Guest {
            os: "ubuntu".to_string(),
            version: None,
            arch: "x86_64".to_string(),
        },
        "64GiB",
    );
    assert!(build_fast_plan(&manifest, Path::new("/tmp/legacy.vmbridge")).is_err());
}

#[test]
fn accepts_aarch64_guest_arch() {
    let mut manifest = valid_fast_manifest();
    manifest.guest.arch = "aarch64".to_string();

    let plan = build_fast_plan(&manifest, Path::new("/tmp/ubuntu-fast.vmbridge")).unwrap();

    assert_eq!(plan.guest_arch, "aarch64");
}

#[test]
fn rejects_unsupported_guest_arch() {
    let mut manifest = valid_fast_manifest();
    manifest.guest.arch = "x86_64".to_string();

    let error = build_fast_plan(&manifest, Path::new("/tmp/ubuntu-fast.vmbridge"))
        .expect_err("x86_64 guests must not enter Apple VZ launch planning");

    assert!(matches!(error, AppleVzError::UnsupportedGuestArch(arch) if arch == "x86_64"));
}

#[test]
fn rejects_unsupported_preferred_backend() {
    let mut manifest = valid_fast_manifest();
    manifest.backend.preferred = Some("qemu".to_string());

    let error = build_fast_plan(&manifest, Path::new("/tmp/ubuntu-fast.vmbridge"))
        .expect_err("non-Apple VZ preferred backend must be rejected");

    assert!(
        matches!(error, AppleVzError::UnsupportedPreferredBackend(backend) if backend == "qemu")
    );
}

#[test]
fn accepts_unset_preferred_backend() {
    let mut manifest = valid_fast_manifest();
    manifest.backend.preferred = None;

    let plan = build_fast_plan(&manifest, Path::new("/tmp/ubuntu-fast.vmbridge")).unwrap();

    assert_eq!(plan.vm_name, "Ubuntu Fast");
}

#[test]
fn plans_linux_installer_media() {
    let mut manifest = valid_fast_manifest();
    manifest.boot = Some(Boot {
        mode: BootMode::LinuxInstaller,
        installer_image: Some("installers/ubuntu-arm64.iso".to_string()),
        kernel_path: None,
        initrd_path: None,
        kernel_command_line: None,
        macos_restore_image: None,
    });

    let plan = build_fast_plan(&manifest, Path::new("/tmp/ubuntu-fast.vmbridge")).unwrap();
    let installer = plan.launch_spec.boot.installer_image.as_ref().unwrap();

    assert_eq!(plan.launch_spec.boot.mode, BootMode::LinuxInstaller);
    assert!(installer
        .path
        .ends_with("/tmp/ubuntu-fast.vmbridge/installers/ubuntu-arm64.iso"));
    assert!(!installer.exists);
    assert!(plan
        .launch_spec
        .readiness
        .blockers
        .iter()
        .any(|blocker| blocker.code == "missing-installer-image"));
}

#[test]
fn plans_linux_kernel_boot_inputs() {
    let mut manifest = valid_fast_manifest();
    manifest.boot = Some(Boot {
        mode: BootMode::LinuxKernel,
        installer_image: None,
        kernel_path: Some("boot/vmlinuz".to_string()),
        initrd_path: Some("boot/initrd".to_string()),
        kernel_command_line: Some("console=hvc0".to_string()),
        macos_restore_image: None,
    });

    let plan = build_fast_plan(&manifest, Path::new("/tmp/ubuntu-fast.vmbridge")).unwrap();
    let kernel = plan.launch_spec.boot.kernel.as_ref().unwrap();
    let initrd = plan.launch_spec.boot.initrd.as_ref().unwrap();

    assert_eq!(plan.launch_spec.boot.mode, BootMode::LinuxKernel);
    assert_eq!(plan.config.kernel_path, Some(kernel.path.clone()));
    assert!(kernel
        .path
        .ends_with("/tmp/ubuntu-fast.vmbridge/boot/vmlinuz"));
    assert!(initrd
        .path
        .ends_with("/tmp/ubuntu-fast.vmbridge/boot/initrd"));
    assert_eq!(
        plan.launch_spec.boot.kernel_command_line.as_deref(),
        Some("console=hvc0")
    );
    assert!(plan
        .launch_spec
        .readiness
        .blockers
        .iter()
        .any(|blocker| blocker.code == "missing-kernel"));
    assert!(plan
        .launch_spec
        .readiness
        .blockers
        .iter()
        .any(|blocker| blocker.code == "missing-initrd"));
}

#[test]
fn plans_macos_restore_image() {
    let mut manifest = valid_fast_manifest();
    manifest.guest.os = "macos".to_string();
    manifest.boot = Some(Boot {
        mode: BootMode::MacosRestore,
        installer_image: None,
        kernel_path: None,
        initrd_path: None,
        kernel_command_line: None,
        macos_restore_image: Some("/Library/BridgeVM/restore.ipsw".to_string()),
    });

    let plan = build_fast_plan(&manifest, Path::new("/tmp/macos.vmbridge")).unwrap();
    let restore = plan.launch_spec.boot.macos_restore_image.as_ref().unwrap();

    assert_eq!(plan.launch_spec.boot.mode, BootMode::MacosRestore);
    assert_eq!(restore.path, "/Library/BridgeVM/restore.ipsw");
    assert!(plan
        .launch_spec
        .readiness
        .blockers
        .iter()
        .any(|blocker| blocker.code == "missing-macos-restore-image"));
}
