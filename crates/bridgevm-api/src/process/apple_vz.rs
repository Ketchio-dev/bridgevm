//! Split out of process.rs by responsibility.

use crate::*;

pub fn apple_vz_display_control_socket_path(bundle: &Path) -> PathBuf {
    PathBuf::from(format!(
        "/tmp/bvm-vz-{:016x}.sock",
        stable_runtime_control_socket_hash(&bundle.to_string_lossy())
    ))
}

pub fn apple_vz_display_framebuffer_rgba_path(bundle: &Path) -> PathBuf {
    bundle
        .join("metadata")
        .join("apple-vz-display-framebuffer.rgba")
}

pub(crate) fn stable_runtime_control_socket_hash(value: &str) -> u64 {
    const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
    value.as_bytes().iter().fold(FNV_OFFSET, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(FNV_PRIME)
    })
}

pub fn runtime_control_command(
    store: &VmStore,
    name: &str,
    command: &str,
) -> Result<RuntimeControlCommandRecord, String> {
    let metadata = store
        .runner_metadata(name)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("No runner metadata recorded for {name}"))?;
    let control = metadata
        .runtime_control
        .ok_or_else(|| format!("No runtime control metadata recorded for {name}"))?;
    if !control
        .commands
        .iter()
        .any(|available| available == command)
    {
        let available = if control.commands.is_empty() {
            "none".to_string()
        } else {
            control.commands.join(", ")
        };
        return Err(format!(
            "runtime control `{}` is not advertised for {} (available: {})",
            command, name, available
        ));
    }

    let response = send_runtime_control_command(&control.socket_path, command)?;
    Ok(RuntimeControlCommandRecord {
        vm: name.to_string(),
        kind: control.kind,
        socket_path: control.socket_path,
        command: command.to_string(),
        response,
    })
}

pub(crate) fn send_runtime_control_command(
    socket: &Path,
    command: &str,
) -> Result<serde_json::Value, String> {
    let mut stream = UnixStream::connect(socket).map_err(|error| {
        format!(
            "failed to connect to runtime control socket {}: {}",
            socket.display(),
            error
        )
    })?;
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .map_err(|error| format!("failed to configure runtime control read timeout: {error}"))?;
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .map_err(|error| format!("failed to configure runtime control write timeout: {error}"))?;
    let mut request = serde_json::to_vec(&serde_json::json!({ "command": command }))
        .map_err(|error| format!("failed to encode runtime control request: {error}"))?;
    request.push(b'\n');
    stream
        .write_all(&request)
        .map_err(|error| format!("failed to write runtime control request: {error}"))?;

    let mut response = Vec::new();
    BufReader::new(stream)
        .take(MAX_RUNTIME_CONTROL_RESPONSE_BYTES + 1)
        .read_until(b'\n', &mut response)
        .map_err(|error| format!("failed to read runtime control response: {error}"))?;
    if response.is_empty() {
        return Err("runtime control socket returned an empty response".to_string());
    }
    if response.len() as u64 > MAX_RUNTIME_CONTROL_RESPONSE_BYTES {
        return Err(format!(
            "runtime control response exceeded {} bytes",
            MAX_RUNTIME_CONTROL_RESPONSE_BYTES
        ));
    }
    if response.last() != Some(&b'\n') {
        return Err("runtime control socket returned an incomplete response frame".to_string());
    }
    serde_json::from_slice(&response)
        .map_err(|error| format!("invalid runtime control response JSON: {error}"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FastRunnerDisplayConfig<'a> {
    pub size: Option<(u32, u32)>,
    pub runtime_control_socket: Option<&'a Path>,
    pub proxy_framebuffer_rgba_file: Option<&'a Path>,
    pub proxy_framebuffer_capture_interval_ms: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FastRunnerConfig<'a> {
    pub launch_spec_path: &'a Path,
    pub apple_vz_runner: &'a Path,
    pub restore_state: Option<&'a Path>,
    pub display: Option<FastRunnerDisplayConfig<'a>>,
}

impl<'a> FastRunnerConfig<'a> {
    pub fn new(launch_spec_path: &'a Path, apple_vz_runner: &'a Path) -> Self {
        Self {
            launch_spec_path,
            apple_vz_runner,
            restore_state: None,
            display: None,
        }
    }
}

/// Build the `lightvm-runner` argv used to launch a Fast Mode Apple VZ VM.
///
/// Shared by the Fast cold-start (`run_backend`) and resume (`resume_backend`)
/// paths. The only difference is the optional saved-state restore: a cold start
/// passes `restore_state == None` (fresh boot), while resume passes the saved
/// state file so the runner appends `--apple-vz-restore-state <file>`.
pub fn fast_runner_args(config: FastRunnerConfig<'_>) -> Vec<String> {
    let mut args: Vec<String> = vec![
        "--launch-spec".to_string(),
        config.launch_spec_path.display().to_string(),
        "--require-ready".to_string(),
        "--launch".to_string(),
        "--apple-vz-runner".to_string(),
        config.apple_vz_runner.display().to_string(),
        "--apple-vz-allow-real-start".to_string(),
    ];
    if let Some(state_path) = config.restore_state {
        args.push("--apple-vz-restore-state".to_string());
        args.push(state_path.display().to_string());
    }
    // Embedded display: lightvm-runner forwards this as `--display` to the
    // AppleVzRunner, which boots with a graphics device + hosts a window.
    if let Some(display) = config.display {
        args.push("--apple-vz-display".to_string());
        if let Some((width, height)) = display.size {
            args.push("--apple-vz-display-width".to_string());
            args.push(width.to_string());
            args.push("--apple-vz-display-height".to_string());
            args.push(height.to_string());
        }
        if let Some(socket_path) = display.runtime_control_socket {
            args.push("--apple-vz-runtime-control-socket".to_string());
            args.push(socket_path.display().to_string());
        }
        if let Some(path) = display.proxy_framebuffer_rgba_file {
            args.push("--apple-vz-proxy-framebuffer-rgba-file".to_string());
            args.push(path.display().to_string());
        }
        if let Some(interval_ms) = display.proxy_framebuffer_capture_interval_ms {
            args.push("--apple-vz-proxy-framebuffer-capture-interval-ms".to_string());
            args.push(interval_ms.to_string());
        }
    }
    args
}

/// Launch a Fast Mode Apple VZ VM via `lightvm-runner` (DETACHED).
///
/// Shared spawn path for the Fast cold-start (`restore_state == None`) and
/// resume (`restore_state == Some(state_file)`). Resolves the signed
/// AppleVzRunner, builds the launch spec, spawns the runner without waiting,
/// records the child pid with `dry_run:false`, and transitions the VM Running.
pub(crate) fn spawn_fast_backend(
    store: &VmStore,
    name: &str,
    bundle: &Path,
    manifest: &VmManifest,
    restore_state: Option<&Path>,
    display: bool,
    display_size: Option<(u32, u32)>,
) -> Result<RunnerMetadata, String> {
    let apple_vz_runner = require_apple_vz_runner()?;
    let lightvm_runner = find_lightvm_runner();

    let (disk, active_disk) = store
        .prepare_active_disk(name)
        .map_err(|error| error.to_string())?;

    let mut manifest = manifest.clone();
    apply_active_disk_to_manifest(&mut manifest, &active_disk);
    let plan = build_fast_plan(&manifest, bundle).map_err(|error| error.to_string())?;
    let launch_spec_path = write_launch_spec_artifact(bundle, plan.launch_spec())
        .map_err(|error| error.to_string())?;
    let readiness = launch_readiness_metadata(&plan.launch_spec().readiness);
    if !readiness.ready {
        return Err(format!(
            "Fast Mode launch readiness failed: {}",
            launch_readiness_blocker_summary(&readiness)
        ));
    }

    fs::create_dir_all(bundle.join("logs")).map_err(|error| error.to_string())?;
    fs::create_dir_all(bundle.join("run")).map_err(|error| error.to_string())?;
    let log_path: PathBuf = plan.launch_spec().logs.runner_log_path.clone().into();
    let stdout = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|error| error.to_string())?;
    let stderr = stdout.try_clone().map_err(|error| error.to_string())?;

    let runtime_control = display.then(|| RuntimeControlMetadata {
        kind: "apple-vz-display".to_string(),
        socket_path: apple_vz_display_control_socket_path(bundle),
        commands: vec![
            "status".to_string(),
            "stop".to_string(),
            "policy".to_string(),
            "pacing".to_string(),
        ],
    });
    let proxy_framebuffer_rgba_file =
        display.then(|| apple_vz_display_framebuffer_rgba_path(bundle));
    let args = fast_runner_args(FastRunnerConfig {
        launch_spec_path: &launch_spec_path,
        apple_vz_runner: &apple_vz_runner,
        restore_state,
        display: display.then_some(FastRunnerDisplayConfig {
            size: display_size,
            runtime_control_socket: runtime_control
                .as_ref()
                .map(|control| control.socket_path.as_path()),
            proxy_framebuffer_rgba_file: proxy_framebuffer_rgba_file.as_deref(),
            proxy_framebuffer_capture_interval_ms: None,
        }),
    });

    let mut runner_command = Command::new(&lightvm_runner);
    runner_command
        .args(&args)
        .env("BRIDGEVM_APPLE_VZ_ALLOW_REAL_START", "1")
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr));
    let child = spawn_detached_fast_runner(&mut runner_command)
        .map_err(|error| format!("failed to spawn {}: {error}", lightvm_runner.display()))?;

    let mut command = vec![lightvm_runner.display().to_string()];
    command.extend(args);
    let metadata = RunnerMetadata {
        engine: "lightvm".to_string(),
        pid: Some(child.id()),
        command,
        log_path,
        started_at_unix: now_unix(),
        dry_run: false,
        launch_spec_path: Some(launch_spec_path),
        guest_tools: None,
        disk: Some(disk),
        active_disk: Some(active_disk),
        launch_readiness: Some(readiness),
        runtime_control,
    };
    store
        .write_runner_metadata(name, &metadata)
        .map_err(|error| error.to_string())?;
    store
        .transition_state(name, VmRuntimeState::Running)
        .map_err(|error| error.to_string())?;
    if display {
        let policy = build_runtime_resource_policy_metadata(
            name,
            &manifest,
            RuntimeResourceVisibility::Foreground,
            VmRuntimeState::Running,
        );
        store
            .write_runtime_resource_policy_metadata(name, &policy)
            .map_err(|error| error.to_string())?;
    }

    Ok(metadata)
}
