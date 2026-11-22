//! Split out of main.rs by responsibility.

use crate::*;

pub(crate) fn print_snapshot_chain(chain: &bridgevm_storage::SnapshotChainMetadata) {
    print_active_disk(&chain.active_disk);
    if chain.disks.is_empty() {
        println!("No disk snapshot chain metadata");
        return;
    }
    for disk in &chain.disks {
        println!("Snapshot disk: {}", disk.snapshot);
        print_snapshot_disk_status(disk);
    }
}

pub(crate) fn print_active_disk(active_disk: &bridgevm_storage::ActiveDiskMetadata) {
    println!("Active disk source: {}", active_disk.source);
    if let Some(snapshot) = &active_disk.snapshot {
        println!("Active disk snapshot: {}", snapshot);
    }
    println!("Active disk: {}", active_disk.path.display());
    println!("Active disk format: {}", active_disk.format);
    println!("Active disk ready: {}", active_disk.exists);
    println!("Active disk activated: {}", active_disk.activated_at_unix);
}

pub(crate) fn print_snapshot_disk_status(metadata: &bridgevm_storage::SnapshotDiskMetadata) {
    println!("Snapshot disk overlay: {}", metadata.overlay_path.display());
    println!("Snapshot disk overlay ready: {}", metadata.overlay_exists);
    println!("Snapshot disk backing: {}", metadata.backing_path.display());
    println!("Snapshot disk backing format: {}", metadata.backing_format);
    println!("Snapshot disk backing ready: {}", metadata.backing_exists);
    println!(
        "Snapshot disk create command: {}",
        metadata.create_command.join(" ")
    );
}

pub(crate) fn print_snapshot_disk_create_status(
    metadata: &bridgevm_storage::SnapshotDiskCreateMetadata,
) {
    println!("Snapshot disk create executed: {}", metadata.executed);
    println!(
        "Snapshot disk create command: {}",
        metadata.command.join(" ")
    );
    if let Some(status) = &metadata.exit_status {
        println!("Snapshot disk create status: {}", status);
    }
    if !metadata.stdout.is_empty() {
        println!(
            "Snapshot disk create stdout: {}",
            metadata.stdout.trim_end()
        );
    }
    if !metadata.stderr.is_empty() {
        println!(
            "Snapshot disk create stderr: {}",
            metadata.stderr.trim_end()
        );
    }
    print_snapshot_disk_status(&metadata.disk);
}

pub(crate) fn print_snapshot_suspend_image_status(
    metadata: &bridgevm_storage::SnapshotSuspendImageMetadata,
) {
    println!("Suspend image: {}", metadata.image_path.display());
    println!("Suspend image format: {}", metadata.image_format);
    println!("Suspend image ready: {}", metadata.image_exists);
    println!("Suspend image prepared: {}", metadata.prepared_at_unix);
}

pub(crate) fn print_application_consistent_snapshot_preflight(
    metadata: &ApplicationConsistentSnapshotPreflightMetadata,
) {
    println!("Application-consistent preflight: {}", metadata.snapshot);
    println!("Guest tools connected: {}", metadata.connected);
    println!(
        "Required capabilities: {}",
        metadata.required_capabilities.join(", ")
    );
    println!(
        "Available capabilities: {}",
        if metadata.available_capabilities.is_empty() {
            "none".to_string()
        } else {
            metadata.available_capabilities.join(", ")
        }
    );
    println!(
        "Missing capabilities: {}",
        if metadata.missing_capabilities.is_empty() {
            "none".to_string()
        } else {
            metadata.missing_capabilities.join(", ")
        }
    );
    println!("Application-consistent ready: {}", metadata.ready);
    println!("Planned freeze: {}", metadata.planned_freeze_semantics);
    println!("Planned thaw: {}", metadata.planned_thaw_semantics);
    println!(
        "Guest tools runtime updated: {}",
        metadata
            .runtime_updated_at_unix
            .map_or("unknown".to_string(), |updated| updated.to_string())
    );
    println!("Preflight prepared: {}", metadata.prepared_at_unix);
}

pub(crate) fn print_snapshot_preflight_status(metadata: &SnapshotPreflightStatusRecord) {
    println!("Snapshot preflight for {}", metadata.vm);
    println!("Consistency: {:?}", metadata.consistency);
    println!(
        "Backend freeze/thaw supported: {}",
        metadata.backend_freeze_thaw_supported
    );
    println!("Guest tools connected: {}", metadata.guest_tools_connected);
    println!(
        "Capabilities: {}",
        if metadata.capabilities.is_empty() {
            "none".to_string()
        } else {
            metadata.capabilities.join(", ")
        }
    );
    println!("Preflight ready: {}", metadata.ready);
    if metadata.blockers.is_empty() {
        println!("Blockers: none");
    } else {
        for blocker in &metadata.blockers {
            if let Some(path) = &blocker.path {
                println!(
                    "Blocker: {} - {} ({})",
                    blocker.code,
                    blocker.message,
                    path.display()
                );
            } else {
                println!("Blocker: {} - {}", blocker.code, blocker.message);
            }
        }
    }
    println!("Checked: {}", metadata.checked_at_unix);
}

pub(crate) fn print_application_consistent_snapshot_execution(
    execution: &ApplicationConsistentSnapshotExecutionRecord,
) {
    println!(
        "Application-consistent snapshot execution for {}",
        execution.vm
    );
    println!("Snapshot: {}", execution.snapshot);
    println!("Freeze request ID: {}", execution.freeze_request_id);
    println!("Thaw request ID: {}", execution.thaw_request_id);
    println!(
        "Pending after freeze: {}",
        execution.pending_commands_after_freeze
    );
    println!(
        "Pending after thaw: {}",
        execution.pending_commands_after_thaw
    );
    println!(
        "Freeze result: {} ({})",
        execution.freeze_result.ok,
        execution
            .freeze_result
            .message
            .as_deref()
            .unwrap_or("no message")
    );
    println!(
        "Thaw result: {} ({})",
        execution.thaw_result.ok,
        execution
            .thaw_result
            .message
            .as_deref()
            .unwrap_or("no message")
    );
    println!("Preflight ready: {}", execution.preflight_ready);
    println!("Snapshot created: {}", execution.snapshot_created_at_unix);
    println!("Note: {}", execution.note);
}

pub(crate) fn recommend(args: GuestArgs) -> Result<()> {
    let choice = GuestChoice {
        os: args.os,
        version: args.version,
        arch: args.arch,
    };
    let rec = recommend_mode(&choice);
    print_mode_recommendation(&rec, Some(&choice));
    Ok(())
}

pub(crate) fn hvf(command: HvfCommand) -> Result<()> {
    match command {
        HvfCommand::VirtioGpu3dHostPreflight(args) => {
            let probe = probe_virtio_gpu_3d_host_preflight_for(args.protocol.into());
            print!("{}", probe.render_text());
            Ok(())
        }
        HvfCommand::VirtioGpuTraceReport(args) => {
            let report = analyze_virtio_gpu_trace(&args.trace)?;
            let blockers = report.p3_blockers(args.protocol);
            print_virtio_gpu_trace_report(&args.trace, args.protocol, &report, &blockers);
            if args.require_p3_gate && !blockers.is_empty() {
                bail!("virtio-gpu P3 trace gate failed: {}", blockers.join("; "));
            }
            Ok(())
        }
        HvfCommand::TitleGateReport(args) => run_title_gate_report(args),
    }
}

pub(crate) const VIRTIO_GPU_TRACE_FEATURE_VIRGL: u64 = 1 << 0;
pub(crate) const VIRTIO_GPU_TRACE_FEATURE_RESOURCE_BLOB: u64 = 1 << 3;
pub(crate) const VIRTIO_GPU_TRACE_FEATURE_CONTEXT_INIT: u64 = 1 << 4;
pub(crate) const VIRTIO_TRACE_FEATURE_VERSION_1: u64 = 1 << 0;
pub(crate) const VIRTIO_GPU_TRACE_CAPSET_VIRGL: u64 = 1;
pub(crate) const VIRTIO_GPU_TRACE_CAPSET_VIRGL2: u64 = 2;
pub(crate) const VIRTIO_GPU_TRACE_CAPSET_VENUS: u64 = 4;

#[derive(Debug)]
pub(crate) struct TitleGateManifest {
    pub(crate) path: PathBuf,
    pub(crate) id: String,
    pub(crate) api: String,
    pub(crate) architecture: String,
    pub(crate) executable: Option<String>,
    pub(crate) working_directory: Option<String>,
    pub(crate) arguments: Vec<String>,
    pub(crate) executable_sha256: Option<String>,
    pub(crate) log: PathBuf,
    pub(crate) pass_marker: String,
    pub(crate) minimum_runtime_seconds: u64,
    pub(crate) required_modules: Vec<String>,
    pub(crate) require_main_window: bool,
    pub(crate) minimum_resource_flushes: u64,
}

#[derive(Debug)]
pub(crate) struct TitleGateResult {
    pub(crate) manifest: TitleGateManifest,
    pub(crate) log_path: PathBuf,
    pub(crate) log_sha256: Option<String>,
    pub(crate) fresh_log: bool,
    pub(crate) elapsed_ms: Option<u64>,
    pub(crate) resource_flushes: u64,
    pub(crate) blockers: Vec<String>,
}

impl TitleGateResult {
    pub(crate) fn passed(&self) -> bool {
        self.blockers.is_empty()
    }

    pub(crate) fn as_json(&self) -> serde_json::Value {
        serde_json::json!({
            "id": self.manifest.id,
            "api": self.manifest.api,
            "architecture": self.manifest.architecture,
            "executable": self.manifest.executable,
            "working_directory": self.manifest.working_directory,
            "arguments": self.manifest.arguments,
            "manifest": self.manifest.path,
            "log": self.log_path,
            "log_sha256": self.log_sha256,
            "fresh_log": self.fresh_log,
            "elapsed_ms": self.elapsed_ms,
            "resource_flushes": self.resource_flushes,
            "minimum_resource_flushes": self.manifest.minimum_resource_flushes,
            "passed": self.passed(),
            "blockers": self.blockers,
        })
    }
}
