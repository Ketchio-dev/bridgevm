//! Split out of main.rs by responsibility.

use crate::*;

pub(crate) fn print_disk_verify_status(metadata: &bridgevm_storage::DiskVerifyMetadata) {
    println!("Disk verify command: {}", metadata.command.join(" "));
    println!("Disk verify status: {}", metadata.exit_status);
    println!(
        "Disk verify duration: {} microseconds",
        metadata.verify_duration_microseconds
    );
    if !metadata.stderr.is_empty() {
        println!("Disk verify stderr: {}", metadata.stderr.trim_end());
    }
    print_active_disk(&metadata.active_disk);
    println!(
        "Disk verify report: {}",
        serde_json::to_string_pretty(&metadata.report)
            .unwrap_or_else(|_| metadata.report.to_string())
    );
}

pub(crate) fn print_disk_compact_status(metadata: &bridgevm_storage::DiskCompactMetadata) {
    println!("Disk compact command: {}", metadata.command.join(" "));
    println!("Disk compact status: {}", metadata.exit_status);
    println!(
        "Disk compact duration: {} microseconds",
        metadata.compact_duration_microseconds
    );
    println!("Disk compact backup: {}", metadata.backup_path.display());
    println!(
        "Disk compact original bytes: {}",
        metadata.original_size_bytes
    );
    println!(
        "Disk compact compacted bytes: {}",
        metadata.compacted_size_bytes
    );
    if !metadata.stdout.is_empty() {
        println!("Disk compact stdout: {}", metadata.stdout.trim_end());
    }
    if !metadata.stderr.is_empty() {
        println!("Disk compact stderr: {}", metadata.stderr.trim_end());
    }
    print_active_disk(&metadata.active_disk);
}

pub(crate) fn print_port_forwards(ports: &PortForwardListRecord) {
    println!("Port forwards for {}", ports.vm);
    if ports.forwards.is_empty() {
        println!("No port forwards configured");
        return;
    }
    for forward in &ports.forwards {
        println!("{}:{}", forward.host, forward.guest);
    }
}

pub(crate) fn print_shared_folders(shares: &SharedFolderListRecord) {
    println!("Shared folders for {}", shares.vm);
    if shares.shared_folders.is_empty() {
        println!("No shared folders configured");
        return;
    }
    for folder in &shares.shared_folders {
        println!("Shared folder: {}", folder.name);
        println!("Host path: {}", folder.host_path);
        println!("Read-only: {}", folder.read_only);
        println!("Host path token: {}", folder.host_path_token);
    }
}

pub(crate) fn print_ssh_plan(plan: &SshPlanRecord) {
    println!("SSH target for {}", plan.vm);
    println!("Source: {:?}", plan.source);
    println!("Host: {}", plan.host);
    println!("Port: {}", plan.port);
    println!("User: {}", plan.user);
    println!("Command: {}", plan.command.join(" "));
}

pub(crate) fn print_open_port_plan(plan: &OpenPortPlanRecord) {
    println!("Open target for {}", plan.vm);
    println!("Scheme: {}", plan.scheme);
    println!("Host: {}", plan.host);
    println!("URL: {}", plan.url);
    println!("Guest port: {}", plan.guest_port);
    println!("Host port: {}", plan.host_port);
    println!("Command: {}", plan.command.join(" "));
}

pub(crate) fn print_diagnostic_bundle(bundle: &DiagnosticBundleMetadata) {
    println!("Diagnostic bundle for {}", bundle.vm);
    println!("Output: {}", bundle.output.display());
    println!("Files: {}", bundle.files.len());
}

pub(crate) fn print_vm_log(log: &VmLogViewRecord) {
    println!("Log for {}", log.vm);
    println!("Kind: {:?}", log.kind);
    println!("Path: {}", log.path.display());
    println!("Exists: {}", log.exists);
    println!("Bytes: {}", log.bytes);
    println!("Returned bytes: {}", log.returned_bytes);
    println!("Truncated: {}", log.truncated);
    if !log.content.is_empty() {
        println!("--- log tail ---");
        print!("{}", log.content);
        if !log.content.ends_with('\n') {
            println!();
        }
    }
}

pub(crate) fn print_performance_baseline(baseline: &PerformanceBaselineMetadata) {
    println!("Performance baseline for {}", baseline.vm);
    println!("Output: {}", baseline.output.display());
    println!("Artifact: {}", baseline.artifact.display());
    println!("Metadata only: {}", baseline.metadata_only);
    println!("State: {}", baseline.state.state);
    match &baseline.runner {
        Some(runner) => {
            println!("Runner: {}", runner.engine);
            println!("Runner dry run: {}", runner.dry_run);
        }
        None => println!("Runner: unavailable"),
    }
    match &baseline.metrics {
        Some(metrics) => {
            println!("Guest CPU: {}%", metrics.cpu_percent);
            println!("Guest memory: {} MiB", metrics.memory_used_mib);
        }
        None => println!("Guest metrics: unavailable"),
    }
    print_performance_measurements(&baseline.measurements);
}

pub(crate) fn print_performance_sample(sample: &PerformanceSampleMetadata) {
    println!("Performance sample for {}", sample.vm);
    println!("Output: {}", sample.output.display());
    println!("Artifact: {}", sample.artifact.display());
    println!("Probe: {}", sample.probe.display());
    println!("Probes: {}", sample.probes.len());
    println!("Probe bytes: {}", sample.artifact_bytes);
    println!("Iterations: {}", sample.iterations);
    println!("Sync: {}", sample.sync);
    println!("State: {}", sample.state.state);
    print_performance_measurements(&sample.measurements);
}

pub(crate) fn print_performance_measurements(
    measurements: &[bridgevm_api::PerformanceMeasurementRecord],
) {
    println!("Measurements: {}", measurements.len());
    for measurement in measurements {
        println!(
            "Measurement: {}={} {} ({})",
            measurement.name, measurement.value, measurement.unit, measurement.source
        );
    }
}
