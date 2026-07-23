//! Split out of main.rs to keep files under 800 lines.

use anyhow::Context;
use anyhow::Result;
use bridgevm_apple_vz::build_fast_plan;
use bridgevm_apple_vz::build_launch_handoff;
use bridgevm_apple_vz::encode_share_flag_value;
use bridgevm_apple_vz::launch_with_apple_vz;
use bridgevm_apple_vz::read_launch_spec_artifact;
use bridgevm_apple_vz::write_launch_spec_artifact;
use bridgevm_apple_vz::AppleVzCommandLauncher;
use bridgevm_apple_vz::AppleVzLaunchHandoff;
use bridgevm_apple_vz::AppleVzLaunchSpec;
use bridgevm_apple_vz::AppleVzReadinessSpec;
use bridgevm_apple_vz::UnsupportedAppleVzLauncher;
use bridgevm_config::VmMode;
use bridgevm_core::VmEngine;
use bridgevm_lightvm::LightVmEngine;
use bridgevm_storage::LaunchReadinessBlockerMetadata;
use bridgevm_storage::LaunchReadinessMetadata;
use bridgevm_storage::RunnerMetadata;
use bridgevm_storage::RuntimeControlMetadata;
use bridgevm_storage::VmRuntimeState;
use bridgevm_storage::VmStore;
use clap::Parser;

#[derive(Debug, Parser)]
#[command(name = "lightvm-runner", about = "BridgeVM Fast Mode runner scaffold")]
pub(crate) struct Args {
    pub(crate) vm: Option<String>,
    #[arg(long, value_name = "PATH")]
    pub(crate) store: Option<std::path::PathBuf>,
    #[arg(long)]
    pub(crate) print_plan: bool,
    #[arg(long)]
    pub(crate) print_handoff: bool,
    #[arg(long)]
    pub(crate) launch: bool,
    #[arg(long, value_name = "PATH")]
    pub(crate) apple_vz_runner: Option<std::path::PathBuf>,
    #[arg(long)]
    pub(crate) apple_vz_allow_real_start: bool,
    #[arg(long, value_name = "SECONDS")]
    pub(crate) apple_vz_stop_after_seconds: Option<u64>,
    #[arg(long, value_name = "SECONDS")]
    pub(crate) apple_vz_force_stop_grace_seconds: Option<u64>,
    #[arg(long, value_name = "PATH")]
    pub(crate) apple_vz_save_state: Option<std::path::PathBuf>,
    #[arg(long, value_name = "PATH")]
    pub(crate) apple_vz_restore_state: Option<std::path::PathBuf>,
    #[arg(long)]
    pub(crate) apple_vz_display: bool,
    #[arg(long, value_name = "PX")]
    pub(crate) apple_vz_display_width: Option<u32>,
    #[arg(long, value_name = "PX")]
    pub(crate) apple_vz_display_height: Option<u32>,
    #[arg(long, value_name = "PATH")]
    pub(crate) apple_vz_runtime_control_socket: Option<std::path::PathBuf>,
    #[arg(long, value_name = "PATH")]
    pub(crate) apple_vz_proxy_framebuffer_rgba_file: Option<std::path::PathBuf>,
    #[arg(long, value_name = "MILLIS")]
    pub(crate) apple_vz_proxy_framebuffer_capture_interval_ms: Option<u64>,
    #[arg(long, value_name = "PATH")]
    pub(crate) launch_spec: Option<std::path::PathBuf>,
    #[arg(long)]
    pub(crate) write_metadata: bool,
    #[arg(long)]
    pub(crate) require_ready: bool,
}

#[derive(Default)]
pub(crate) struct AppleVzLaunchOptions<'a> {
    pub(crate) runner: Option<&'a std::path::Path>,
    pub(crate) allow_real_start: bool,
    pub(crate) stop_after_seconds: Option<u64>,
    pub(crate) force_stop_grace_seconds: Option<u64>,
    pub(crate) save_state: Option<&'a std::path::Path>,
    pub(crate) restore_state: Option<&'a std::path::Path>,
    pub(crate) display: bool,
    pub(crate) display_width: Option<u32>,
    pub(crate) display_height: Option<u32>,
    pub(crate) runtime_control_socket: Option<&'a std::path::Path>,
    pub(crate) proxy_framebuffer_rgba_file: Option<&'a std::path::Path>,
    pub(crate) proxy_framebuffer_capture_interval_ms: Option<u64>,
}

impl<'a> From<&'a Args> for AppleVzLaunchOptions<'a> {
    fn from(args: &'a Args) -> Self {
        Self {
            runner: args.apple_vz_runner.as_deref(),
            allow_real_start: args.apple_vz_allow_real_start,
            stop_after_seconds: args.apple_vz_stop_after_seconds,
            force_stop_grace_seconds: args.apple_vz_force_stop_grace_seconds,
            save_state: args.apple_vz_save_state.as_deref(),
            restore_state: args.apple_vz_restore_state.as_deref(),
            display: args.apple_vz_display,
            display_width: args.apple_vz_display_width,
            display_height: args.apple_vz_display_height,
            runtime_control_socket: args.apple_vz_runtime_control_socket.as_deref(),
            proxy_framebuffer_rgba_file: args.apple_vz_proxy_framebuffer_rgba_file.as_deref(),
            proxy_framebuffer_capture_interval_ms: args
                .apple_vz_proxy_framebuffer_capture_interval_ms,
        }
    }
}

pub(crate) fn run() -> Result<()> {
    let args = Args::parse();
    let engine = LightVmEngine;
    if let Some(path) = args.launch_spec.as_ref() {
        let spec =
            read_launch_spec_artifact(path).context("failed to read Fast Mode launch spec")?;
        if args.require_ready {
            ensure_launch_ready(&spec)?;
        }
        if args.launch {
            launch_handoff(&spec, Some(path), AppleVzLaunchOptions::from(&args))?;
            return Ok(());
        }
        print_launch_spec_or_handoff(&spec, Some(path), args.print_plan, args.print_handoff)?;
        return Ok(());
    }

    let Some(vm) = args.vm.as_deref() else {
        println!("{}", runner_ready_line(engine.name()));
        return Ok(());
    };

    let store = args
        .store
        .clone()
        .map(VmStore::new)
        .unwrap_or_else(VmStore::default);
    let (bundle, mut manifest, _) = store
        .get_vm_with_active_disk(vm)
        .context("failed to read VM")?;
    ensure_fast_mode(manifest.mode)?;

    if args.write_metadata || args.require_ready || args.launch {
        let (_, active_disk) = store
            .prepare_active_disk(vm)
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
            .prepare_active_disk(vm)
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
            .write_runner_metadata(vm, &metadata)
            .context("failed to write runner metadata")?;
    }
    if args.require_ready {
        ensure_launch_ready(plan.launch_spec())?;
    }
    if args.launch {
        record_live_launch_metadata(
            &store,
            vm,
            engine.name(),
            &plan,
            launch_spec_path.clone(),
            args.apple_vz_runtime_control_socket.clone(),
            std::env::args().collect(),
        )?;
        let launch_result = launch_handoff(
            plan.launch_spec(),
            launch_spec_path.as_deref(),
            AppleVzLaunchOptions::from(&args),
        );
        if let Err(error) = launch_result {
            let _ = store.transition_state(vm, VmRuntimeState::Stopped);
            return Err(error);
        }
        store
            .transition_state(vm, VmRuntimeState::Stopped)
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

pub(crate) fn record_live_launch_metadata(
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

pub(crate) fn launch_handoff(
    spec: &AppleVzLaunchSpec,
    launch_spec_path: Option<&std::path::Path>,
    options: AppleVzLaunchOptions<'_>,
) -> Result<()> {
    let handoff = build_launch_handoff(spec, launch_spec_path);
    let attempt = if let Some(program) = options.runner {
        let mut launcher = AppleVzCommandLauncher::new(program);
        if options.allow_real_start {
            launcher = launcher
                .arg("--allow-real-vz-start")
                .env("BRIDGEVM_APPLE_VZ_ALLOW_REAL_START", "1");
        }
        if let Some(seconds) = options.stop_after_seconds {
            launcher = launcher
                .arg("--stop-after-seconds")
                .arg(seconds.to_string());
        }
        if let Some(seconds) = options.force_stop_grace_seconds {
            launcher = launcher
                .arg("--force-stop-grace-seconds")
                .arg(seconds.to_string());
        }
        // Suspend/resume: forward the saved-state path to AppleVzRunner, which
        // maps these to VZ saveMachineState/restoreMachineState.
        if let Some(path) = options.save_state {
            launcher = launcher.arg("--save-state").arg(path.display().to_string());
        }
        if let Some(path) = options.restore_state {
            launcher = launcher
                .arg("--restore-state")
                .arg(path.display().to_string());
        }
        // Embedded display: forward --display so AppleVzRunner boots with a
        // graphics device and hosts the VM in a VZVirtualMachineView window.
        if options.display {
            launcher = launcher.arg("--display");
        }
        if let Some(width) = options.display_width {
            launcher = launcher.arg("--display-width").arg(width.to_string());
        }
        if let Some(height) = options.display_height {
            launcher = launcher.arg("--display-height").arg(height.to_string());
        }
        if let Some(path) = options.runtime_control_socket {
            launcher = launcher
                .arg("--runtime-control-socket")
                .arg(path.display().to_string());
        }
        if let Some(path) = options.proxy_framebuffer_rgba_file {
            launcher = launcher
                .arg("--proxy-framebuffer-rgba-file")
                .arg(path.display().to_string());
        }
        if let Some(interval_ms) = options.proxy_framebuffer_capture_interval_ms {
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

pub(crate) fn runner_ready_line(engine_name: &str) -> String {
    format!("{engine_name} runner ready")
}

pub(crate) fn ensure_fast_mode(mode: VmMode) -> Result<()> {
    if mode != VmMode::Fast {
        anyhow::bail!(
            "lightvm-runner only supports Fast Mode manifests, got {}",
            mode
        );
    }
    Ok(())
}

pub(crate) fn ensure_launch_ready(spec: &AppleVzLaunchSpec) -> Result<()> {
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

pub(crate) fn print_launch_spec_or_words(
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

pub(crate) fn print_launch_spec_or_handoff(
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

pub(crate) fn print_launch_handoff_summary(handoff: &AppleVzLaunchHandoff) {
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

pub(crate) fn launch_readiness_metadata(
    readiness: &AppleVzReadinessSpec,
) -> LaunchReadinessMetadata {
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
