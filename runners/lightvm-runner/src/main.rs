use anyhow::{Context, Result};
use bridgevm_apple_vz::{
    build_fast_plan, build_launch_handoff, launch_with_apple_vz, read_launch_spec_artifact,
    write_launch_spec_artifact, AppleVzCommandLauncher, AppleVzLaunchHandoff, AppleVzLaunchSpec,
    AppleVzReadinessSpec, UnsupportedAppleVzLauncher,
};
use bridgevm_config::VmMode;
use bridgevm_core::VmEngine;
use bridgevm_lightvm::LightVmEngine;
use bridgevm_storage::{
    LaunchReadinessBlockerMetadata, LaunchReadinessMetadata, RunnerMetadata, VmStore,
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
    let (bundle, manifest, _) = store
        .get_vm_with_active_disk(&vm)
        .context("failed to read VM")?;
    ensure_fast_mode(manifest.mode)?;

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
            command: plan.render_runner_words(),
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
        };
        store
            .write_runner_metadata(&vm, &metadata)
            .context("failed to write runner metadata")?;
    }
    if args.require_ready {
        ensure_launch_ready(plan.launch_spec())?;
    }
    if args.launch {
        launch_handoff(
            plan.launch_spec(),
            launch_spec_path.as_deref(),
            args.apple_vz_runner.as_deref(),
            args.apple_vz_allow_real_start,
            args.apple_vz_stop_after_seconds,
            args.apple_vz_force_stop_grace_seconds,
            args.apple_vz_save_state.as_deref(),
            args.apple_vz_restore_state.as_deref(),
            args.apple_vz_display,
        )?;
        return Ok(());
    }
    print_launch_spec_or_words(
        plan.launch_spec(),
        launch_spec_path.as_deref(),
        args.print_plan,
        args.print_handoff,
        plan.render_runner_words(),
    )?;
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
        // Shared folder: forward --share-dir/--share-tag (and --share-read-only)
        // so AppleVzRunner attaches the Virtio-FS VZSingleDirectoryShare. The
        // share comes from the launch spec / handoff (planned in build_fast_plan
        // from the manifest's approved shared folders).
        if let Some(share) = &handoff.share {
            launcher = launcher.arg("--share-dir").arg(share.host_path.clone());
            launcher = launcher.arg("--share-tag").arg(share.tag.clone());
            if share.read_only {
                launcher = launcher.arg("--share-read-only");
            }
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
    use bridgevm_config::BootMode;
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

        let error = launch_handoff(&spec, None, None, false, None, None, None, None, false)
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
        )
        .expect_err("default Apple VZ launcher must remain unimplemented");
        let message = format!("{error:#}");

        assert!(message.contains("failed to launch Apple VZ handoff"));
        assert!(message.contains("not implemented yet"));
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

        launch_handoff(&spec, None, Some(&helper), true, Some(5), Some(2), None, None, false)
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
        launch_handoff(&spec, None, Some(&helper), true, Some(5), None, Some(&state), None, false)
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
        launch_handoff(&spec, None, Some(&helper), true, None, None, None, None, true)
            .expect("helper launch should succeed");

        let captured = std::fs::read_to_string(&captured_arg).unwrap();
        assert!(
            captured.contains("--display"),
            "helper did not receive --display: {captured}"
        );

        let _ = std::fs::remove_dir_all(&temp);
    }

    #[cfg(unix)]
    #[test]
    fn launch_handoff_forwards_share_dir_to_helper() {
        use std::os::unix::fs::PermissionsExt;

        let temp = std::env::temp_dir().join(format!(
            "lightvm-runner-apple-vz-share-dir-{}",
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
        spec.share = Some(AppleVzShareSpec {
            host_path: "/Users/me/work".to_string(),
            tag: "workspace".to_string(),
            read_only: true,
        });
        launch_handoff(&spec, None, Some(&helper), true, None, None, None, None, false)
            .expect("helper launch should succeed");

        let captured = std::fs::read_to_string(&captured_arg).unwrap();
        assert!(
            captured.contains("--share-dir\n/Users/me/work"),
            "helper did not receive --share-dir: {captured}"
        );
        assert!(
            captured.contains("--share-tag\nworkspace"),
            "helper did not receive --share-tag: {captured}"
        );
        assert!(
            captured.contains("--share-read-only"),
            "helper did not receive --share-read-only: {captured}"
        );

        let _ = std::fs::remove_dir_all(&temp);
    }

    #[cfg(unix)]
    #[test]
    fn launch_handoff_omits_share_flags_when_no_share_planned() {
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

        // Default spec carries no share, so no --share-* flags should reach the helper.
        let spec = launch_spec_with_readiness(true, Vec::new());
        launch_handoff(&spec, None, Some(&helper), true, None, None, None, None, false)
            .expect("helper launch should succeed");

        let captured = std::fs::read_to_string(&captured_arg).unwrap();
        assert!(
            !captured.contains("--share-dir"),
            "helper unexpectedly received --share-dir: {captured}"
        );
        assert!(
            !captured.contains("--share-tag"),
            "helper unexpectedly received --share-tag: {captured}"
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
            share: None,
            readiness: AppleVzReadinessSpec { ready, blockers },
        }
    }
}
