//! Split out of main.rs by responsibility.

use crate::*;

pub(crate) fn print_clone(clone: &bridgevm_storage::VmCloneMetadata) {
    println!(
        "Cloned {} from {} to {}",
        clone.vm,
        clone.source.display(),
        clone.output.display()
    );
    if clone.linked {
        println!("Linked clone: true");
        if let Some(backing_path) = &clone.backing_path {
            println!("Backing disk: {}", backing_path.display());
        }
        if let Some(backing_format) = &clone.backing_format {
            println!("Backing format: {backing_format}");
        }
        if let Some(command) = &clone.create_command {
            println!("Clone disk create command: {}", command.join(" "));
        }
    }
}

pub(crate) fn diagnostics(store: &VmStore, args: DiagnosticsCommand) -> Result<()> {
    match args.command {
        DiagnosticsSubcommand::Bundle(args) => {
            let bundle = create_diagnostic_bundle(store, &args.vm, args.output)
                .map_err(anyhow::Error::msg)?;
            print_diagnostic_bundle(&bundle);
        }
    }
    Ok(())
}

pub(crate) fn logs(store: &VmStore, args: LogsCommand) -> Result<()> {
    let log = match args.command {
        LogsSubcommand::Qemu(args) => {
            view_vm_log(store, &args.vm, VmLogKind::Qemu, args.bytes).map_err(anyhow::Error::msg)?
        }
        LogsSubcommand::Serial(args) => view_vm_log(store, &args.vm, VmLogKind::Serial, args.bytes)
            .map_err(anyhow::Error::msg)?,
    };
    print_vm_log(&log);
    Ok(())
}

pub(crate) fn performance(store: &VmStore, args: PerformanceCommand) -> Result<()> {
    match args.command {
        PerformanceSubcommand::Baseline(args) => {
            let baseline = create_performance_baseline(store, &args.vm, args.output)
                .map_err(anyhow::Error::msg)?;
            print_performance_baseline(&baseline);
        }
        PerformanceSubcommand::Sample(args) => {
            let sample = create_performance_sample(
                store,
                &args.vm,
                args.output,
                args.artifact_bytes,
                args.iterations,
                args.sync,
            )
            .map_err(anyhow::Error::msg)?;
            print_performance_sample(&sample);
        }
    }
    Ok(())
}

pub(crate) fn metadata(store: &VmStore, args: MetadataCommand) -> Result<()> {
    match args.command {
        MetadataSubcommand::Repair(args) => {
            let repair = store
                .repair_metadata(&args.name)
                .with_context(|| format!("failed to repair metadata for VM '{}'", args.name))?;
            print_metadata_repair(&repair);
        }
        MetadataSubcommand::MigrateManifest(args) => {
            let migration = store
                .migrate_manifest(&args.name, args.dry_run)
                .with_context(|| format!("failed to migrate manifest for VM '{}'", args.name))?;
            print_manifest_migration(&migration);
        }
        MetadataSubcommand::ManifestSchema => {
            println!("{}", manifest_json_schema_v1());
        }
        MetadataSubcommand::ValidateManifest(args) => {
            let manifest = VmManifest::read(&args.path)
                .with_context(|| format!("failed to validate manifest {}", args.path.display()))?;
            println!("Manifest valid: {}", args.path.display());
            println!("Schema version: {}", manifest.schema_version);
            println!("Name: {}", manifest.name);
            println!("Mode: {}", manifest.mode);
        }
    }
    Ok(())
}

pub(crate) fn print_metadata_repair(repair: &VmMetadataRepairMetadata) {
    println!("Metadata repair for {}", repair.vm);
    println!("Metadata repaired: {}", repair.repaired);
    println!("Bundle: {}", repair.bundle.display());
    println!("Timestamp: {}", repair.repaired_at_unix);
    if repair.actions.is_empty() {
        println!("No metadata repairs needed");
        return;
    }
    for action in &repair.actions {
        println!(
            "{}: {} ({})",
            action.action,
            action.path.display(),
            action.detail
        );
    }
}

pub(crate) fn print_manifest_migration(migration: &VmManifestMigrationMetadata) {
    println!("Manifest migration for {}", migration.vm);
    println!("Dry run: {}", migration.dry_run);
    println!("Migrated: {}", migration.migrated);
    println!("From schema: {}", migration.from_schema);
    println!("To schema: {}", migration.to_schema);
    println!("Bundle: {}", migration.bundle.display());
    println!("Manifest: {}", migration.manifest_path.display());
    println!("Timestamp: {}", migration.migrated_at_unix);
    if let Some(path) = &migration.backup_path {
        println!("Backup: {}", path.display());
    }
    if let Some(path) = &migration.receipt_path {
        println!("Receipt: {}", path.display());
    }
    for action in &migration.actions {
        println!(
            "{}: {} ({})",
            action.action,
            action.path.display(),
            action.detail
        );
    }
}

pub(crate) fn snapshot(store: &VmStore, args: SnapshotCommand) -> Result<()> {
    match args.command {
        SnapshotSubcommand::Create(args) => {
            let snapshot = store
                .create_snapshot(&args.vm, &args.name, args.kind.into())
                .context("failed to create snapshot metadata")?;
            println!(
                "Created {} snapshot '{}' for {}",
                snapshot.kind, snapshot.name, args.vm
            );
            if let Some(disk) = store
                .snapshot_disk_metadata(&args.vm, &args.name)
                .context("failed to read snapshot disk metadata")?
            {
                print_snapshot_disk_status(&disk);
            }
            if let Some(preflight) = store
                .application_consistent_snapshot_preflight_metadata(&args.vm, &args.name)
                .context("failed to read application-consistent snapshot preflight metadata")?
            {
                print_application_consistent_snapshot_preflight(&preflight);
            }
            Ok(())
        }
        SnapshotSubcommand::ExecuteApplicationConsistent(_) => {
            bail!("application-consistent snapshot execution requires --socket bridgevmd access")
        }
        SnapshotSubcommand::List(args) => {
            let snapshots = store
                .snapshots(&args.name)
                .context("failed to list snapshots")?;
            if snapshots.is_empty() {
                println!("No snapshots found for {}", args.name);
                return Ok(());
            }
            for snapshot in snapshots {
                println!(
                    "{}\t{}\t{}\t{}",
                    snapshot.name, snapshot.kind, snapshot.vm_state, snapshot.created_at_unix
                );
            }
            Ok(())
        }
        SnapshotSubcommand::Chain(args) => {
            let chain = store
                .snapshot_chain(&args.name)
                .context("failed to inspect snapshot chain")?;
            print_snapshot_chain(&chain);
            Ok(())
        }
        SnapshotSubcommand::Restore(args) => {
            let restore = store
                .restore_snapshot(&args.vm, &args.name)
                .context("failed to restore snapshot metadata")?;
            println!(
                "Restored snapshot '{}' metadata for {}; recorded state: {}",
                restore.snapshot, args.vm, restore.restored_state
            );
            if let Some(active_disk) = restore.active_disk {
                print_active_disk(&active_disk);
            }
            if let Some(suspend_image) = restore.suspend_image {
                print_snapshot_suspend_image_status(&suspend_image);
            }
            Ok(())
        }
        SnapshotSubcommand::DiskCreate(args) => {
            let metadata = store
                .create_snapshot_disk(&args.vm, &args.name)
                .context("failed to create snapshot disk overlay")?;
            print_snapshot_disk_create_status(&metadata);
            Ok(())
        }
    }
}

pub(crate) fn disk(store: &VmStore, args: DiskCommand) -> Result<()> {
    match args.command {
        DiskSubcommand::Prepare(args) => {
            let metadata = store
                .prepare_primary_disk(&args.name)
                .context("failed to prepare primary disk")?;
            print_disk_status(&metadata);
            Ok(())
        }
        DiskSubcommand::Create(args) => {
            let metadata = store
                .create_primary_disk(&args.name)
                .context("failed to create primary disk")?;
            print_disk_create_status(&metadata);
            Ok(())
        }
        DiskSubcommand::Inspect(args) => {
            let metadata = store
                .inspect_primary_disk(&args.name)
                .context("failed to inspect primary disk")?;
            print_disk_inspect_status(&metadata);
            Ok(())
        }
        DiskSubcommand::Verify(args) => {
            let metadata = store
                .verify_active_disk(&args.name)
                .context("failed to verify active disk")?;
            print_disk_verify_status(&metadata);
            Ok(())
        }
        DiskSubcommand::Compact(args) => {
            let metadata = store
                .compact_active_disk(&args.name)
                .context("failed to compact active disk")?;
            print_disk_compact_status(&metadata);
            Ok(())
        }
    }
}

pub(crate) fn port(store: &VmStore, args: PortCommand) -> Result<()> {
    match args.command {
        PortSubcommand::List(args) => {
            let ports = list_ports(store, &args.name).map_err(anyhow::Error::msg)?;
            print_port_forwards(&ports);
        }
        PortSubcommand::Add(args) => {
            let (host, guest) = parse_port_mapping(&args.mapping)?;
            let ports = add_port(store, &args.vm, host, guest).map_err(anyhow::Error::msg)?;
            print_port_forwards(&ports);
        }
        PortSubcommand::Remove(args) => {
            let (host, guest) = parse_port_mapping(&args.mapping)?;
            let ports = remove_port(store, &args.vm, host, guest).map_err(anyhow::Error::msg)?;
            print_port_forwards(&ports);
        }
    }
    Ok(())
}

pub(crate) fn network_plan(store: &VmStore, args: VmNameArgs) -> Result<()> {
    let plan = bridgevm_api::network_plan(store, &args.name).map_err(anyhow::Error::msg)?;
    print_network_plan(&plan);
    Ok(())
}

pub(crate) fn share(store: &VmStore, args: ShareCommand) -> Result<()> {
    match args.command {
        ShareSubcommand::List(args) => {
            let shares = list_shares(store, &args.name).map_err(anyhow::Error::msg)?;
            print_shared_folders(&shares);
        }
        ShareSubcommand::Add(args) => {
            let shares = add_share(
                store,
                &args.vm,
                args.name,
                args.host_path,
                args.read_only,
                args.host_path_token,
            )
            .map_err(anyhow::Error::msg)?;
            print_shared_folders(&shares);
        }
        ShareSubcommand::Remove(args) => {
            let shares = remove_share(store, &args.vm, &args.name).map_err(anyhow::Error::msg)?;
            print_shared_folders(&shares);
        }
    }
    Ok(())
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

    fn unique_trace_path(prefix: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "{prefix}-{}-{}.jsonl",
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

    #[test]
    fn hvf_virtio_block_file_backing_probe_cli_requires_disk() {
        let error = Cli::try_parse_from(["bridgevm", "hvf", "virtio-block-file-backing-probe"])
            .unwrap_err();

        assert!(error.to_string().contains("--disk"));
    }

    #[test]
    fn hvf_virtio_block_writable_file_backing_probe_cli_requires_disk() {
        let error = Cli::try_parse_from([
            "bridgevm",
            "hvf",
            "virtio-block-writable-file-backing-probe",
        ])
        .unwrap_err();

        assert!(error.to_string().contains("--disk"));
    }

    #[test]
    fn title_gate_report_rejects_duplicate_title_ids() {
        let mut root = unique_trace_path("bridgevm-title-duplicate");
        root.set_extension("dir");
        let guest_logs = root.join("guest-logs");
        fs::create_dir_all(&guest_logs).unwrap();
        let trace = root.join("trace.jsonl");
        fs::write(&trace, "").unwrap();
        let manifest = root.join("title.json");
        fs::write(
            &manifest,
            r#"{
  "version": 1,
  "id": "duplicate",
  "api": "vulkan",
  "architecture": "arm64",
  "log": "title.log",
  "pass_marker": "PASS",
  "minimum_runtime_seconds": 1,
  "minimum_resource_flushes": 0
}"#,
        )
        .unwrap();

        let error = run_title_gate_report(HvfTitleGateReportArgs {
            manifests: vec![manifest.clone(), manifest],
            guest_logs,
            trace,
            pre_run_state: None,
            json_output: None,
            require_title_gates: false,
        })
        .unwrap_err()
        .to_string();

        fs::remove_dir_all(root).unwrap();
        assert!(error.contains("duplicate title manifest id 'duplicate'"));
    }

    #[test]
    fn windows_hvf_machine_plan_render_is_blocked_and_qemu_free() {
        let plan = plan_windows_11_arm_hvf_machine(HvfMachinePlanOptions {
            installer: Some(PathBuf::from("ISO/Win11_25H2_English_Arm64_v2.iso")),
            memory_gib: 6,
            vcpu_count: 4,
        });
        let output = plan.render_text();

        assert!(output.contains("Windows 11 Arm HVF machine plan"));
        assert!(output.contains("QEMU: not used"));
        assert!(output.contains("Memory: 6 GiB"));
        assert!(output.contains("vCPU lifecycle:"));
        assert!(output.contains("Devices:"));
        assert!(output.contains("firmware UART and RTC skeletons"));
        assert!(output.contains("read-only installer media"));
        assert!(output.contains("system boot disk"));
        assert!(output.contains("Overall: blocked"));
        assert!(!output.contains("qemu-system"));
        assert!(!output.contains('%'));
    }

    #[test]
    fn guest_tools_mount_share_cli_builds_host_command_envelope() {
        let cli = Cli::try_parse_from([
            "bridgevm",
            "guest-tools",
            "mount-share",
            "dev",
            "--name",
            "work",
            "--host-path-token",
            "share-token-1",
            "--request-id",
            "mount-1",
        ])
        .unwrap();

        let request = request_for(cli.command).unwrap();
        let BridgeVmRequest::GuestToolsSendCommand { name, envelope } = request else {
            panic!("expected guest tools send command request");
        };

        assert_eq!(name, "dev");
        assert_eq!(envelope.request_id.as_deref(), Some("mount-1"));
        assert_eq!(
            envelope.message,
            AgentMessage::MountShare {
                name: "work".to_string(),
                host_path_token: "share-token-1".to_string(),
            }
        );
    }

    #[test]
    fn application_consistent_snapshot_cli_builds_typed_request() {
        let cli = Cli::try_parse_from([
            "bridgevm",
            "snapshot",
            "create",
            "dev",
            "before-upgrade",
            "--kind",
            "application-consistent",
        ])
        .unwrap();

        let request = request_for(cli.command).unwrap();
        let BridgeVmRequest::CreateSnapshot { vm, name, kind } = request else {
            panic!("expected create snapshot request");
        };

        assert_eq!(vm, "dev");
        assert_eq!(name, "before-upgrade");
        assert_eq!(kind, SnapshotKind::ApplicationConsistent);

        let cli = Cli::try_parse_from([
            "bridgevm",
            "snapshot",
            "execute-application-consistent",
            "dev",
            "before-upgrade",
            "--freeze-timeout-millis",
            "5000",
        ])
        .unwrap();

        let request = request_for(cli.command).unwrap();
        let BridgeVmRequest::ExecuteApplicationConsistentSnapshot {
            vm,
            name,
            freeze_timeout_millis,
        } = request
        else {
            panic!("expected execute application-consistent snapshot request");
        };

        assert_eq!(vm, "dev");
        assert_eq!(name, "before-upgrade");
        assert_eq!(freeze_timeout_millis, Some(5_000));
    }

    #[test]
    fn snapshot_restore_cli_builds_typed_request() {
        let cli = Cli::try_parse_from(["bridgevm", "snapshot", "restore", "dev", "before-upgrade"])
            .unwrap();

        let request = request_for(cli.command).unwrap();
        assert_eq!(
            request,
            BridgeVmRequest::RestoreSnapshot {
                vm: "dev".to_string(),
                name: "before-upgrade".to_string(),
            }
        );
    }

    #[test]
    fn guest_tools_mount_approved_share_cli_builds_named_share_request() {
        let cli = Cli::try_parse_from([
            "bridgevm",
            "guest-tools",
            "mount-approved-share",
            "dev",
            "--share",
            "work",
            "--request-id",
            "mount-1",
        ])
        .unwrap();

        let request = request_for(cli.command).unwrap();
        assert_eq!(
            request,
            BridgeVmRequest::GuestToolsMountApprovedShare {
                name: "dev".to_string(),
                share: "work".to_string(),
                request_id: Some("mount-1".to_string()),
            }
        );
    }

    #[test]
    fn guest_tools_unmount_share_cli_builds_host_command_envelope() {
        let cli = Cli::try_parse_from([
            "bridgevm",
            "guest-tools",
            "unmount-share",
            "dev",
            "--name",
            "work",
            "--request-id",
            "unmount-1",
        ])
        .unwrap();

        let request = request_for(cli.command).unwrap();
        let BridgeVmRequest::GuestToolsSendCommand { name, envelope } = request else {
            panic!("expected guest tools send command request");
        };

        assert_eq!(name, "dev");
        assert_eq!(envelope.request_id.as_deref(), Some("unmount-1"));
        assert_eq!(
            envelope.message,
            AgentMessage::UnmountShare {
                name: "work".to_string(),
            }
        );
    }

    #[test]
    fn port_add_and_remove_cli_build_typed_requests() {
        let add = Cli::try_parse_from(["bridgevm", "port", "add", "legacy", "3000:3000"]).unwrap();
        let request = request_for(add.command).unwrap();
        assert_eq!(
            request,
            BridgeVmRequest::AddPort {
                name: "legacy".to_string(),
                host: 3000,
                guest: 3000,
            }
        );

        let remove =
            Cli::try_parse_from(["bridgevm", "port", "remove", "legacy", "3000:3000"]).unwrap();
        let request = request_for(remove.command).unwrap();
        assert_eq!(
            request,
            BridgeVmRequest::RemovePort {
                name: "legacy".to_string(),
                host: 3000,
                guest: 3000,
            }
        );
    }

    #[test]
    fn local_fast_spawn_error_updates_runner_metadata_with_blocker() {
        let store = unique_store("bridgevm-cli-fast-spawn-blocker-test");
        store.create_vm(&test_manifest("fast-linux")).unwrap();

        let error = build_runner_metadata(&store, "fast-linux", true).unwrap_err();
        let message = format!("{error:#}");

        assert!(
            message.contains("Fast Mode spawn requires BRIDGEVM_APPLE_VZ_RUNNER"),
            "{message}"
        );
        assert!(message.contains("launch blockers:"), "{message}");
        assert!(message.contains("missing-primary-disk"), "{message}");
        assert!(message.contains("apple-vz-runner-unavailable"), "{message}");
        let metadata = store
            .runner_metadata("fast-linux")
            .unwrap()
            .expect("Fast spawn blocker writes dry-run runner metadata");
        assert!(metadata.dry_run);
        assert_eq!(metadata.engine, "lightvm");
        let readiness = metadata
            .launch_readiness
            .expect("Fast Mode runner metadata includes launch readiness");
        assert!(!readiness.ready);
        assert!(readiness
            .blockers
            .iter()
            .any(|blocker| blocker.code == "apple-vz-runner-unavailable"));
    }

    #[test]
    fn share_cli_builds_typed_requests() {
        let list = Cli::try_parse_from(["bridgevm", "share", "list", "dev"]).unwrap();
        assert_eq!(
            request_for(list.command).unwrap(),
            BridgeVmRequest::ListShares {
                name: "dev".to_string(),
            }
        );

        let add = Cli::try_parse_from([
            "bridgevm",
            "share",
            "add",
            "dev",
            "workspace",
            "/Users/me/project",
            "--read-only",
            "--host-path-token",
            "share-token-workspace",
        ])
        .unwrap();
        assert_eq!(
            request_for(add.command).unwrap(),
            BridgeVmRequest::AddShare {
                name: "dev".to_string(),
                share: "workspace".to_string(),
                host_path: "/Users/me/project".to_string(),
                read_only: true,
                host_path_token: Some("share-token-workspace".to_string()),
            }
        );

        let remove =
            Cli::try_parse_from(["bridgevm", "share", "remove", "dev", "workspace"]).unwrap();
        assert_eq!(
            request_for(remove.command).unwrap(),
            BridgeVmRequest::RemoveShare {
                name: "dev".to_string(),
                share: "workspace".to_string(),
            }
        );
    }

    #[test]
    fn diagnostics_bundle_cli_builds_typed_request() {
        let cli = Cli::try_parse_from([
            "bridgevm",
            "diagnostics",
            "bundle",
            "legacy",
            "--output",
            "target/diagnostics",
        ])
        .unwrap();

        let request = request_for(cli.command).unwrap();
        assert_eq!(
            request,
            BridgeVmRequest::CreateDiagnosticBundle {
                name: "legacy".to_string(),
                output: PathBuf::from("target/diagnostics"),
            }
        );
    }

    #[test]
    fn logs_cli_builds_typed_request() {
        let cli =
            Cli::try_parse_from(["bridgevm", "logs", "qemu", "legacy", "--bytes", "4096"]).unwrap();

        let request = request_for(cli.command).unwrap();
        assert_eq!(
            request,
            BridgeVmRequest::ViewLogs {
                name: "legacy".to_string(),
                kind: VmLogKind::Qemu,
                max_bytes: Some(4096),
            }
        );
    }

    #[test]
    fn serial_logs_cli_builds_typed_request() {
        let cli = Cli::try_parse_from(["bridgevm", "logs", "serial", "legacy"]).unwrap();

        let request = request_for(cli.command).unwrap();
        assert_eq!(
            request,
            BridgeVmRequest::ViewLogs {
                name: "legacy".to_string(),
                kind: VmLogKind::Serial,
                max_bytes: None,
            }
        );
    }

    #[test]
    fn performance_baseline_cli_builds_typed_request() {
        let cli = Cli::try_parse_from([
            "bridgevm",
            "performance",
            "baseline",
            "dev",
            "--output",
            "target/performance",
        ])
        .unwrap();

        let request = request_for(cli.command).unwrap();
        assert_eq!(
            request,
            BridgeVmRequest::CreatePerformanceBaseline {
                name: "dev".to_string(),
                output: PathBuf::from("target/performance"),
            }
        );
    }

    #[test]
    fn performance_sample_cli_builds_typed_request() {
        let cli = Cli::try_parse_from([
            "bridgevm",
            "performance",
            "sample",
            "dev",
            "--output",
            "target/performance",
            "--artifact-bytes",
            "4096",
            "--iterations",
            "3",
            "--sync",
        ])
        .unwrap();

        let request = request_for(cli.command).unwrap();
        assert_eq!(
            request,
            BridgeVmRequest::CreatePerformanceSample {
                name: "dev".to_string(),
                output: PathBuf::from("target/performance"),
                artifact_bytes: Some(4096),
                iterations: Some(3),
                sync: true,
            }
        );
    }

    #[test]
    fn performance_sample_cli_uses_default_options() {
        let cli = Cli::try_parse_from([
            "bridgevm",
            "performance",
            "sample",
            "dev",
            "--output",
            "target/performance",
        ])
        .unwrap();

        let request = request_for(cli.command).unwrap();
        assert_eq!(
            request,
            BridgeVmRequest::CreatePerformanceSample {
                name: "dev".to_string(),
                output: PathBuf::from("target/performance"),
                artifact_bytes: None,
                iterations: None,
                sync: false,
            }
        );
    }

    #[test]
    fn performance_sample_cli_accepts_bounds_friendly_args() {
        let cli = Cli::try_parse_from([
            "bridgevm",
            "performance",
            "sample",
            "dev",
            "--output",
            "target/performance",
            "--artifact-bytes",
            "18446744073709551615",
            "--iterations",
            "65535",
        ])
        .unwrap();

        let request = request_for(cli.command).unwrap();
        assert_eq!(
            request,
            BridgeVmRequest::CreatePerformanceSample {
                name: "dev".to_string(),
                output: PathBuf::from("target/performance"),
                artifact_bytes: Some(u64::MAX),
                iterations: Some(u16::MAX),
                sync: false,
            }
        );
    }

    #[test]
    fn metadata_repair_cli_builds_typed_request() {
        let cli = Cli::try_parse_from(["bridgevm", "metadata", "repair", "dev"]).unwrap();
        let request = request_for(cli.command).unwrap();
        assert_eq!(
            request,
            BridgeVmRequest::RepairMetadata {
                name: "dev".to_string(),
            }
        );
    }

    #[test]
    fn metadata_migrate_manifest_cli_builds_typed_request() {
        let cli = Cli::try_parse_from([
            "bridgevm",
            "metadata",
            "migrate-manifest",
            "dev",
            "--dry-run",
        ])
        .unwrap();
        let request = request_for(cli.command).unwrap();
        assert_eq!(
            request,
            BridgeVmRequest::MigrateManifest {
                name: "dev".to_string(),
                dry_run: true,
            }
        );
    }

    #[test]
    fn metadata_manifest_schema_is_local_only() {
        let cli = Cli::try_parse_from(["bridgevm", "metadata", "manifest-schema"]).unwrap();
        let error = request_for(cli.command).unwrap_err().to_string();
        assert!(error.contains("local-only"), "{error}");
    }

    #[test]
    fn metadata_validate_manifest_is_local_only() {
        let cli =
            Cli::try_parse_from(["bridgevm", "metadata", "validate-manifest", "manifest.yaml"])
                .unwrap();
        let error = request_for(cli.command).unwrap_err().to_string();
        assert!(error.contains("local-only"), "{error}");
    }

    #[test]
    fn disk_verify_cli_builds_typed_request() {
        let cli = Cli::try_parse_from(["bridgevm", "disk", "verify", "dev"]).unwrap();
        let request = request_for(cli.command).unwrap();
        assert_eq!(
            request,
            BridgeVmRequest::VerifyDisk {
                name: "dev".to_string(),
            }
        );
    }
}
