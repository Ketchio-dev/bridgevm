//! Tests split so no file exceeds 1000 lines.

use crate::test_support::*;

#[test]
fn runtime_control_reader_accepts_fragmented_response() {
    let socket_path = unique_runtime_control_test_socket("fragmented");
    let listener = std::os::unix::net::UnixListener::bind(&socket_path).unwrap();
    let server = std::thread::spawn({
        let socket_path = socket_path.clone();
        move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = String::new();
            BufReader::new(stream.try_clone().unwrap())
                .read_line(&mut request)
                .unwrap();
            stream.write_all(br#"{"ok":true,"state":"run"#).unwrap();
            std::thread::sleep(Duration::from_millis(10));
            stream.write_all(b"ning\"}\n").unwrap();
            drop(stream);
            let _ = fs::remove_file(socket_path);
        }
    });

    let response = send_runtime_control_command(&socket_path, "status").unwrap();
    assert_eq!(
        response.get("state").and_then(serde_json::Value::as_str),
        Some("running")
    );
    server.join().unwrap();
}

#[test]
fn display_runtime_policy_uses_foreground_visibility() {
    let _battery = EnvVarGuard::set("BRIDGEVM_FORCE_ON_BATTERY", "0");
    let mut manifest = VmManifest::new(
        "fast-display",
        VmMode::Fast,
        Guest {
            os: "ubuntu".to_string(),
            version: None,
            arch: "arm64".to_string(),
        },
        "80GiB",
    );
    apply_power_aware_fast_resources(&mut manifest);

    let policy = build_runtime_resource_policy_metadata(
        "fast-display",
        &manifest,
        RuntimeResourceVisibility::Foreground,
        VmRuntimeState::Running,
    );

    assert_eq!(policy.visibility, RuntimeResourceVisibility::Foreground);
    assert_eq!(policy.state, VmRuntimeState::Running);
    assert!(!policy.on_battery);
    assert_eq!(policy.memory, "4096");
    assert_eq!(policy.cpu, "2");
    assert_eq!(policy.display_fps_cap, "adaptive");
    assert!(!policy.live_applied);
    assert_eq!(
        policy.live_apply_blockers[0].code,
        "runtime-control-unavailable"
    );
}

#[test]
fn fast_runner_args_cold_start_omits_restore_state() {
    let launch_spec = Path::new("/bundle/metadata/launch-spec.json");
    let runner = Path::new("/helpers/AppleVzRunner");
    let args = fast_runner_args(FastRunnerConfig::new(launch_spec, runner));
    assert_eq!(
        args,
        vec![
            "--launch-spec".to_string(),
            "/bundle/metadata/launch-spec.json".to_string(),
            "--require-ready".to_string(),
            "--launch".to_string(),
            "--apple-vz-runner".to_string(),
            "/helpers/AppleVzRunner".to_string(),
            "--apple-vz-allow-real-start".to_string(),
        ]
    );
    // A cold start never restores saved state.
    assert!(!args.iter().any(|arg| arg == "--apple-vz-restore-state"));
    // and a non-display cold start does not request a display window.
    assert!(!args.iter().any(|arg| arg == "--apple-vz-display"));
}

#[test]
fn fast_runner_args_display_appends_display_flag() {
    let launch_spec = Path::new("/bundle/metadata/launch-spec.json");
    let runner = Path::new("/helpers/AppleVzRunner");
    let mut config = FastRunnerConfig::new(launch_spec, runner);
    config.display = Some(FastRunnerDisplayConfig {
        size: None,
        runtime_control_socket: None,
        proxy_framebuffer_rgba_file: None,
        proxy_framebuffer_capture_interval_ms: None,
    });
    let args = fast_runner_args(config);
    assert!(args.iter().any(|arg| arg == "--apple-vz-display"));
    assert!(!args.iter().any(|arg| arg == "--apple-vz-restore-state"));
}

#[test]
fn fast_runner_args_display_appends_display_dimensions() {
    let launch_spec = Path::new("/bundle/metadata/launch-spec.json");
    let runner = Path::new("/helpers/AppleVzRunner");
    let args = fast_runner_args(FastRunnerConfig {
        launch_spec_path: launch_spec,
        apple_vz_runner: runner,
        restore_state: None,
        display: Some(FastRunnerDisplayConfig {
            size: Some((1440, 900)),
            runtime_control_socket: None,
            proxy_framebuffer_rgba_file: None,
            proxy_framebuffer_capture_interval_ms: None,
        }),
    });
    assert_eq!(
        args,
        vec![
            "--launch-spec".to_string(),
            "/bundle/metadata/launch-spec.json".to_string(),
            "--require-ready".to_string(),
            "--launch".to_string(),
            "--apple-vz-runner".to_string(),
            "/helpers/AppleVzRunner".to_string(),
            "--apple-vz-allow-real-start".to_string(),
            "--apple-vz-display".to_string(),
            "--apple-vz-display-width".to_string(),
            "1440".to_string(),
            "--apple-vz-display-height".to_string(),
            "900".to_string(),
        ]
    );
}

#[test]
fn fast_runner_args_display_appends_runtime_control_socket() {
    let launch_spec = Path::new("/bundle/metadata/launch-spec.json");
    let runner = Path::new("/helpers/AppleVzRunner");
    let socket = Path::new("/bundle/run/apple-vz-display-control.sock");
    let args = fast_runner_args(FastRunnerConfig {
        launch_spec_path: launch_spec,
        apple_vz_runner: runner,
        restore_state: None,
        display: Some(FastRunnerDisplayConfig {
            size: None,
            runtime_control_socket: Some(socket),
            proxy_framebuffer_rgba_file: None,
            proxy_framebuffer_capture_interval_ms: None,
        }),
    });

    assert!(args.iter().any(|arg| arg == "--apple-vz-display"));
    assert!(args.windows(2).any(|pair| pair
        == [
            "--apple-vz-runtime-control-socket",
            socket.to_str().unwrap()
        ]));
}

#[test]
fn fast_runner_args_display_appends_proxy_framebuffer_export() {
    let launch_spec = Path::new("/bundle/metadata/launch-spec.json");
    let runner = Path::new("/helpers/AppleVzRunner");
    let framebuffer = Path::new("/bundle/metadata/apple-vz-display-framebuffer.rgba");
    let args = fast_runner_args(FastRunnerConfig {
        launch_spec_path: launch_spec,
        apple_vz_runner: runner,
        restore_state: None,
        display: Some(FastRunnerDisplayConfig {
            size: None,
            runtime_control_socket: None,
            proxy_framebuffer_rgba_file: Some(framebuffer),
            proxy_framebuffer_capture_interval_ms: Some(250),
        }),
    });

    assert!(args.iter().any(|arg| arg == "--apple-vz-display"));
    assert!(args.windows(2).any(|pair| pair
        == [
            "--apple-vz-proxy-framebuffer-rgba-file",
            framebuffer.to_str().unwrap()
        ]));
    assert!(args
        .windows(2)
        .any(|pair| pair == ["--apple-vz-proxy-framebuffer-capture-interval-ms", "250"]));
}

#[test]
fn apple_vz_display_control_socket_path_stays_short_for_macos_unix_sockets() {
    let bundle = Path::new("/Users/example/.bridgevm/vms/runtime-resources-fast.vmbridge");
    let socket = apple_vz_display_control_socket_path(bundle);

    assert_eq!(socket, PathBuf::from("/tmp/bvm-vz-50f391db705184f1.sock"));
    assert!(socket.to_string_lossy().len() < 104);
}

#[test]
fn fast_runner_args_resume_appends_restore_state() {
    let launch_spec = Path::new("/bundle/metadata/launch-spec.json");
    let runner = Path::new("/helpers/AppleVzRunner");
    let state = Path::new("/bundle/metadata/suspend-images/fast.bin");
    let mut config = FastRunnerConfig::new(launch_spec, runner);
    config.restore_state = Some(state);
    let args = fast_runner_args(config);
    assert_eq!(
        args,
        vec![
            "--launch-spec".to_string(),
            "/bundle/metadata/launch-spec.json".to_string(),
            "--require-ready".to_string(),
            "--launch".to_string(),
            "--apple-vz-runner".to_string(),
            "/helpers/AppleVzRunner".to_string(),
            "--apple-vz-allow-real-start".to_string(),
            "--apple-vz-restore-state".to_string(),
            "/bundle/metadata/suspend-images/fast.bin".to_string(),
        ]
    );
}

#[cfg(unix)]
#[test]
fn display_fast_backend_spawns_detached_runner_that_survives_return() {
    if !(cfg!(target_os = "macos") && cfg!(target_arch = "aarch64")) {
        return;
    }

    let _guard = APPLE_VZ_RUNNER_ENV_LOCK.lock().unwrap();
    let (store, name) = fast_test_store("display-spawn-detached");
    let bundle = stage_ready_fast_linux_kernel_vm(&store, &name);
    let helper_dir = store.root().join("helpers");
    std::fs::create_dir_all(&helper_dir).unwrap();
    let fake_lightvm_runner = helper_dir.join("lightvm-runner");
    let fake_apple_vz_runner = helper_dir.join("AppleVzRunner");
    let args_file = store.root().join("fake-lightvm-args.txt");
    let env_file = store.root().join("fake-lightvm-env.txt");

    write_executable(&fake_apple_vz_runner, "#!/bin/sh\nexit 0\n");
    write_executable(
        &fake_lightvm_runner,
        r#"#!/bin/sh
printf '%s\n' "$@" > "$BRIDGEVM_FAKE_RUNNER_ARGS"
printf '%s\n' "$BRIDGEVM_APPLE_VZ_ALLOW_REAL_START" > "$BRIDGEVM_FAKE_RUNNER_ENV"
exec sleep 60
"#,
    );

    let _apple_runner_env = EnvVarGuard::set(
        "BRIDGEVM_APPLE_VZ_RUNNER",
        fake_apple_vz_runner.to_str().unwrap(),
    );
    let _lightvm_runner_env = EnvVarGuard::set(
        "BRIDGEVM_LIGHTVM_RUNNER",
        fake_lightvm_runner.to_str().unwrap(),
    );
    let _fake_args_env = EnvVarGuard::set("BRIDGEVM_FAKE_RUNNER_ARGS", args_file.to_str().unwrap());
    let _fake_env_env = EnvVarGuard::set("BRIDGEVM_FAKE_RUNNER_ENV", env_file.to_str().unwrap());

    let metadata = display_fast_backend_with_size(&store, &name, Some((1440, 900))).unwrap();
    let pid = metadata
        .pid
        .expect("display spawn records the detached runner pid");
    for _ in 0..200 {
        if args_file.exists() && process_is_alive(pid) {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }

    assert!(
        process_is_alive(pid),
        "display runner pid {pid} should survive API return"
    );
    assert_eq!(
        process_group_id(pid),
        Some(pid),
        "display runner should launch in its own process group"
    );
    assert!(metadata
        .command
        .iter()
        .any(|arg| arg == "--apple-vz-display"));
    assert!(metadata
        .command
        .windows(2)
        .any(|pair| pair == ["--apple-vz-display-width", "1440"]));
    assert!(metadata
        .command
        .windows(2)
        .any(|pair| pair == ["--apple-vz-display-height", "900"]));
    let runtime_control = metadata
        .runtime_control
        .as_ref()
        .expect("display spawn records runtime-control metadata");
    assert_eq!(
        runtime_control.socket_path,
        apple_vz_display_control_socket_path(&bundle)
    );

    let args = std::fs::read_to_string(&args_file).unwrap();
    assert!(args.contains("--apple-vz-display\n"), "{args}");
    assert!(args.contains("--apple-vz-display-width\n1440\n"), "{args}");
    assert!(args.contains("--apple-vz-display-height\n900\n"), "{args}");
    assert!(
        args.contains(
            apple_vz_display_framebuffer_rgba_path(&bundle)
                .to_str()
                .unwrap()
        ),
        "{args}"
    );
    assert_eq!(std::fs::read_to_string(&env_file).unwrap().trim(), "1");

    let _ = signal_process(pid, "TERM");
    let _ = std::fs::remove_dir_all(store.root());
}

#[test]
fn apple_vz_runner_configured_reflects_env() {
    let _guard = APPLE_VZ_RUNNER_ENV_LOCK.lock().unwrap();
    let _env = EnvVarGuard::capture("BRIDGEVM_APPLE_VZ_RUNNER");

    std::env::remove_var("BRIDGEVM_APPLE_VZ_RUNNER");
    assert!(!apple_vz_runner_configured());

    std::env::set_var("BRIDGEVM_APPLE_VZ_RUNNER", "/helpers/AppleVzRunner");
    assert!(apple_vz_runner_configured());

    // An empty value does not count as configured.
    std::env::set_var("BRIDGEVM_APPLE_VZ_RUNNER", "");
    assert!(!apple_vz_runner_configured());
}

#[test]
fn parse_ps_etime_handles_all_field_widths() {
    assert_eq!(parse_ps_etime("00:05"), Some(5));
    assert_eq!(parse_ps_etime("  01:30 "), Some(90));
    assert_eq!(parse_ps_etime("01:02:03"), Some(3723));
    assert_eq!(
        parse_ps_etime("2-03:04:05"),
        Some(2 * 86_400 + 3 * 3_600 + 4 * 60 + 5)
    );
    assert_eq!(parse_ps_etime("garbage"), None);
    assert_eq!(parse_ps_etime(""), None);
}

#[cfg(unix)]
#[test]
fn process_elapsed_time_query_is_bounded_and_parses_current_process() {
    let output = query_process_elapsed_time(std::process::id()).unwrap();

    assert!(output.status.success());
    assert!(parse_ps_etime(&String::from_utf8_lossy(&output.stdout)).is_some());
    assert!(output.stdout.len() <= 4096);
}

#[cfg(unix)]
#[test]
fn process_signal_rejects_non_allowlisted_signal() {
    let error = signal_process(std::process::id(), "STOP").unwrap_err();

    assert_eq!(error, "unsupported process signal: STOP");
    assert!(process_is_alive(std::process::id()));
}

#[cfg(unix)]
#[test]
fn bounded_process_group_wait_terminates_and_reaps_timeout() {
    let mut command = Command::new("/bin/sh");
    command
        .args(["-c", "sleep 5"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0);
    let mut child = command.spawn().unwrap();

    let error = wait_process_group_bounded(
        &mut child,
        Duration::from_millis(20),
        Duration::from_millis(20),
        "test runner",
    )
    .unwrap_err();

    assert!(error.contains("test runner timed out"));
    assert!(child.try_wait().unwrap().is_some());
}

#[cfg(unix)]
#[test]
fn terminate_recorded_process_kills_live_child() {
    let pid = spawn_detached_sleep();
    assert!(process_is_alive(pid));

    let outcome = terminate_recorded_process(
        pid,
        now_unix(),
        Duration::from_secs(STOP_TERMINATION_GRACE_SECONDS),
    )
    .unwrap();
    // Release gate: the process is terminated. `sleep` normally exits on
    // SIGTERM (ExitedAfterTerm), but a reparented-to-init process can be
    // reaped slightly after the grace window, in which case the SIGKILL
    // fallback (Killed) takes over. Either is a successful termination; what
    // matters is that no process remains. AlreadyGone would mean we never
    // observed it live, which this test rules out via the assert above.
    assert_ne!(outcome, ProcessTerminationOutcome::AlreadyGone);
    // Poll briefly: init/launchd reaps the reparented process asynchronously.
    let mut gone = false;
    for _ in 0..200 {
        if !process_is_alive(pid) {
            gone = true;
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    assert!(gone, "process {pid} should be gone after termination");
}

#[cfg(unix)]
#[test]
fn terminate_recorded_process_is_noop_for_dead_pid() {
    let pid = spawn_detached_sleep();
    signal_process(pid, "KILL").unwrap();
    // Wait for the detached process to fully exit (init reaps it).
    for _ in 0..200 {
        if !process_is_alive(pid) {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    let outcome = terminate_recorded_process(
        pid,
        now_unix(),
        Duration::from_secs(STOP_TERMINATION_GRACE_SECONDS),
    )
    .unwrap();
    assert_eq!(outcome, ProcessTerminationOutcome::AlreadyGone);
}

#[cfg(unix)]
#[test]
fn stop_backend_terminates_recorded_child_process() {
    let store = VmStore::new(unique_test_root("stop-kills-child"));
    let manifest = VmManifest::new(
        "fast-linux",
        VmMode::Fast,
        Guest {
            os: "ubuntu".to_string(),
            version: None,
            arch: "arm64".to_string(),
        },
        "80GiB",
    );
    handle_request(&store, BridgeVmRequest::create_vm(manifest))
        .into_result()
        .unwrap();
    store
        .transition_state("fast-linux", VmRuntimeState::Running)
        .unwrap();

    let pid = spawn_detached_sleep();
    let bundle = store.bundle_path("fast-linux");
    let runner = RunnerMetadata {
        engine: "lightvm".to_string(),
        pid: Some(pid),
        command: vec!["lightvm-runner".to_string()],
        log_path: bundle.join("logs").join("runner.log"),
        started_at_unix: now_unix(),
        dry_run: false,
        launch_spec_path: None,
        guest_tools: None,
        disk: None,
        active_disk: None,
        launch_readiness: None,
        runtime_control: None,
    };
    store.write_runner_metadata("fast-linux", &runner).unwrap();

    let result = stop_backend(&store, "fast-linux").unwrap();
    assert!(result.is_none());
    // Release gate: no VM process remains after stop.
    assert!(!process_is_alive(pid));
    // State cleared.
    assert_eq!(
        store.state("fast-linux").unwrap().state,
        VmRuntimeState::Stopped
    );
    assert!(store.runner_metadata("fast-linux").unwrap().is_none());
    let _ = std::fs::remove_dir_all(store.root());
}

#[test]
fn stop_backend_leaves_dry_run_vm_as_metadata_only() {
    let store = VmStore::new(unique_test_root("stop-dry-run"));
    let manifest = VmManifest::new(
        "fast-linux",
        VmMode::Fast,
        Guest {
            os: "ubuntu".to_string(),
            version: None,
            arch: "arm64".to_string(),
        },
        "80GiB",
    );
    handle_request(&store, BridgeVmRequest::create_vm(manifest))
        .into_result()
        .unwrap();
    store
        .transition_state("fast-linux", VmRuntimeState::Running)
        .unwrap();

    let bundle = store.bundle_path("fast-linux");
    let runner = RunnerMetadata {
        engine: "lightvm".to_string(),
        pid: None,
        command: vec!["lightvm-runner".to_string()],
        log_path: bundle.join("logs").join("runner.log"),
        started_at_unix: now_unix(),
        dry_run: true,
        launch_spec_path: None,
        guest_tools: None,
        disk: None,
        active_disk: None,
        launch_readiness: None,
        runtime_control: None,
    };
    store.write_runner_metadata("fast-linux", &runner).unwrap();

    // No real pid -> no termination attempted; metadata-only stop succeeds.
    let result = stop_backend(&store, "fast-linux").unwrap();
    assert!(result.is_none());
    assert_eq!(
        store.state("fast-linux").unwrap().state,
        VmRuntimeState::Stopped
    );
    assert!(store.runner_metadata("fast-linux").unwrap().is_none());
    let _ = std::fs::remove_dir_all(store.root());
}

#[test]
fn compatibility_resume_command_appends_loadvm_tag() {
    let manifest = VmManifest::new(
        "compat",
        VmMode::Compatibility,
        Guest {
            os: "ubuntu".to_string(),
            version: None,
            arch: "x86_64".to_string(),
        },
        "40GiB",
    );
    let bundle = unique_test_root("compat-resume-cmd");
    let command = build_compatibility_resume_command(&manifest, &bundle).unwrap();
    // The last two args must be `-loadvm <tag>`.
    let tail = &command.args[command.args.len() - 2..];
    assert_eq!(
        tail,
        &["-loadvm".to_string(), "bridgevm-suspend".to_string()]
    );
}

#[test]
fn compatibility_resume_load_failure_reports_preserved_snapshot() {
    let bundle = unique_test_root("compat-resume-load-failure");
    let log_path = bundle.join("logs").join("qemu.log");
    std::fs::create_dir_all(log_path.parent().unwrap()).unwrap();
    std::fs::write(&log_path, "qemu loadvm failed\n").unwrap();

    let mut child = Command::new("/bin/sh")
        .args(["-c", "exit 42"])
        .spawn()
        .expect("spawn exiting fake qemu");
    let error = verify_compatibility_resume_loaded(&mut child, &bundle, &log_path).unwrap_err();

    assert!(
        error.contains("Compatibility Mode resume failed: QEMU exited"),
        "unexpected error: {error}"
    );
    assert!(
        error.contains("the suspend snapshot is preserved"),
        "unexpected error: {error}"
    );
    assert!(error.contains(&log_path.display().to_string()));

    let _ = std::fs::remove_dir_all(bundle);
}
