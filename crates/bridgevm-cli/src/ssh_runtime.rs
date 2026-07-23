//! Split out of main.rs by responsibility.

use crate::*;

pub(crate) fn ssh(store: &VmStore, args: SshArgs) -> Result<()> {
    let plan =
        bridgevm_api::ssh_plan(store, &args.vm, Some(&args.user)).map_err(anyhow::Error::msg)?;
    print_ssh_plan(&plan);
    Ok(())
}

pub(crate) fn open_port(store: &VmStore, args: OpenArgs) -> Result<()> {
    let plan = open_port_plan(store, &args.vm, args.guest, Some(&args.scheme))
        .map_err(anyhow::Error::msg)?;
    print_open_port_plan(&plan);
    Ok(())
}

pub(crate) fn parse_port_mapping(mapping: &str) -> Result<(u16, u16)> {
    let (host, guest) = mapping
        .split_once(':')
        .ok_or_else(|| anyhow::anyhow!("port mapping must be HOST:GUEST"))?;
    let host = parse_port_number("host", host)?;
    let guest = parse_port_number("guest", guest)?;
    Ok((host, guest))
}

pub(crate) fn parse_port_number(label: &str, value: &str) -> Result<u16> {
    let port = value
        .parse::<u16>()
        .with_context(|| format!("{label} port must be between 1 and 65535"))?;
    if port == 0 {
        bail!("{label} port must be between 1 and 65535");
    }
    Ok(port)
}

pub(crate) fn media(store: &VmStore, args: MediaCommand) -> Result<()> {
    match args.command {
        MediaSubcommand::Download(args) => {
            let download = download_boot_media(store, &args.vm, args.kind.map(Into::into))
                .map_err(anyhow::Error::msg)?;
            print_boot_media_download(&download);
        }
        MediaSubcommand::DownloadPlan(args) => {
            let plan = plan_boot_media_download(
                store,
                &args.vm,
                &args.url,
                args.sha256.as_deref(),
                args.kind.map(Into::into),
            )
            .map_err(anyhow::Error::msg)?;
            print_boot_media_download_plan(&plan);
        }
        MediaSubcommand::Import(args) => {
            let metadata =
                import_boot_media(store, &args.vm, args.source, args.kind.map(Into::into))
                    .map_err(anyhow::Error::msg)?;
            print_boot_media_import(&metadata);
        }
        MediaSubcommand::Status(args) => {
            let status =
                inspect_boot_media_status(store, &args.name).map_err(anyhow::Error::msg)?;
            print_boot_media_status(&status);
        }
        MediaSubcommand::Verify(args) => {
            let verification =
                verify_boot_media(store, &args.vm, &args.sha256, args.kind.map(Into::into))
                    .map_err(anyhow::Error::msg)?;
            print_boot_media_verification(&verification);
        }
    }
    Ok(())
}

pub(crate) fn guest_tools(store: &VmStore, args: GuestToolsCommand) -> Result<()> {
    match args.command {
        GuestToolsSubcommand::Status(args) => {
            let status =
                inspect_guest_tools_status(store, &args.name).map_err(anyhow::Error::msg)?;
            print_guest_tools_status(&status);
        }
        GuestToolsSubcommand::Token(args) => {
            let token = guest_tools_token(store, &args.name).map_err(anyhow::Error::msg)?;
            print_guest_tools_token(&token);
        }
        GuestToolsSubcommand::LinuxCommand(args) => {
            let command = guest_tools_linux_command(
                store,
                &args.vm,
                args.transport.into(),
                args.token_file,
                args.device,
            )
            .map_err(anyhow::Error::msg)?;
            print_guest_tools_linux_command(&command);
        }
        GuestToolsSubcommand::AcceptHello(args) => {
            let envelope = parse_agent_envelope(&args.hello_json)?;
            let session =
                accept_guest_tools_hello(store, &args.vm, &envelope).map_err(anyhow::Error::msg)?;
            print_guest_tools_session(&session);
        }
        GuestToolsSubcommand::SendCommand(_)
        | GuestToolsSubcommand::FreezeFilesystem(_)
        | GuestToolsSubcommand::ThawFilesystem(_)
        | GuestToolsSubcommand::SetClipboard(_)
        | GuestToolsSubcommand::ResizeDisplay(_)
        | GuestToolsSubcommand::MountShare(_)
        | GuestToolsSubcommand::MountApprovedShare(_)
        | GuestToolsSubcommand::UnmountShare(_)
        | GuestToolsSubcommand::FileDropStart(_)
        | GuestToolsSubcommand::FileDropChunk(_)
        | GuestToolsSubcommand::FileDropComplete(_)
        | GuestToolsSubcommand::ListApplications(_)
        | GuestToolsSubcommand::LaunchApplication(_)
        | GuestToolsSubcommand::ListWindows(_)
        | GuestToolsSubcommand::FocusWindow(_)
        | GuestToolsSubcommand::CloseWindow(_)
        | GuestToolsSubcommand::SetWindowBounds(_)
        | GuestToolsSubcommand::WindowPointer(_)
        | GuestToolsSubcommand::WindowKey(_)
        | GuestToolsSubcommand::TimeSync(_) => {
            bail!("guest-tools command dispatch requires --socket bridgevmd access")
        }
    }
    Ok(())
}

pub(crate) fn parse_agent_envelope(value: &str) -> Result<AgentEnvelope> {
    serde_json::from_str(value).context("invalid guest tools envelope JSON")
}

pub(crate) fn agent_command_envelope(
    message: AgentMessage,
    request_id: Option<String>,
) -> AgentEnvelope {
    match request_id {
        Some(request_id) => AgentEnvelope::with_request_id(message, request_id),
        None => AgentEnvelope::new(message),
    }
}

pub(crate) fn current_unix_epoch_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

pub(crate) fn resources(store: &VmStore, args: ResourcesCommand) -> Result<()> {
    match args {
        ResourcesCommand::Reapply(args) => {
            let policy = reapply_runtime_resources(store, &args.name, args.visibility.into())
                .map_err(anyhow::Error::msg)
                .with_context(|| {
                    format!("failed to reapply runtime resources for '{}'", args.name)
                })?;
            print_runtime_resource_policy(&policy);
        }
    }
    Ok(())
}

pub(crate) fn runtime_control(store: &VmStore, args: RuntimeControlCommand) -> Result<()> {
    match args {
        RuntimeControlCommand::Status(args) => {
            run_runtime_control_command(store, &args.name, "status")
        }
        RuntimeControlCommand::Stop(args) => run_runtime_control_command(store, &args.name, "stop"),
        RuntimeControlCommand::Policy(args) => {
            run_runtime_control_command(store, &args.name, "policy")
        }
        RuntimeControlCommand::Pacing(args) => {
            run_runtime_control_command(store, &args.name, "pacing")
        }
        RuntimeControlCommand::Reapply(args) => {
            let policy = reapply_runtime_resources(store, &args.name, args.visibility.into())
                .map_err(anyhow::Error::msg)
                .with_context(|| {
                    format!(
                        "failed to reapply runtime control policy for '{}'",
                        args.name
                    )
                })?;
            print_runtime_resource_policy(&policy);
            Ok(())
        }
    }
}

pub(crate) fn run_runtime_control_command(
    store: &VmStore,
    name: &str,
    command: &str,
) -> Result<()> {
    let control = runtime_control_command(store, name, command).map_err(anyhow::Error::msg)?;
    print_runtime_control_command(&control)
}

pub(crate) fn print_runtime_control_command(control: &RuntimeControlCommandRecord) -> Result<()> {
    println!("Runtime control {} for {}", control.command, control.vm);
    println!("Kind: {}", control.kind);
    println!("Socket: {}", control.socket_path.display());
    println!(
        "{}",
        serde_json::to_string_pretty(&control.response)
            .context("failed to format runtime response")?
    );
    Ok(())
}

pub(crate) fn qemu_args(store: &VmStore, args: VmNameArgs) -> Result<()> {
    let (bundle, manifest, _) = store
        .get_vm_with_active_disk(&args.name)
        .context("failed to read VM")?;
    let command = build_compatibility_command(&manifest, &bundle)
        .map_err(|error| anyhow::anyhow!("{}", compatibility_qemu_command_error(error)))?;
    for word in command.render_shell_words() {
        println!("{word}");
    }
    Ok(())
}

pub(crate) fn prepare_run(store: &VmStore, args: VmNameArgs) -> Result<()> {
    let metadata = build_runner_metadata(store, &args.name, false)?;
    let qmp_supervisor = store
        .qmp_supervisor_metadata(&args.name)
        .context("failed to read QMP supervisor metadata")?;
    print_runner_status(Some(metadata), qmp_supervisor.as_ref(), None);
    Ok(())
}

pub(crate) fn boot_media(store: &VmStore, args: VmNameArgs) -> Result<()> {
    let (bundle, manifest, _) = store
        .get_vm_with_active_disk(&args.name)
        .context("failed to read VM")?;
    let plan =
        build_fast_plan(&manifest, &bundle).context("failed to inspect Fast Mode boot media")?;
    print_boot_media(&args.name, &plan.launch_spec().boot);
    Ok(())
}

pub(crate) fn run_backend_local(store: &VmStore, args: RunArgs) -> Result<()> {
    let metadata = build_runner_metadata(store, &args.name, args.spawn)?;
    let qmp_supervisor = store
        .qmp_supervisor_metadata(&args.name)
        .context("failed to read QMP supervisor metadata")?;
    print_runner_status(Some(metadata), qmp_supervisor.as_ref(), None);
    Ok(())
}

pub(crate) fn suspend_backend_local(store: &VmStore, args: VmNameArgs) -> Result<()> {
    let metadata = suspend_backend(store, &args.name)
        .map_err(anyhow::Error::msg)
        .with_context(|| format!("failed to suspend VM '{}'", args.name))?;
    let qmp_supervisor = store
        .qmp_supervisor_metadata(&args.name)
        .context("failed to read QMP supervisor metadata")?;
    println!("Suspended {}", args.name);
    print_runner_status(Some(metadata), qmp_supervisor.as_ref(), None);
    Ok(())
}

pub(crate) fn resume_backend_local(store: &VmStore, args: VmNameArgs) -> Result<()> {
    let metadata = resume_backend(store, &args.name)
        .map_err(anyhow::Error::msg)
        .with_context(|| format!("failed to resume VM '{}'", args.name))?;
    let qmp_supervisor = store
        .qmp_supervisor_metadata(&args.name)
        .context("failed to read QMP supervisor metadata")?;
    println!("Resumed {}", args.name);
    print_runner_status(Some(metadata), qmp_supervisor.as_ref(), None);
    Ok(())
}

pub(crate) fn display_backend_local(store: &VmStore, args: DisplayArgs) -> Result<()> {
    if !apple_vz_runner_configured() {
        anyhow::bail!(
            "embedded display requires BRIDGEVM_APPLE_VZ_RUNNER to point at a signed AppleVzRunner"
        );
    }
    let display_size = args.display_size()?;
    let metadata = display_fast_backend_with_size(store, &args.name, display_size)
        .map_err(anyhow::Error::msg)
        .with_context(|| format!("failed to launch embedded display for VM '{}'", args.name))?;
    println!(
        "Launched embedded display window for {} (close the window to stop the VM)",
        args.name
    );
    let runtime_policy = store
        .runtime_resource_policy_metadata(&args.name)
        .context("failed to read runtime resource policy metadata")?;
    print_runner_status(Some(metadata), None, runtime_policy.as_ref());
    Ok(())
}

pub(crate) fn build_runner_metadata(
    store: &VmStore,
    name: &str,
    spawn: bool,
) -> Result<bridgevm_storage::RunnerMetadata> {
    let (bundle, mut manifest, _) = store
        .get_vm_with_active_disk(name)
        .context("failed to read VM")?;

    let (disk, active_disk) = store
        .prepare_active_disk(name)
        .context("failed to prepare active disk")?;
    manifest.storage.primary.path = active_disk.path.display().to_string();
    manifest.storage.primary.format = active_disk.format.clone();
    if manifest.mode == VmMode::Fast {
        // Gated REAL cold-start launch: when `BRIDGEVM_APPLE_VZ_RUNNER` is set
        // and the caller asked to spawn, boot a real Apple VZ VM. When unset,
        // preserve the legacy dry-run + runner-required fallback.
        if spawn && apple_vz_runner_configured() {
            return cold_start_fast_backend(store, name)
                .map_err(anyhow::Error::msg)
                .with_context(|| format!("failed to launch Fast Mode VM '{name}'"));
        }
        let plan = build_fast_plan(&manifest, &bundle).context("failed to build Fast Mode plan")?;
        let launch_spec_path = write_launch_spec_artifact(&bundle, plan.launch_spec())
            .context("failed to write Fast Mode launch spec")?;
        let mut readiness = launch_readiness_metadata(&plan.launch_spec().readiness);
        if spawn {
            add_fast_spawn_runner_required_blocker(&mut readiness);
        }
        let spawn_error = spawn.then(|| fast_spawn_runner_required_error(&readiness));
        let metadata = bridgevm_storage::RunnerMetadata {
            engine: "lightvm".to_string(),
            pid: None,
            command: plan.render_runner_words_for_launch_spec(&launch_spec_path),
            log_path: plan.launch_spec().logs.runner_log_path.clone().into(),
            started_at_unix: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            dry_run: true,
            launch_spec_path: Some(launch_spec_path),
            guest_tools: None,
            disk: Some(disk),
            active_disk: Some(active_disk),
            launch_readiness: Some(readiness),
            runtime_control: None,
        };
        store
            .write_runner_metadata(name, &metadata)
            .context("failed to write runner metadata")?;
        if let Some(error) = spawn_error {
            bail!("{}", error);
        }
        return Ok(metadata);
    }

    let command = build_compatibility_command(&manifest, &bundle)
        .map_err(|error| anyhow::anyhow!("{}", compatibility_qemu_command_error(error)))?;
    let readiness = compatibility_launch_readiness_metadata(
        &disk,
        compatibility_launch_dependency_blockers(&manifest, &bundle),
    );
    if spawn && !readiness.ready {
        bail!("{}", compatibility_launch_readiness_summary(&readiness));
    }
    let log_path = bundle.join("logs").join("qemu.log");
    let guest_tools = store
        .guest_tools_runner_metadata(name)
        .context("failed to prepare guest tools runner metadata")?;

    if spawn {
        fs::create_dir_all(bundle.join("logs"))?;
        let stdout = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .context("failed to open QEMU log file")?;
        let stderr = stdout
            .try_clone()
            .context("failed to clone QEMU log file")?;
        let child = ProcessCommand::new(&command.program)
            .args(&command.args)
            .stdout(Stdio::from(stdout))
            .stderr(Stdio::from(stderr))
            .spawn()
            .with_context(|| format!("failed to spawn {}", command.program))?;
        let metadata = bridgevm_storage::RunnerMetadata {
            engine: "fullvm".to_string(),
            pid: Some(child.id()),
            command: command.render_shell_words(),
            log_path,
            started_at_unix: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            dry_run: false,
            launch_spec_path: None,
            guest_tools: Some(guest_tools),
            disk: Some(disk),
            active_disk: Some(active_disk),
            launch_readiness: Some(readiness),
            runtime_control: None,
        };
        store
            .write_runner_metadata(name, &metadata)
            .context("failed to write runner metadata")?;
        store
            .transition_state(name, VmRuntimeState::Running)
            .context("failed to mark VM running")?;
        return Ok(metadata);
    }

    let metadata = bridgevm_storage::RunnerMetadata {
        engine: "fullvm".to_string(),
        pid: None,
        command: command.render_shell_words(),
        log_path,
        started_at_unix: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        dry_run: true,
        launch_spec_path: None,
        guest_tools: Some(guest_tools),
        disk: Some(disk),
        active_disk: Some(active_disk),
        launch_readiness: Some(readiness),
        runtime_control: None,
    };
    store
        .write_runner_metadata(name, &metadata)
        .context("failed to write runner metadata")?;
    Ok(metadata)
}

pub(crate) fn lifecycle_plan(store: &VmStore, args: LifecyclePlanArgs) -> Result<()> {
    match bridgevm_api::handle_request(
        store,
        BridgeVmRequest::LifecyclePlan {
            name: args.name,
            action: args.action.into(),
        },
    ) {
        BridgeVmResponse::LifecyclePlan { plan } => {
            print_lifecycle_plan(&plan);
            Ok(())
        }
        BridgeVmResponse::Error { message } => bail!(message),
        _ => bail!("unexpected lifecycle plan response"),
    }
}

pub(crate) fn readiness(store: &VmStore, args: ReadinessArgs) -> Result<()> {
    let report = bridgevm_api::readiness_report_with_live_evidence_options(
        store,
        &args.name,
        args.live_evidence.as_deref(),
        args.record_live_evidence,
        args.clear_live_evidence,
    )
    .map_err(anyhow::Error::msg)?;
    print_readiness_report(&report);
    Ok(())
}

pub(crate) fn stop_backend_local(store: &VmStore, args: VmNameArgs) -> Result<()> {
    // Delegate to the shared backend so the CLI's direct-stop path also
    // terminates the recorded child process (SIGTERM -> SIGKILL) and clears
    // state, matching the daemon. This guarantees no AppleVzRunner / qemu
    // orphan remains after `bridgevm stop`.
    stop_backend(store, &args.name)
        .map_err(anyhow::Error::msg)
        .with_context(|| format!("failed to stop VM '{}'", args.name))?;
    println!("Stopped {}", args.name);
    Ok(())
}

pub(crate) fn restart_local(store: &VmStore, args: VmNameArgs) -> Result<()> {
    stop_backend_local(
        store,
        VmNameArgs {
            name: args.name.clone(),
        },
    )?;
    let state = store
        .transition_state(&args.name, VmRuntimeState::Running)
        .with_context(|| format!("failed to restart VM '{}'", args.name))?;
    println!(
        "Metadata state recorded for {} ({})",
        args.name, state.state
    );
    Ok(())
}

pub(crate) fn qmp_socket(store: &VmStore, args: VmNameArgs) -> Result<()> {
    let (bundle, _) = store.get_vm(&args.name).context("failed to read VM")?;
    println!("{}", qmp_socket_path(&bundle).display());
    Ok(())
}

pub(crate) fn qmp_status(store: &VmStore, args: VmNameArgs) -> Result<()> {
    let (bundle, _) = store.get_vm(&args.name).context("failed to read VM")?;
    let path = qmp_socket_path(&bundle);
    if !path.exists() {
        println!("QMP socket unavailable: {}", path.display());
        return Ok(());
    }

    let status = match query_status(&path) {
        Ok(status) => status,
        Err(error) if is_qmp_status_unavailable(&error) => {
            println!("QMP socket unavailable: {}", path.display());
            return Ok(());
        }
        Err(error) => return Err(error).context("failed to query QMP status"),
    };
    println!("QMP status: {}", status.status);
    println!("Running: {}", status.running);
    Ok(())
}

pub(crate) fn qmp_control<F>(
    store: &VmStore,
    args: VmNameArgs,
    command: &str,
    execute: F,
) -> Result<()>
where
    F: FnOnce(&Path) -> std::result::Result<(), QemuError>,
{
    let (bundle, _) = store.get_vm(&args.name).context("failed to read VM")?;
    let path = qmp_socket_path(&bundle);
    if !path.exists() {
        bail!("QMP socket unavailable: {}", path.display());
    }

    execute(&path).with_context(|| format!("failed to send QMP {command}"))?;
    println!("QMP command sent: {command}");
    println!("VM: {}", args.name);
    println!("QMP socket: {}", path.display());
    Ok(())
}

pub(crate) fn runner_status(store: &VmStore, args: VmNameArgs) -> Result<()> {
    let metadata = store
        .runner_metadata(&args.name)
        .context("failed to read runner metadata")?;
    let qmp_supervisor = store
        .qmp_supervisor_metadata(&args.name)
        .context("failed to read QMP supervisor metadata")?;
    let runtime_policy = store
        .runtime_resource_policy_metadata(&args.name)
        .context("failed to read runtime resource policy metadata")?;
    print_runner_status(metadata, qmp_supervisor.as_ref(), runtime_policy.as_ref());
    Ok(())
}

pub(crate) fn print_runner_status(
    metadata: Option<bridgevm_storage::RunnerMetadata>,
    qmp_supervisor: Option<&QmpSupervisorMetadata>,
    runtime_policy: Option<&RuntimeResourcePolicyMetadata>,
) {
    match metadata {
        Some(metadata) => {
            println!("Engine: {}", metadata.engine);
            println!(
                "PID: {}",
                metadata
                    .pid
                    .map_or("none".to_string(), |pid| pid.to_string())
            );
            println!("Dry run: {}", metadata.dry_run);
            println!("Metadata recorded: {}", metadata.started_at_unix);
            println!("Log: {}", metadata.log_path.display());
            if let Some(path) = &metadata.launch_spec_path {
                println!("Launch spec: {}", path.display());
            }
            if let Some(guest_tools) = &metadata.guest_tools {
                println!("Guest tools transport: {}", guest_tools.transport);
                println!("Guest tools channel: {}", guest_tools.channel_name);
                println!("Guest tools socket: {}", guest_tools.socket_path.display());
                println!(
                    "Guest tools token file: {}",
                    guest_tools.token_path.display()
                );
                println!(
                    "Guest tools token created: {}",
                    guest_tools.token_created_at_unix
                );
            }
            if let Some(disk) = metadata.disk {
                print_disk_status(&disk);
            }
            if let Some(readiness) = metadata.launch_readiness {
                print_launch_readiness(&readiness);
            }
            if let Some(runtime_control) = &metadata.runtime_control {
                print_runtime_control(runtime_control);
            }
            if let Some(policy) = runtime_policy {
                print_runner_runtime_policy(policy);
            }
            println!("Command: {}", metadata.command.join(" "));
        }
        None => println!("No runner metadata"),
    }
    if let Some(supervisor) = qmp_supervisor {
        print_qmp_supervisor(supervisor);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_store(prefix: &str) -> VmStore {
        let mut root = std::env::temp_dir();
        root.push(format!(
            "{prefix}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        VmStore::new(root)
    }

    fn unique_socket_path(prefix: &str) -> PathBuf {
        let mut path = PathBuf::from("/tmp");
        path.push(format!(
            "{prefix}-{}-{}.sock",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        path
    }

    fn test_manifest(name: &str) -> VmManifest {
        VmManifest::new(
            name,
            VmMode::Fast,
            Guest {
                os: "ubuntu".to_string(),
                version: None,
                arch: "arm64".to_string(),
            },
            "80GiB",
        )
    }

    fn compatibility_manifest(name: &str) -> VmManifest {
        VmManifest::new(
            name,
            VmMode::Compatibility,
            Guest {
                os: "ubuntu".to_string(),
                version: None,
                arch: "x86_64".to_string(),
            },
            "64GiB",
        )
    }

    #[test]
    fn runtime_control_status_uses_recorded_socket_metadata() {
        let store = unique_store("bridgevm-cli-runtime-control-test");
        store.create_vm(&test_manifest("dev")).unwrap();
        let socket_path = unique_socket_path("bridgevm-cli-rc");
        let listener = std::os::unix::net::UnixListener::bind(&socket_path).unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = String::new();
            BufReader::new(stream.try_clone().unwrap())
                .read_line(&mut request)
                .unwrap();
            let request: serde_json::Value = serde_json::from_str(&request).unwrap();
            assert_eq!(
                request.get("command").and_then(serde_json::Value::as_str),
                Some("status")
            );
            stream
                .write_all(
                    br#"{"display":{"height":768,"width":1024},"ok":true,"state":"running","stopping":false,"supported_commands":["status","stop","policy","pacing"],"vm":"dev"}"#,
                )
                .unwrap();
            stream.write_all(b"\n").unwrap();
        });

        let metadata = bridgevm_storage::RunnerMetadata {
            engine: "lightvm".to_string(),
            pid: Some(42),
            command: vec!["lightvm-runner".to_string()],
            log_path: PathBuf::from("lightvm.log"),
            started_at_unix: 1,
            dry_run: false,
            launch_spec_path: None,
            guest_tools: None,
            disk: None,
            active_disk: None,
            launch_readiness: None,
            runtime_control: Some(bridgevm_storage::RuntimeControlMetadata {
                kind: "apple-vz-display".to_string(),
                socket_path: socket_path.clone(),
                commands: vec![
                    "status".to_string(),
                    "stop".to_string(),
                    "policy".to_string(),
                    "pacing".to_string(),
                ],
            }),
        };
        store.write_runner_metadata("dev", &metadata).unwrap();

        run_runtime_control_command(&store, "dev", "status").unwrap();
        server.join().unwrap();
        let _ = fs::remove_file(socket_path);
    }

    #[test]
    fn port_mapping_parser_rejects_invalid_shapes() {
        assert!(parse_port_mapping("3000").is_err());
        assert!(parse_port_mapping("0:3000").is_err());
        assert!(parse_port_mapping("3000:0").is_err());
        assert!(parse_port_mapping("abc:3000").is_err());
    }

    #[test]
    fn local_prepare_run_error_preserves_qemu_network_blocker_requirement() {
        let store = unique_store("bridgevm-cli-qemu-network-blocker-test");
        let mut manifest = compatibility_manifest("legacy");
        manifest.network.mode = "advanced".to_string();
        store.create_vm(&manifest).unwrap();

        let error = build_runner_metadata(&store, "legacy", false).unwrap_err();
        let message = format!("{error:#}");

        assert!(
            message.contains("failed to build Compatibility Mode QEMU command"),
            "missing CLI context: {message}"
        );
        assert!(
            message.contains("QEMU launch blocker qemu-advanced-network-requires-schema"),
            "missing QEMU blocker: {message}"
        );
        assert!(
            message.contains("requirement: Compatibility Mode QEMU requires an advanced network schema and launcher wiring before launch"),
            "missing QEMU requirement: {message}"
        );
    }
}
