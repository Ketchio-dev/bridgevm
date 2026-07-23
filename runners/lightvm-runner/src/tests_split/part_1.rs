//! Split test module.

use super::helpers::*;
use crate::*;
use bridgevm_apple_vz::build_fast_plan;
use bridgevm_apple_vz::AppleVzReadinessBlocker;
use bridgevm_apple_vz::AppleVzReadinessSpec;
use bridgevm_apple_vz::AppleVzShareSpec;
use bridgevm_config::BootMode;
use bridgevm_config::Guest;
use bridgevm_config::VmManifest;
use bridgevm_config::VmMode;
use bridgevm_storage::VmRuntimeState;
use bridgevm_storage::VmStore;
use clap::Parser;
use std::path::PathBuf;

#[test]
fn ready_line_matches_no_vm_output() {
    assert_eq!(runner_ready_line("lightvm"), "lightvm runner ready");
}

#[test]
fn fast_mode_is_accepted() {
    ensure_fast_mode(VmMode::Fast).expect("fast mode should be accepted");
}

#[test]
fn compatibility_mode_is_rejected() {
    let error = ensure_fast_mode(VmMode::Compatibility).expect_err("compatibility must fail");

    assert!(
        error
            .to_string()
            .contains("lightvm-runner only supports Fast Mode manifests, got compatibility"),
        "unexpected error: {error}"
    );
}

#[test]
fn launch_readiness_metadata_preserves_blockers() {
    let readiness = AppleVzReadinessSpec {
        ready: false,
        blockers: vec![
            AppleVzReadinessBlocker {
                code: "missing-kernel".to_string(),
                message: "kernel image is required".to_string(),
                path: Some("images/vmlinuz".to_string()),
                capability: None,
            },
            AppleVzReadinessBlocker {
                code: "missing-initrd".to_string(),
                message: "initrd image is required".to_string(),
                path: None,
                capability: Some("apple-virtualization-framework".to_string()),
            },
        ],
    };

    let metadata = launch_readiness_metadata(&readiness);

    assert!(!metadata.ready);
    assert_eq!(metadata.blockers.len(), 2);
    assert_eq!(metadata.blockers[0].code, "missing-kernel");
    assert_eq!(metadata.blockers[0].message, "kernel image is required");
    assert_eq!(
        metadata.blockers[0].path,
        Some(PathBuf::from("images/vmlinuz"))
    );
    assert_eq!(metadata.blockers[1].code, "missing-initrd");
    assert_eq!(metadata.blockers[1].path, None);
    assert_eq!(
        metadata.blockers[1].capability.as_deref(),
        Some("apple-virtualization-framework")
    );
}

#[test]
fn apple_vz_real_start_flag_parses_next_to_runner_path() {
    let args = Args::try_parse_from([
        "lightvm-runner",
        "dev",
        "--launch",
        "--apple-vz-runner",
        "/tmp/AppleVzRunner",
        "--apple-vz-allow-real-start",
        "--apple-vz-stop-after-seconds",
        "5",
        "--apple-vz-force-stop-grace-seconds",
        "2",
    ])
    .expect("Apple VZ runner opt-in flags should parse");

    assert_eq!(args.vm.as_deref(), Some("dev"));
    assert_eq!(
        args.apple_vz_runner.as_deref(),
        Some(std::path::Path::new("/tmp/AppleVzRunner"))
    );
    assert!(args.apple_vz_allow_real_start);
    assert_eq!(args.apple_vz_stop_after_seconds, Some(5));
    assert_eq!(args.apple_vz_force_stop_grace_seconds, Some(2));

    let default_args =
        Args::try_parse_from(["lightvm-runner", "dev"]).expect("default args should parse");
    assert!(!default_args.apple_vz_allow_real_start);
    assert_eq!(default_args.apple_vz_stop_after_seconds, None);
    assert_eq!(default_args.apple_vz_force_stop_grace_seconds, None);
    assert_eq!(default_args.apple_vz_save_state, None);
    assert_eq!(default_args.apple_vz_restore_state, None);
}

#[test]
fn launch_spec_flag_parses_without_vm_name() {
    let args = Args::try_parse_from([
        "lightvm-runner",
        "--launch-spec",
        "/tmp/ubuntu-fast.vmbridge/metadata/apple-vz-launch.json",
    ])
    .expect("launch spec handoff should parse without a VM name");

    assert_eq!(args.vm, None);
    assert_eq!(
        args.launch_spec.as_deref(),
        Some(std::path::Path::new(
            "/tmp/ubuntu-fast.vmbridge/metadata/apple-vz-launch.json"
        ))
    );
}

#[test]
fn apple_vz_suspend_resume_state_flags_parse() {
    let save = Args::try_parse_from([
        "lightvm-runner",
        "dev",
        "--launch",
        "--apple-vz-runner",
        "/tmp/AppleVzRunner",
        "--apple-vz-save-state",
        "/tmp/state.bin",
    ])
    .expect("save-state flag should parse");
    assert_eq!(
        save.apple_vz_save_state.as_deref(),
        Some(std::path::Path::new("/tmp/state.bin"))
    );
    assert_eq!(save.apple_vz_restore_state, None);

    let restore = Args::try_parse_from([
        "lightvm-runner",
        "dev",
        "--launch",
        "--apple-vz-runner",
        "/tmp/AppleVzRunner",
        "--apple-vz-restore-state",
        "/tmp/state.bin",
    ])
    .expect("restore-state flag should parse");
    assert_eq!(
        restore.apple_vz_restore_state.as_deref(),
        Some(std::path::Path::new("/tmp/state.bin"))
    );
}

#[test]
fn require_ready_accepts_ready_launch_spec() {
    let mut spec = launch_spec_with_readiness(true, Vec::new());

    ensure_launch_ready(&spec).expect("ready launch spec should pass");

    spec.readiness.ready = false;
    spec.readiness.blockers.push(AppleVzReadinessBlocker {
        code: "missing-primary-disk".to_string(),
        message: "Primary disk is missing.".to_string(),
        path: Some("/tmp/dev.vmbridge/disks/root.qcow2".to_string()),
        capability: None,
    });
    let error = ensure_launch_ready(&spec).expect_err("blocked launch spec must fail");
    let message = error.to_string();
    assert!(message.contains("Fast Mode launch readiness failed"));
    assert!(message.contains("missing-primary-disk"));
    assert!(message.contains("/tmp/dev.vmbridge/disks/root.qcow2"));
}

#[test]
fn record_live_launch_metadata_writes_running_state_and_runtime_control() {
    let temp = std::env::temp_dir().join(format!(
        "lightvm-runner-live-metadata-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&temp);
    let store = VmStore::new(&temp);
    let mut manifest = VmManifest::new(
        "dev",
        VmMode::Fast,
        Guest {
            os: "ubuntu".to_string(),
            version: None,
            arch: "arm64".to_string(),
        },
        "80GiB",
    );
    manifest.storage.primary.path = "disks/root.raw".to_string();
    manifest.storage.primary.format = "raw".to_string();
    manifest.boot = Some(bridgevm_config::Boot {
        mode: BootMode::LinuxKernel,
        installer_image: None,
        kernel_path: Some("boot/vmlinuz".to_string()),
        initrd_path: None,
        kernel_command_line: Some("console=hvc0".to_string()),
        macos_restore_image: None,
    });
    store.create_vm(&manifest).unwrap();
    let bundle = store.bundle_path("dev");
    let plan = build_fast_plan(&manifest, &bundle).unwrap();
    let socket = temp.join("bvm-vz.sock");
    let framebuffer = bundle
        .join("metadata")
        .join("apple-vz-display-framebuffer.rgba");
    let command = vec![
        "lightvm-runner".to_string(),
        "dev".to_string(),
        "--launch".to_string(),
        "--apple-vz-display".to_string(),
        "--apple-vz-display-width".to_string(),
        "1440".to_string(),
        "--apple-vz-display-height".to_string(),
        "900".to_string(),
        "--apple-vz-runtime-control-socket".to_string(),
        socket.display().to_string(),
        "--apple-vz-proxy-framebuffer-rgba-file".to_string(),
        framebuffer.display().to_string(),
    ];

    record_live_launch_metadata(
        &store,
        "dev",
        "lightvm",
        &plan,
        Some(bundle.join("metadata/apple-vz-launch.json")),
        Some(socket.clone()),
        command.clone(),
    )
    .unwrap();

    let metadata = store.runner_metadata("dev").unwrap().unwrap();
    assert_eq!(metadata.engine, "lightvm");
    assert_eq!(metadata.pid, Some(std::process::id()));
    assert_eq!(metadata.command, command);
    assert!(!metadata.dry_run);
    assert_eq!(
        metadata
            .runtime_control
            .as_ref()
            .map(|control| &control.socket_path),
        Some(&socket)
    );
    assert_eq!(
        metadata.runtime_control.as_ref().unwrap().commands,
        vec![
            "status".to_string(),
            "stop".to_string(),
            "policy".to_string(),
            "pacing".to_string(),
        ]
    );
    assert_eq!(store.state("dev").unwrap().state, VmRuntimeState::Running);

    let _ = std::fs::remove_dir_all(&temp);
}

#[test]
fn launch_handoff_rejects_blocked_spec_before_launcher() {
    let spec = launch_spec_with_readiness(
        false,
        vec![AppleVzReadinessBlocker {
            code: "missing-primary-disk".to_string(),
            message: "Primary disk is missing.".to_string(),
            path: Some("/tmp/dev.vmbridge/disks/root.qcow2".to_string()),
            capability: None,
        }],
    );

    let error = launch_handoff(&spec, None, AppleVzLaunchOptions::default())
        .expect_err("blocked launch must fail");
    let message = format!("{error:#}");

    assert!(message.contains("failed to launch Apple VZ handoff"));
    assert!(message.contains("Fast Mode launch readiness failed"));
    assert!(message.contains("missing-primary-disk"));
    assert!(!message.contains("not implemented yet"));
}

#[test]
fn launch_handoff_reaches_unsupported_launcher_for_ready_spec() {
    let spec = launch_spec_with_readiness(true, Vec::new());

    let error = launch_handoff(
        &spec,
        Some(&PathBuf::from(
            "/tmp/dev.vmbridge/metadata/apple-vz-launch.json",
        )),
        AppleVzLaunchOptions::default(),
    )
    .expect_err("default Apple VZ launcher must require the Swift helper");
    let message = format!("{error:#}");

    assert!(message.contains("failed to launch Apple VZ handoff"));
    assert!(message.contains("--apple-vz-runner"));
    assert!(message.contains("signed AppleVzRunner"));
}

#[cfg(unix)]
#[test]
fn launch_handoff_forwards_save_state_to_helper() {
    use std::os::unix::fs::PermissionsExt;

    let temp = std::env::temp_dir().join(format!(
        "lightvm-runner-apple-vz-save-state-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&temp);
    std::fs::create_dir_all(&temp).unwrap();
    let helper = temp.join("helper.sh");
    let captured_arg = temp.join("arg.txt");
    std::fs::write(
        &helper,
        format!(
            "#!/bin/sh\ncat >/dev/null\nprintf '%s\\n' \"$@\" > '{}'\n",
            captured_arg.display()
        ),
    )
    .unwrap();
    let mut permissions = std::fs::metadata(&helper).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&helper, permissions).unwrap();

    let spec = launch_spec_with_readiness(true, Vec::new());
    let state = temp.join("suspend.bin");
    launch_handoff(
        &spec,
        None,
        AppleVzLaunchOptions {
            runner: Some(&helper),
            allow_real_start: true,
            stop_after_seconds: Some(5),
            save_state: Some(&state),
            ..AppleVzLaunchOptions::default()
        },
    )
    .expect("helper launch should succeed");

    let captured = std::fs::read_to_string(&captured_arg).unwrap();
    assert!(
        captured.contains(&format!("--save-state\n{}", state.display())),
        "helper did not receive --save-state: {captured}"
    );

    let _ = std::fs::remove_dir_all(&temp);
}

#[cfg(unix)]
#[test]
fn launch_handoff_forwards_display_to_helper() {
    use std::os::unix::fs::PermissionsExt;

    let temp = std::env::temp_dir().join(format!(
        "lightvm-runner-apple-vz-display-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&temp);
    std::fs::create_dir_all(&temp).unwrap();
    let helper = temp.join("helper.sh");
    let captured_arg = temp.join("arg.txt");
    std::fs::write(
        &helper,
        format!(
            "#!/bin/sh\ncat >/dev/null\nprintf '%s\\n' \"$@\" > '{}'\n",
            captured_arg.display()
        ),
    )
    .unwrap();
    let mut permissions = std::fs::metadata(&helper).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&helper, permissions).unwrap();

    let spec = launch_spec_with_readiness(true, Vec::new());
    let framebuffer = temp.join("display.rgba");
    launch_handoff(
        &spec,
        None,
        AppleVzLaunchOptions {
            runner: Some(&helper),
            allow_real_start: true,
            display: true,
            display_width: Some(1440),
            display_height: Some(900),
            proxy_framebuffer_rgba_file: Some(&framebuffer),
            proxy_framebuffer_capture_interval_ms: Some(250),
            ..AppleVzLaunchOptions::default()
        },
    )
    .expect("helper launch should succeed");

    let captured = std::fs::read_to_string(&captured_arg).unwrap();
    assert!(
        captured.contains("--display"),
        "helper did not receive --display: {captured}"
    );
    assert!(
        captured.contains("--display-width\n1440\n--display-height\n900"),
        "helper did not receive display dimensions: {captured}"
    );
    assert!(
        captured.contains(&format!(
            "--proxy-framebuffer-rgba-file\n{}",
            framebuffer.display()
        )),
        "helper did not receive proxy framebuffer export path: {captured}"
    );
    assert!(
        captured.contains("--proxy-framebuffer-capture-interval-ms\n250"),
        "helper did not receive proxy framebuffer export interval: {captured}"
    );

    let _ = std::fs::remove_dir_all(&temp);
}

#[cfg(unix)]
#[test]
fn launch_handoff_forwards_all_shares_to_helper() {
    use std::os::unix::fs::PermissionsExt;

    let temp = std::env::temp_dir().join(format!(
        "lightvm-runner-apple-vz-shares-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&temp);
    std::fs::create_dir_all(&temp).unwrap();
    let helper = temp.join("helper.sh");
    let captured_arg = temp.join("arg.txt");
    std::fs::write(
        &helper,
        format!(
            "#!/bin/sh\ncat >/dev/null\nprintf '%s\\n' \"$@\" > '{}'\n",
            captured_arg.display()
        ),
    )
    .unwrap();
    let mut permissions = std::fs::metadata(&helper).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&helper, permissions).unwrap();

    let mut spec = launch_spec_with_readiness(true, Vec::new());
    spec.shares = vec![
        AppleVzShareSpec {
            host_path: "/Users/me/work".to_string(),
            tag: "workspace".to_string(),
            read_only: true,
        },
        AppleVzShareSpec {
            host_path: "/Users/me/docs".to_string(),
            tag: "docs".to_string(),
            read_only: false,
        },
    ];
    launch_handoff(
        &spec,
        None,
        AppleVzLaunchOptions {
            runner: Some(&helper),
            allow_real_start: true,
            ..AppleVzLaunchOptions::default()
        },
    )
    .expect("helper launch should succeed");

    let captured = std::fs::read_to_string(&captured_arg).unwrap();
    assert!(
        captured.contains("--share\nro:workspace=/Users/me/work"),
        "helper did not receive read-only share flag: {captured}"
    );
    assert!(
        captured.contains("--share\ndocs=/Users/me/docs"),
        "helper did not receive writable share flag: {captured}"
    );
    // Old single-share flags are no longer emitted.
    assert!(
        !captured.contains("--share-dir"),
        "helper unexpectedly received --share-dir: {captured}"
    );

    let _ = std::fs::remove_dir_all(&temp);
}

#[cfg(unix)]
#[test]
fn launch_handoff_omits_share_flags_when_no_shares_planned() {
    use std::os::unix::fs::PermissionsExt;

    let temp = std::env::temp_dir().join(format!(
        "lightvm-runner-apple-vz-no-share-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&temp);
    std::fs::create_dir_all(&temp).unwrap();
    let helper = temp.join("helper.sh");
    let captured_arg = temp.join("arg.txt");
    std::fs::write(
        &helper,
        format!(
            "#!/bin/sh\ncat >/dev/null\nprintf '%s\\n' \"$@\" > '{}'\n",
            captured_arg.display()
        ),
    )
    .unwrap();
    let mut permissions = std::fs::metadata(&helper).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&helper, permissions).unwrap();

    // Default spec carries no shares, so no --share flags should reach the helper.
    let spec = launch_spec_with_readiness(true, Vec::new());
    launch_handoff(
        &spec,
        None,
        AppleVzLaunchOptions {
            runner: Some(&helper),
            allow_real_start: true,
            ..AppleVzLaunchOptions::default()
        },
    )
    .expect("helper launch should succeed");

    let captured = std::fs::read_to_string(&captured_arg).unwrap();
    assert!(
        !captured.contains("--share"),
        "helper unexpectedly received a --share flag: {captured}"
    );

    let _ = std::fs::remove_dir_all(&temp);
}
