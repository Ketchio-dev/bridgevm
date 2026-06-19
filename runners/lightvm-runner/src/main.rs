use anyhow::{Context, Result};
use bridgevm_apple_vz::{
    build_fast_plan, build_launch_handoff, encode_share_flag_value, launch_with_apple_vz,
    read_launch_spec_artifact, write_launch_spec_artifact, AppleVzCommandLauncher,
    AppleVzLaunchHandoff, AppleVzLaunchSpec, AppleVzReadinessSpec, UnsupportedAppleVzLauncher,
};
use bridgevm_config::VmMode;
use bridgevm_core::VmEngine;
use bridgevm_lightvm::LightVmEngine;
use bridgevm_storage::{
    LaunchReadinessBlockerMetadata, LaunchReadinessMetadata, RunnerMetadata,
    RuntimeControlMetadata, VmRuntimeState, VmStore,
};
use clap::Parser;

#[derive(Debug, Parser)]
#[command(name = "lightvm-runner", about = "BridgeVM Fast Mode runner scaffold")]
struct Args {
    vm: Option<String>,
    #[arg(long, value_name = "PATH")]
    store: Option<std::path::PathBuf>,
    #[arg(long)]
    print_plan: bool,
    #[arg(long)]
    print_handoff: bool,
    #[arg(long)]
    launch: bool,
    #[arg(long, value_name = "PATH")]
    apple_vz_runner: Option<std::path::PathBuf>,
    #[arg(long)]
    apple_vz_allow_real_start: bool,
    #[arg(long, value_name = "SECONDS")]
    apple_vz_stop_after_seconds: Option<u64>,
    #[arg(long, value_name = "SECONDS")]
    apple_vz_force_stop_grace_seconds: Option<u64>,
    #[arg(long, value_name = "PATH")]
    apple_vz_save_state: Option<std::path::PathBuf>,
    #[arg(long, value_name = "PATH")]
    apple_vz_restore_state: Option<std::path::PathBuf>,
    #[arg(long)]
    apple_vz_display: bool,
    #[arg(long, value_name = "PX")]
    apple_vz_display_width: Option<u32>,
    #[arg(long, value_name = "PX")]
    apple_vz_display_height: Option<u32>,
    #[arg(long, value_name = "PATH")]
    apple_vz_runtime_control_socket: Option<std::path::PathBuf>,
    #[arg(long, value_name = "PATH")]
    apple_vz_proxy_framebuffer_rgba_file: Option<std::path::PathBuf>,
    #[arg(long, value_name = "MILLIS")]
    apple_vz_proxy_framebuffer_capture_interval_ms: Option<u64>,
    #[arg(long, value_name = "PATH")]
    launch_spec: Option<std::path::PathBuf>,
    #[arg(long)]
    write_metadata: bool,
    #[arg(long)]
    require_ready: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let engine = LightVmEngine;
    if let Some(path) = args.launch_spec.as_ref() {
        let spec =
            read_launch_spec_artifact(path).context("failed to read Fast Mode launch spec")?;
        if args.require_ready {
            ensure_launch_ready(&spec)?;
        }
        if args.launch {
            launch_handoff(
                &spec,
                Some(path),
                args.apple_vz_runner.as_deref(),
                args.apple_vz_allow_real_start,
                args.apple_vz_stop_after_seconds,
                args.apple_vz_force_stop_grace_seconds,
                args.apple_vz_save_state.as_deref(),
                args.apple_vz_restore_state.as_deref(),
                args.apple_vz_display,
                args.apple_vz_display_width,
                args.apple_vz_display_height,
                args.apple_vz_runtime_control_socket.as_deref(),
                args.apple_vz_proxy_framebuffer_rgba_file.as_deref(),
                args.apple_vz_proxy_framebuffer_capture_interval_ms,
            )?;
            return Ok(());
        }
        print_launch_spec_or_handoff(&spec, Some(path), args.print_plan, args.print_handoff)?;
        return Ok(());
    }

    let Some(vm) = args.vm else {
        println!("{}", runner_ready_line(engine.name()));
        return Ok(());
    };

    let store = args
        .store
        .map(VmStore::new)
        .unwrap_or_else(VmStore::default);
    let (bundle, mut manifest, _) = store
        .get_vm_with_active_disk(&vm)
        .context("failed to read VM")?;
    ensure_fast_mode(manifest.mode)?;

    if args.write_metadata || args.require_ready || args.launch {
        let (_, active_disk) = store
            .prepare_active_disk(&vm)
            .context("failed to prepare active disk")?;
        manifest.storage.primary.path = active_disk.path.display().to_string();
        manifest.storage.primary.format = active_disk.format;
    }

    let plan = build_fast_plan(&manifest, &bundle).context("failed to build Apple VZ plan")?;
    let launch_spec_path = if args.write_metadata || args.require_ready || args.launch {
        Some(
            write_launch_spec_artifact(&bundle, plan.launch_spec())
                .context("failed to write Fast Mode launch spec")?,
        )
    } else {
        None
    };

    if args.write_metadata {
        let (disk, active_disk) = store
            .prepare_active_disk(&vm)
            .context("failed to prepare active disk")?;
        let metadata = RunnerMetadata {
            engine: engine.name().to_string(),
            pid: None,
            command: plan.render_runner_words_for_launch_spec(
                launch_spec_path
                    .as_deref()
                    .expect("--write-metadata writes a launch spec before runner metadata"),
            ),
            log_path: plan.launch_spec().logs.runner_log_path.clone().into(),
            started_at_unix: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            dry_run: true,
            launch_spec_path: launch_spec_path.clone(),
            guest_tools: None,
            disk: Some(disk),
            active_disk: Some(active_disk),
            launch_readiness: Some(launch_readiness_metadata(&plan.launch_spec().readiness)),
            runtime_control: None,
        };
        store
            .write_runner_metadata(&vm, &metadata)
            .context("failed to write runner metadata")?;
    }
    if args.require_ready {
        ensure_launch_ready(plan.launch_spec())?;
    }
    if args.launch {
        record_live_launch_metadata(
            &store,
            &vm,
            engine.name(),
            &plan,
            launch_spec_path.clone(),
            args.apple_vz_runtime_control_socket.clone(),
            std::env::args().collect(),
        )?;
        let launch_result = launch_handoff(
            plan.launch_spec(),
            launch_spec_path.as_deref(),
            args.apple_vz_runner.as_deref(),
            args.apple_vz_allow_real_start,
            args.apple_vz_stop_after_seconds,
            args.apple_vz_force_stop_grace_seconds,
            args.apple_vz_save_state.as_deref(),
            args.apple_vz_restore_state.as_deref(),
            args.apple_vz_display,
            args.apple_vz_display_width,
            args.apple_vz_display_height,
            args.apple_vz_runtime_control_socket.as_deref(),
            args.apple_vz_proxy_framebuffer_rgba_file.as_deref(),
            args.apple_vz_proxy_framebuffer_capture_interval_ms,
        );
        if let Err(error) = launch_result {
            let _ = store.transition_state(&vm, VmRuntimeState::Stopped);
            return Err(error);
        }
        store
            .transition_state(&vm, VmRuntimeState::Stopped)
            .context("failed to record VM stopped after Fast launch exit")?;
        return Ok(());
    }
    print_launch_spec_or_words(
        plan.launch_spec(),
        launch_spec_path.as_deref(),
        args.print_plan,
        args.print_handoff,
        launch_spec_path
            .as_deref()
            .map(|path| plan.render_runner_words_for_launch_spec(path))
            .unwrap_or_else(|| plan.render_runner_words()),
    )?;
    Ok(())
}

fn record_live_launch_metadata(
    store: &VmStore,
    vm: &str,
    engine_name: &str,
    plan: &bridgevm_apple_vz::AppleVzPlan,
    launch_spec_path: Option<std::path::PathBuf>,
    runtime_control_socket: Option<std::path::PathBuf>,
    command: Vec<String>,
) -> Result<()> {
    let (disk, active_disk) = store
        .prepare_active_disk(vm)
        .context("failed to prepare active disk for live Fast launch metadata")?;
    let runtime_control = runtime_control_socket.map(|socket_path| RuntimeControlMetadata {
        kind: "apple-vz-display".to_string(),
        socket_path,
        commands: vec![
            "status".to_string(),
            "stop".to_string(),
            "policy".to_string(),
            "pacing".to_string(),
        ],
    });
    let metadata = RunnerMetadata {
        engine: engine_name.to_string(),
        pid: Some(std::process::id()),
        command,
        log_path: plan.launch_spec().logs.runner_log_path.clone().into(),
        started_at_unix: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        dry_run: false,
        launch_spec_path,
        guest_tools: None,
        disk: Some(disk),
        active_disk: Some(active_disk),
        launch_readiness: Some(launch_readiness_metadata(&plan.launch_spec().readiness)),
        runtime_control,
    };
    store
        .write_runner_metadata(vm, &metadata)
        .context("failed to write live Fast launch runner metadata")?;
    store
        .transition_state(vm, VmRuntimeState::Running)
        .context("failed to record VM running for live Fast launch")?;
    Ok(())
}

fn launch_handoff(
    spec: &AppleVzLaunchSpec,
    launch_spec_path: Option<&std::path::Path>,
    apple_vz_runner: Option<&std::path::Path>,
    apple_vz_allow_real_start: bool,
    apple_vz_stop_after_seconds: Option<u64>,
    apple_vz_force_stop_grace_seconds: Option<u64>,
    apple_vz_save_state: Option<&std::path::Path>,
    apple_vz_restore_state: Option<&std::path::Path>,
    apple_vz_display: bool,
    apple_vz_display_width: Option<u32>,
    apple_vz_display_height: Option<u32>,
    apple_vz_runtime_control_socket: Option<&std::path::Path>,
    apple_vz_proxy_framebuffer_rgba_file: Option<&std::path::Path>,
    apple_vz_proxy_framebuffer_capture_interval_ms: Option<u64>,
) -> Result<()> {
    let handoff = build_launch_handoff(spec, launch_spec_path);
    let attempt = if let Some(program) = apple_vz_runner {
        let mut launcher = AppleVzCommandLauncher::new(program);
        if apple_vz_allow_real_start {
            launcher = launcher
                .arg("--allow-real-vz-start")
                .env("BRIDGEVM_APPLE_VZ_ALLOW_REAL_START", "1");
        }
        if let Some(seconds) = apple_vz_stop_after_seconds {
            launcher = launcher
                .arg("--stop-after-seconds")
                .arg(seconds.to_string());
        }
        if let Some(seconds) = apple_vz_force_stop_grace_seconds {
            launcher = launcher
                .arg("--force-stop-grace-seconds")
                .arg(seconds.to_string());
        }
        // Suspend/resume: forward the saved-state path to AppleVzRunner, which
        // maps these to VZ saveMachineState/restoreMachineState.
        if let Some(path) = apple_vz_save_state {
            launcher = launcher.arg("--save-state").arg(path.display().to_string());
        }
        if let Some(path) = apple_vz_restore_state {
            launcher = launcher
                .arg("--restore-state")
                .arg(path.display().to_string());
        }
        // Embedded display: forward --display so AppleVzRunner boots with a
        // graphics device and hosts the VM in a VZVirtualMachineView window.
        if apple_vz_display {
            launcher = launcher.arg("--display");
        }
        if let Some(width) = apple_vz_display_width {
            launcher = launcher.arg("--display-width").arg(width.to_string());
        }
        if let Some(height) = apple_vz_display_height {
            launcher = launcher.arg("--display-height").arg(height.to_string());
        }
        if let Some(path) = apple_vz_runtime_control_socket {
            launcher = launcher
                .arg("--runtime-control-socket")
                .arg(path.display().to_string());
        }
        if let Some(path) = apple_vz_proxy_framebuffer_rgba_file {
            launcher = launcher
                .arg("--proxy-framebuffer-rgba-file")
                .arg(path.display().to_string());
        }
        if let Some(interval_ms) = apple_vz_proxy_framebuffer_capture_interval_ms {
            launcher = launcher
                .arg("--proxy-framebuffer-capture-interval-ms")
                .arg(interval_ms.to_string());
        }
        // Shared folders: forward one repeatable `--share <tag>=<host_path>`
        // (optionally `ro:`-prefixed) flag per approved folder so AppleVzRunner
        // attaches them all over Virtio-FS (a single VZSingleDirectoryShare, or a
        // VZMultipleDirectoryShare for 2+). The shares come from the launch spec /
        // handoff (planned in build_fast_plan from the manifest's approved folders).
        for share in &handoff.shares {
            launcher = launcher.arg("--share").arg(encode_share_flag_value(share));
        }
        launch_with_apple_vz(&launcher, handoff)
    } else {
        launch_with_apple_vz(&UnsupportedAppleVzLauncher, handoff)
    }
    .context("failed to launch Apple VZ handoff")?;
    if !attempt.stdout.is_empty() {
        println!("{}", attempt.stdout);
    }
    if !attempt.stderr.is_empty() {
        eprintln!("{}", attempt.stderr);
    }
    println!("Backend: {}", attempt.backend);
    println!("VM: {}", attempt.vm_name);
    Ok(())
}

fn runner_ready_line(engine_name: &str) -> String {
    format!("{engine_name} runner ready")
}

fn ensure_fast_mode(mode: VmMode) -> Result<()> {
    if mode != VmMode::Fast {
        anyhow::bail!(
            "lightvm-runner only supports Fast Mode manifests, got {}",
            mode
        );
    }
    Ok(())
}

fn ensure_launch_ready(spec: &AppleVzLaunchSpec) -> Result<()> {
    if spec.readiness.ready {
        return Ok(());
    }
    let blockers = spec
        .readiness
        .blockers
        .iter()
        .map(|blocker| match &blocker.path {
            Some(path) => format!("{}: {} ({path})", blocker.code, blocker.message),
            None => format!("{}: {}", blocker.code, blocker.message),
        })
        .collect::<Vec<_>>()
        .join("; ");
    anyhow::bail!("Fast Mode launch readiness failed: {blockers}");
}

fn print_launch_spec_or_words(
    spec: &AppleVzLaunchSpec,
    launch_spec_path: Option<&std::path::Path>,
    print_plan: bool,
    print_handoff: bool,
    runner_words: Vec<String>,
) -> Result<()> {
    if print_plan || print_handoff {
        print_launch_spec_or_handoff(spec, launch_spec_path, print_plan, print_handoff)
    } else {
        for word in runner_words {
            println!("{word}");
        }
        Ok(())
    }
}

fn print_launch_spec_or_handoff(
    spec: &AppleVzLaunchSpec,
    launch_spec_path: Option<&std::path::Path>,
    print_plan: bool,
    print_handoff: bool,
) -> Result<()> {
    if print_plan {
        println!("{}", serde_json::to_string_pretty(spec)?);
    } else if print_handoff {
        let handoff = build_launch_handoff(spec, launch_spec_path);
        println!("{}", serde_json::to_string_pretty(&handoff)?);
    } else {
        print_launch_handoff_summary(&build_launch_handoff(spec, launch_spec_path));
    }
    Ok(())
}

fn print_launch_handoff_summary(handoff: &AppleVzLaunchHandoff) {
    println!("Backend: {}", handoff.backend);
    println!("VM: {}", handoff.vm_name);
    if let Some(path) = &handoff.launch_spec_path {
        println!("Launch spec: {path}");
    }
    println!("Boot mode: {}", handoff.boot_mode);
    println!("Disk: {} ({})", handoff.disk.path, handoff.disk.format);
    println!("Runner log: {}", handoff.runner_log_path);
    println!("Launch ready: {}", handoff.readiness.ready);
    if !handoff.readiness.blockers.is_empty() {
        println!("Launch blockers:");
        for blocker in &handoff.readiness.blockers {
            match (&blocker.path, &blocker.capability) {
                (Some(path), _) => println!("- {}: {} ({path})", blocker.code, blocker.message),
                (None, Some(capability)) => {
                    println!("- {}: {} ({capability})", blocker.code, blocker.message)
                }
                (None, None) => println!("- {}: {}", blocker.code, blocker.message),
            }
        }
    }
}

fn launch_readiness_metadata(readiness: &AppleVzReadinessSpec) -> LaunchReadinessMetadata {
    LaunchReadinessMetadata {
        ready: readiness.ready,
        blockers: readiness
            .blockers
            .iter()
            .map(|blocker| LaunchReadinessBlockerMetadata {
                code: blocker.code.clone(),
                message: blocker.message.clone(),
                path: blocker.path.as_ref().map(Into::into),
                capability: blocker.capability.clone(),
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bridgevm_apple_vz::{
        AppleVzBootSpec, AppleVzDeviceSpec, AppleVzDiskSpec, AppleVzGuestSpec,
        AppleVzIntegrationSpec, AppleVzLogSpec, AppleVzPathSpec, AppleVzReadinessBlocker,
        AppleVzResourceSpec, AppleVzShareSpec,
    };
    use bridgevm_config::{BootMode, Guest, VmManifest};
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

        let error = launch_handoff(
            &spec, None, None, false, None, None, None, None, false, None, None, None, None, None,
        )
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
            None,
            false,
            None,
            None,
            None,
            None,
            false,
            None,
            None,
            None,
            None,
            None,
        )
        .expect_err("default Apple VZ launcher must require the Swift helper");
        let message = format!("{error:#}");

        assert!(message.contains("failed to launch Apple VZ handoff"));
        assert!(message.contains("--apple-vz-runner"));
        assert!(message.contains("signed AppleVzRunner"));
    }

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
            Some(&helper),
            true,
            Some(5),
            Some(2),
            None,
            None,
            false,
            None,
            None,
            None,
            None,
            None,
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
            Some(&helper),
            true,
            Some(5),
            None,
            Some(&state),
            None,
            false,
            None,
            None,
            None,
            None,
            None,
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
            Some(&helper),
            true,
            None,
            None,
            None,
            None,
            true,
            Some(1440),
            Some(900),
            None,
            Some(&framebuffer),
            Some(250),
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
            Some(&helper),
            true,
            None,
            None,
            None,
            None,
            false,
            None,
            None,
            None,
            None,
            None,
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
            Some(&helper),
            true,
            None,
            None,
            None,
            None,
            false,
            None,
            None,
            None,
            None,
            None,
        )
        .expect("helper launch should succeed");

        let captured = std::fs::read_to_string(&captured_arg).unwrap();
        assert!(
            !captured.contains("--share"),
            "helper unexpectedly received a --share flag: {captured}"
        );

        let _ = std::fs::remove_dir_all(&temp);
    }

    fn launch_spec_with_readiness(
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
}
