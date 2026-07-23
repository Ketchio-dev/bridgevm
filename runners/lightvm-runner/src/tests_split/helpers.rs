//! Split test module.

use crate::*;
use bridgevm_apple_vz::AppleVzBootSpec;
use bridgevm_apple_vz::AppleVzDeviceSpec;
use bridgevm_apple_vz::AppleVzDiskSpec;
use bridgevm_apple_vz::AppleVzGuestSpec;
use bridgevm_apple_vz::AppleVzIntegrationSpec;
use bridgevm_apple_vz::AppleVzLaunchSpec;
use bridgevm_apple_vz::AppleVzLogSpec;
use bridgevm_apple_vz::AppleVzPathSpec;
use bridgevm_apple_vz::AppleVzReadinessBlocker;
use bridgevm_apple_vz::AppleVzReadinessSpec;
use bridgevm_apple_vz::AppleVzResourceSpec;
use bridgevm_config::BootMode;

#[cfg(unix)]
#[test]
fn launch_handoff_passes_real_start_opt_in_to_helper() {
    use std::os::unix::fs::PermissionsExt;

    let temp = std::env::temp_dir().join(format!(
        "lightvm-runner-apple-vz-real-start-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&temp);
    std::fs::create_dir_all(&temp).unwrap();
    let helper = temp.join("helper.sh");
    let captured_arg = temp.join("arg.txt");
    let captured_env = temp.join("env.txt");
    std::fs::write(
        &helper,
        format!(
        "#!/bin/sh\ncat >/dev/null\nprintf '%s\\n' \"$@\" > '{}'\nprintf '%s\\n' \"$BRIDGEVM_APPLE_VZ_ALLOW_REAL_START\" > '{}'\n",
            captured_arg.display(),
            captured_env.display()
        ),
    )
    .unwrap();
    let mut permissions = std::fs::metadata(&helper).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&helper, permissions).unwrap();

    let spec = launch_spec_with_readiness(true, Vec::new());

    launch_handoff(
        &spec,
        None,
        AppleVzLaunchOptions {
            runner: Some(&helper),
            allow_real_start: true,
            stop_after_seconds: Some(5),
            force_stop_grace_seconds: Some(2),
            ..AppleVzLaunchOptions::default()
        },
    )
    .expect("helper launch should succeed");

    let captured_arg = std::fs::read_to_string(&captured_arg).unwrap();
    assert_eq!(
        captured_arg.trim(),
        "--allow-real-vz-start\n--stop-after-seconds\n5\n--force-stop-grace-seconds\n2"
    );
    let captured_env = std::fs::read_to_string(&captured_env).unwrap();
    assert_eq!(captured_env.trim(), "1");

    let _ = std::fs::remove_dir_all(&temp);
}

pub(super) fn launch_spec_with_readiness(
    ready: bool,
    blockers: Vec<AppleVzReadinessBlocker>,
) -> AppleVzLaunchSpec {
    AppleVzLaunchSpec {
        vm_name: "dev".to_string(),
        bundle_path: "/tmp/dev.vmbridge".to_string(),
        guest: AppleVzGuestSpec {
            os: "ubuntu".to_string(),
            arch: "arm64".to_string(),
        },
        boot: AppleVzBootSpec {
            mode: BootMode::LinuxInstaller,
            installer_image: Some(AppleVzPathSpec {
                path: "/tmp/dev.vmbridge/installers/ubuntu.iso".to_string(),
                exists: true,
            }),
            kernel: None,
            initrd: None,
            kernel_command_line: None,
            macos_restore_image: None,
        },
        disk: AppleVzDiskSpec {
            path: "/tmp/dev.vmbridge/disks/root.qcow2".to_string(),
            format: "qcow2".to_string(),
            read_only: false,
        },
        resources: AppleVzResourceSpec {
            memory: "4096".to_string(),
            cpu: "2".to_string(),
            display_fps_cap: "adaptive".to_string(),
            rationale: "Automatic balanced policy.".to_string(),
            balloon_device: true,
        },
        devices: AppleVzDeviceSpec {
            entropy_device: true,
            network: "nat".to_string(),
            serial_log_path: "/tmp/dev.vmbridge/logs/serial.log".to_string(),
        },
        integration: AppleVzIntegrationSpec {
            clipboard: true,
            dynamic_resolution: true,
            shared_folders: true,
            virtiofs: true,
        },
        logs: AppleVzLogSpec {
            runner_log_path: "/tmp/dev.vmbridge/logs/lightvm.log".to_string(),
        },
        shares: Vec::new(),
        readiness: AppleVzReadinessSpec { ready, blockers },
    }
}
