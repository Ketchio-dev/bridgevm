//! Split out of lib.rs by responsibility.

use crate::*;

pub(crate) fn verify_live_evidence_bundle_with_context(
    path: &Path,
    context: Option<&LiveEvidenceVerificationContext>,
) -> Result<VmLiveEvidenceVerification, String> {
    if path.join("qemu-live-evidence.json").exists() {
        verify_qemu_live_evidence_bundle(path, context)
    } else {
        verify_apple_vz_live_evidence_bundle(path, context)
    }
}

pub(crate) fn live_evidence_backend_label(backend: &str) -> &'static str {
    match backend {
        "apple-virtualization-framework" => "Apple VZ",
        "qemu" => "QEMU",
        _ => "backend",
    }
}

pub(crate) fn verify_apple_vz_live_evidence_bundle(
    path: &Path,
    context: Option<&LiveEvidenceVerificationContext>,
) -> Result<VmLiveEvidenceVerification, String> {
    if !path.is_dir() {
        return Err(format!("evidence directory not found: {}", path.display()));
    }

    let summary = read_evidence_text(path, "SUMMARY.txt")?;
    let environment = read_evidence_text(path, "environment.txt")?;
    let validate_output = read_evidence_text(path, "apple-vz-validate.output")?;
    let launch_output = read_evidence_text(path, "apple-vz-live-launch.output")?;
    let missing_opt_in_stderr = read_evidence_text(path, "live-vz-missing-helper-opt-in.stderr")?;
    let missing_opt_in_stdout = read_evidence_text(path, "live-vz-missing-helper-opt-in.stdout")?;
    let runner_path = read_evidence_text(path, "apple-vz-runner.path")?;
    let runner_artifact = read_optional_evidence_text(path, "apple-vz-runner.artifact")?;
    let runner_sha = read_evidence_text(path, "apple-vz-runner.sha256")?;
    let manifest = read_evidence_json(path, "fixture-manifest.json")?;
    let launch = read_evidence_json(path, "apple-vz-launch.json")?;
    let handoff = read_evidence_json(path, "live-vz-handoff.json")?;

    require_contains(
        &summary,
        "Apple VZ live boot opt-in smoke: passed",
        "SUMMARY.txt",
    )?;
    require_contains(&summary, "Serial evidence:", "SUMMARY.txt")?;
    require_contains(
        &validate_output,
        "AppleVzRunner handoff ready",
        "apple-vz-validate.output",
    )?;
    require_contains(
        &validate_output,
        "VZ configuration validation: ready",
        "apple-vz-validate.output",
    )?;
    require_contains(
        &validate_output,
        "Boot loader: linux-kernel",
        "apple-vz-validate.output",
    )?;
    require_contains(
        &validate_output,
        "Disk attachment: disk-image-raw",
        "apple-vz-validate.output",
    )?;
    require_contains(
        &validate_output,
        "Network attachment: nat",
        "apple-vz-validate.output",
    )?;
    require_contains(
        &environment,
        "BRIDGEVM_LIVE_VZ_ALLOW_REAL_START=1",
        "environment.txt",
    )?;
    require_contains(
        &missing_opt_in_stderr,
        "real Apple VZ start requires --allow-real-vz-start",
        "live-vz-missing-helper-opt-in.stderr",
    )?;
    if !missing_opt_in_stdout.is_empty() {
        return Err("live-vz-missing-helper-opt-in.stdout should be empty".to_string());
    }
    let runner_path = runner_path.lines().next().unwrap_or("").trim();
    if runner_path.is_empty() {
        return Err("apple-vz-runner.path is empty".to_string());
    }
    let runner_check_path = if let Some(runner_artifact) = runner_artifact {
        let runner_artifact = runner_artifact.lines().next().unwrap_or("").trim();
        if runner_artifact.is_empty() {
            return Err("apple-vz-runner.artifact is empty".to_string());
        }
        let artifact_path = Path::new(runner_artifact);
        if artifact_path.is_absolute() || runner_artifact.contains("..") {
            return Err(format!(
                "apple-vz-runner.artifact must be a relative evidence path: {runner_artifact}"
            ));
        }
        path.join(artifact_path)
    } else {
        PathBuf::from(runner_path)
    };
    let runner_metadata = fs::symlink_metadata(&runner_check_path).map_err(|error| {
        format!(
            "failed to inspect AppleVzRunner evidence {}: {error}",
            runner_check_path.display()
        )
    })?;
    if runner_metadata.file_type().is_symlink() {
        return Err(format!(
            "AppleVzRunner evidence must not be a symlink: {}",
            runner_check_path.display()
        ));
    }
    if !runner_metadata.is_file() {
        return Err(format!(
            "AppleVzRunner evidence is not a file: {}",
            runner_check_path.display()
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if runner_metadata.permissions().mode() & 0o111 == 0 {
            return Err(format!(
                "AppleVzRunner evidence is not executable: {}",
                runner_check_path.display()
            ));
        }
    }
    let runner_sha = runner_sha.lines().next().unwrap_or("").trim();
    if !is_sha256_hex(runner_sha) {
        return Err("apple-vz-runner.sha256 is not lowercase SHA-256 hex".to_string());
    }
    let actual_runner_sha = sha256_file(&runner_check_path)?;
    if actual_runner_sha != runner_sha {
        return Err("AppleVzRunner SHA-256 does not match evidence artifact".to_string());
    }

    let vm_name = json_string(&launch, &["vm_name"])?;
    if vm_name.trim().is_empty() {
        return Err("launch vm_name is empty".to_string());
    }
    if let Some(context) = context {
        if context.mode != VmMode::Fast {
            return Err(format!(
                "Apple VZ live evidence cannot verify {} Mode VM {}",
                context.mode, context.vm_name
            ));
        }
        if vm_name != context.vm_name {
            return Err(format!(
                "Apple VZ launch vm_name {vm_name} does not match readiness VM {}",
                context.vm_name
            ));
        }
        let launch_bundle_path = json_string(&launch, &["bundle_path"])?;
        let expected_bundle_path = context.bundle_path.display().to_string();
        if launch_bundle_path != expected_bundle_path {
            return Err(format!(
                "Apple VZ launch bundle_path {launch_bundle_path} does not match readiness VM bundle {expected_bundle_path}"
            ));
        }
    }
    require_contains(
        &launch_output,
        "AppleVzRunner handoff ready",
        "apple-vz-live-launch.output",
    )?;
    require_contains(
        &launch_output,
        "Launch spec diagnostics:",
        "apple-vz-live-launch.output",
    )?;
    require_contains(
        &launch_output,
        &format!("AppleVzRunner starting VM: {vm_name}"),
        "apple-vz-live-launch.output",
    )?;
    require_contains(
        &launch_output,
        &format!("AppleVzRunner VM finished: {vm_name}"),
        "apple-vz-live-launch.output",
    )?;
    let boot_mode = json_string(&launch, &["boot", "mode"])?;
    if boot_mode != "linux-kernel" {
        return Err(format!("launch boot mode is not linux-kernel: {boot_mode}"));
    }
    let disk_format = json_string(&launch, &["disk", "format"])?;
    if disk_format != "raw" {
        return Err(format!("launch disk format is not raw: {disk_format}"));
    }
    if let Some(expected) = context.and_then(|context| context.disk_format.as_deref()) {
        if expected == "raw" && disk_format != expected {
            return Err(format!(
                "Apple VZ launch disk format {disk_format} does not match active disk format {expected}"
            ));
        }
    }
    let network = json_string(&launch, &["devices", "network"])?;
    if network != "nat" {
        return Err(format!("launch network is not nat: {network}"));
    }
    if !json_bool(&launch, &["readiness", "ready"])? {
        return Err("launch readiness is not ready".to_string());
    }
    if json_array_len(&launch, &["readiness", "blockers"])? != 0 {
        return Err("launch readiness blockers are not empty".to_string());
    }
    if json_string(&handoff, &["backend"])? != "apple-virtualization-framework" {
        return Err("handoff backend is not apple-virtualization-framework".to_string());
    }
    if !apple_vz_handoff_ready(&handoff)? {
        return Err("handoff is not ready".to_string());
    }
    if json_string(&handoff, &["vm_name"])? != vm_name {
        return Err("handoff VM name does not match launch spec".to_string());
    }

    for key in [
        "source_kernel",
        "source_raw_disk",
        "bundle_kernel",
        "bundle_raw_disk",
    ] {
        verify_fixture_entry(&manifest, key, true)?;
    }
    for key in ["source_initrd", "bundle_initrd"] {
        verify_fixture_entry(&manifest, key, false)?;
    }
    verify_fixture_pair(&manifest, "source_kernel", "bundle_kernel")?;
    verify_fixture_pair(&manifest, "source_raw_disk", "bundle_raw_disk")?;
    verify_fixture_pair(&manifest, "source_initrd", "bundle_initrd")?;

    let launch_kernel = json_string(&launch, &["boot", "kernel", "path"])?;
    let environment_values = parse_environment_values(&environment);
    require_environment_value(
        &environment_values,
        "BRIDGEVM_LIVE_VZ_KERNEL",
        &json_string(&manifest, &["source_kernel", "path"])?,
        "environment kernel path does not match source kernel evidence",
    )?;
    require_environment_value(
        &environment_values,
        "BRIDGEVM_LIVE_VZ_RAW_DISK",
        &json_string(&manifest, &["source_raw_disk", "path"])?,
        "environment raw disk path does not match source raw disk evidence",
    )?;
    require_environment_value(
        &environment_values,
        "BRIDGEVM_LIVE_VZ_KERNEL_CMDLINE",
        &json_string(&launch, &["boot", "kernel_command_line"])?,
        "environment kernel command line does not match launch spec",
    )?;
    require_environment_value(
        &environment_values,
        "BRIDGEVM_LIVE_VZ_MEMORY_MIB",
        &json_u64_like(&launch, &["resources", "memory"])?.to_string(),
        "environment memory does not match launch spec resources",
    )?;
    require_environment_value(
        &environment_values,
        "BRIDGEVM_LIVE_VZ_CPU_COUNT",
        &json_u64_like(&launch, &["resources", "cpu"])?.to_string(),
        "environment CPU count does not match launch spec resources",
    )?;
    let stop_after_seconds = require_environment_entry(
        &environment_values,
        "BRIDGEVM_LIVE_VZ_STOP_AFTER_SECONDS",
        "environment stop-after seconds is missing",
    )?;
    let force_stop_grace_seconds = require_environment_entry(
        &environment_values,
        "BRIDGEVM_LIVE_VZ_FORCE_STOP_GRACE_SECONDS",
        "environment force-stop grace seconds is missing",
    )?;
    require_contains(
        &summary,
        &format!("Stop after seconds: {stop_after_seconds}"),
        "SUMMARY.txt",
    )?;
    require_contains(
        &summary,
        &format!("Force stop grace seconds: {force_stop_grace_seconds}"),
        "SUMMARY.txt",
    )?;
    require_contains(
        &launch_output,
        &format!("BRIDGEVM_LIVE_VZ_STOP_AFTER_SECONDS={stop_after_seconds}"),
        "apple-vz-live-launch.output",
    )?;
    require_contains(
        &launch_output,
        &format!("BRIDGEVM_LIVE_VZ_FORCE_STOP_GRACE_SECONDS={force_stop_grace_seconds}"),
        "apple-vz-live-launch.output",
    )?;
    if let Some(environment_runner) = environment_values.get("BRIDGEVM_LIVE_VZ_RUNNER") {
        if environment_runner != "<auto-build>" && environment_runner != runner_path {
            return Err(
                "environment runner path does not match recorded AppleVzRunner path".to_string(),
            );
        }
    }

    let bundle_kernel = json_string(&manifest, &["bundle_kernel", "path"])?;
    if launch_kernel != bundle_kernel {
        return Err("launch kernel path does not match bundled kernel evidence".to_string());
    }
    let launch_disk = json_string(&launch, &["disk", "path"])?;
    let bundle_disk = json_string(&manifest, &["bundle_raw_disk", "path"])?;
    if launch_disk != bundle_disk {
        return Err("launch disk path does not match bundled raw disk evidence".to_string());
    }
    require_contains(
        &launch_output,
        &format!("Kernel: {launch_kernel} "),
        "apple-vz-live-launch.output",
    )?;
    require_contains(
        &launch_output,
        &format!("Disk: {launch_disk} "),
        "apple-vz-live-launch.output",
    )?;
    let runner_log_path = json_string(&launch, &["logs", "runner_log_path"])?;
    if let Ok(handoff_runner_log_path) = json_string(&handoff, &["runner_log_path"]) {
        if !handoff_runner_log_path.is_empty() && handoff_runner_log_path != runner_log_path {
            return Err("handoff runner log path does not match launch spec".to_string());
        }
    }
    let _runner_log = evidence_bundle_file_path(path, &runner_log_path, "Apple VZ runner log")?;

    let serial_expected = environment_values
        .get("BRIDGEVM_LIVE_VZ_SERIAL_EXPECTED")
        .filter(|value| !value.is_empty() && value.as_str() != "<unset>");
    let serial_sentinel_proven = if let Some(expected) = serial_expected {
        require_contains(
            &summary,
            &format!("required sentinel found: {expected}"),
            "SUMMARY.txt",
        )?;
        let serial_log_path = json_string(&launch, &["devices", "serial_log_path"])?;
        if let Ok(handoff_serial_log_path) = json_string(&handoff, &["serial_log_path"]) {
            if !handoff_serial_log_path.is_empty() && handoff_serial_log_path != serial_log_path {
                return Err("handoff serial log path does not match launch spec".to_string());
            }
        }
        let serial_log_path =
            evidence_bundle_file_path(path, &serial_log_path, "Apple VZ serial log")?;
        let serial_log = read_bounded_text_file(&serial_log_path, "serial log evidence")?;
        require_contains(&serial_log, expected, "serial log evidence")?;
        true
    } else {
        false
    };
    let graphical_boot_progress_proven = verify_graphical_boot_progress_evidence(path)?;
    let viewer_evidence_proven = verify_viewer_evidence(path)?;
    let guest_tools_effects_proven = verify_guest_tools_effects_evidence(path)?;

    Ok(VmLiveEvidenceVerification {
        path: path.to_path_buf(),
        backend: "apple-virtualization-framework".to_string(),
        vm_name,
        boot_mode,
        disk_format,
        network,
        serial_sentinel_required: serial_expected.is_some(),
        serial_sentinel_proven,
        graphical_boot_progress_proven,
        viewer_evidence_proven,
        qmp_evidence_proven: false,
        guest_tools_effects_proven,
        summary: "Apple VZ live boot opt-in smoke: passed".to_string(),
    })
}

pub(crate) fn evidence_bundle_file_path(
    root: &Path,
    artifact: &str,
    label: &str,
) -> Result<PathBuf, String> {
    if artifact.trim().is_empty() {
        return Err(format!("{label} path is empty"));
    }
    let artifact_path = Path::new(artifact);
    let full_path = if artifact_path.is_absolute() {
        artifact_path.to_path_buf()
    } else {
        if artifact_path.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir | std::path::Component::Prefix(_)
            )
        }) {
            return Err(format!("{label} path must stay inside the evidence bundle"));
        }
        root.join(artifact_path)
    };
    let root_canonical = fs::canonicalize(root)
        .map_err(|error| format!("failed to canonicalize evidence bundle: {error}"))?;
    let metadata = fs::symlink_metadata(&full_path)
        .map_err(|error| format!("{label} is not a file: {} ({error})", full_path.display()))?;
    if metadata.file_type().is_symlink() {
        return Err(format!(
            "{label} must not be a symlink: {}",
            full_path.display()
        ));
    }
    if !metadata.is_file() {
        return Err(format!("{label} is not a file: {}", full_path.display()));
    }
    let full_canonical = fs::canonicalize(&full_path)
        .map_err(|error| format!("failed to canonicalize {label}: {error}"))?;
    if !full_canonical.starts_with(&root_canonical) {
        return Err(format!(
            "{label} path must stay inside the evidence bundle: {}",
            full_path.display()
        ));
    }
    Ok(full_path)
}

pub(crate) fn verify_qemu_live_evidence_bundle(
    path: &Path,
    context: Option<&LiveEvidenceVerificationContext>,
) -> Result<VmLiveEvidenceVerification, String> {
    if !path.is_dir() {
        return Err(format!("evidence directory not found: {}", path.display()));
    }

    let evidence = read_evidence_json(path, "qemu-live-evidence.json")?;
    if !json_bool(&evidence, &["proven"])? {
        return Err("qemu-live-evidence.json does not mark live evidence as proven".to_string());
    }
    let backend = json_string(&evidence, &["backend"])?;
    if backend != "qemu" {
        return Err(format!(
            "qemu-live-evidence.json backend is not qemu: {backend}"
        ));
    }
    let vm_name = json_string(&evidence, &["vm_name"])?;
    if vm_name.trim().is_empty() {
        return Err("qemu-live-evidence.json vm_name is empty".to_string());
    }
    if let Some(context) = context {
        if context.mode != VmMode::Compatibility {
            return Err(format!(
                "QEMU live evidence cannot verify {} Mode VM {}",
                context.mode, context.vm_name
            ));
        }
        if vm_name != context.vm_name {
            return Err(format!(
                "qemu-live-evidence.json vm_name {vm_name} does not match readiness VM {}",
                context.vm_name
            ));
        }
    }
    let boot_mode = json_string(&evidence, &["boot_mode"])?;
    if boot_mode != "compatibility" {
        return Err(format!(
            "qemu-live-evidence.json boot_mode is not compatibility: {boot_mode}"
        ));
    }
    let disk_format = json_string(&evidence, &["disk_format"])?;
    if disk_format.trim().is_empty() {
        return Err("qemu-live-evidence.json disk_format is empty".to_string());
    }
    if disk_format != "qcow2" {
        return Err(format!(
            "qemu-live-evidence.json disk_format is not qcow2: {disk_format}"
        ));
    }
    if let Some(expected) = context.and_then(|context| context.disk_format.as_deref()) {
        if disk_format != expected {
            return Err(format!(
                "qemu-live-evidence.json disk_format {disk_format} does not match active disk format {expected}"
            ));
        }
    }
    let network = json_string(&evidence, &["network"])?;
    if network.trim().is_empty() {
        return Err("qemu-live-evidence.json network is empty".to_string());
    }
    if network != "nat" {
        return Err(format!(
            "qemu-live-evidence.json network is not nat: {network}"
        ));
    }
    if let Some(context) = context {
        if network != context.network {
            return Err(format!(
                "qemu-live-evidence.json network {network} does not match expected network {}",
                context.network
            ));
        }
    }

    let command = json_array(&evidence, &["command"])?;
    if command.is_empty() {
        return Err("qemu-live-evidence.json command is empty".to_string());
    }
    let mut command_args = Vec::new();
    for (index, arg) in command.iter().enumerate() {
        command_args.push(
            arg.as_str().ok_or_else(|| {
                format!("qemu-live-evidence.json command[{index}] is not a string")
            })?,
        );
    }
    let executable = command_args[0];
    if !is_supported_qemu_system_executable(executable) {
        return Err(format!(
            "qemu-live-evidence.json command[0] is not a supported qemu-system executable: {executable}"
        ));
    }
    let command_vm_name =
        command_option_value(&command_args, "-name", "qemu-live-evidence.json command")?;
    if command_vm_name != vm_name {
        return Err(format!(
            "qemu-live-evidence.json command -name {command_vm_name} does not match vm_name {vm_name}"
        ));
    }

    if !json_bool(&evidence, &["qmp", "running"])? {
        return Err("qemu-live-evidence.json qmp.running is not true".to_string());
    }
    let qmp_status = json_string(&evidence, &["qmp", "status"])?;
    if qmp_status != "running" {
        return Err(format!(
            "qemu-live-evidence.json qmp.status is not running: {qmp_status}"
        ));
    }
    let qmp_socket = json_string(&evidence, &["qmp", "socket"])?;
    if qmp_socket.trim().is_empty() {
        return Err("qemu-live-evidence.json qmp.socket is empty".to_string());
    }
    if let Some(context) = context {
        let expected_qmp_socket = context.qmp_socket.display().to_string();
        if qmp_socket != expected_qmp_socket {
            return Err(format!(
                "qemu-live-evidence.json qmp.socket {qmp_socket} does not match expected VM QMP socket {expected_qmp_socket}"
            ));
        }
    }
    let command_qmp =
        command_option_value(&command_args, "-qmp", "qemu-live-evidence.json command")?;
    let expected_qmp = format!("unix:{qmp_socket},server=on,wait=off");
    if command_qmp != expected_qmp {
        return Err(format!(
            "qemu-live-evidence.json qmp.socket {qmp_socket} does not match command -qmp {command_qmp}"
        ));
    }

    let qemu_log = verify_evidence_artifact_sha256(path, &evidence, &["artifacts", "qemu_log"])?;
    let serial_log =
        verify_evidence_artifact_sha256(path, &evidence, &["artifacts", "serial_log"])?;
    let qmp_transcript =
        verify_evidence_artifact_sha256(path, &evidence, &["artifacts", "qmp_transcript"])?;
    let qemu_log_content = read_bounded_text_file(&qemu_log, "QEMU log evidence")?;
    let command_line = command_args.join(" ");
    require_contains(&qemu_log_content, &vm_name, "QEMU log evidence")?;
    require_contains(
        &qemu_log_content,
        "QMP status: running",
        "QEMU log evidence",
    )?;
    require_contains(&qemu_log_content, executable, "QEMU log evidence")?;
    require_contains(&qemu_log_content, &qmp_socket, "QEMU log evidence")?;
    require_contains(
        &qemu_log_content,
        &format!("Command: {command_line}"),
        "QEMU log evidence",
    )?;
    require_contains(
        &qemu_log_content,
        &format!("QMP socket: {qmp_socket}"),
        "QEMU log evidence",
    )?;
    verify_qmp_transcript_evidence(&qmp_transcript)?;
    let serial_sentinel = json_string(&evidence, &["serial_sentinel"])?;
    let serial_sentinel_required = !serial_sentinel.trim().is_empty();
    let serial_sentinel_proven = if serial_sentinel_required {
        let serial_content = read_bounded_text_file(&serial_log, "QEMU serial evidence")?;
        require_contains(
            &serial_content,
            &serial_sentinel,
            "QEMU serial log evidence",
        )?;
        true
    } else {
        false
    };

    let graphical_boot_progress_proven = verify_graphical_boot_progress_evidence(path)?;
    let viewer_evidence_proven = verify_viewer_evidence(path)?;
    let guest_tools_effects_proven = verify_guest_tools_effects_evidence(path)?;

    Ok(VmLiveEvidenceVerification {
        path: path.to_path_buf(),
        backend,
        vm_name,
        boot_mode,
        disk_format,
        network,
        serial_sentinel_required,
        serial_sentinel_proven,
        graphical_boot_progress_proven,
        viewer_evidence_proven,
        qmp_evidence_proven: true,
        guest_tools_effects_proven,
        summary: "QEMU live evidence: passed".to_string(),
    })
}

pub(crate) fn is_supported_qemu_system_executable(executable: &str) -> bool {
    let Some(basename) = Path::new(executable)
        .file_name()
        .and_then(|name| name.to_str())
    else {
        return false;
    };
    matches!(
        basename,
        "qemu-system-aarch64" | "qemu-system-i386" | "qemu-system-riscv64" | "qemu-system-x86_64"
    )
}

pub(crate) fn verify_qmp_transcript_evidence(path: &Path) -> Result<(), String> {
    let content = read_bounded_text_file(path, "QMP transcript evidence")?;
    let mut saw_greeting = false;
    let mut saw_query_status_command = false;
    let mut saw_running_query_status_response = false;
    let mut pending_query_status_response = false;

    for (index, line) in content.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let value: serde_json::Value = serde_json::from_str(line).map_err(|error| {
            format!(
                "QMP transcript evidence line {} is not valid JSON: {error}",
                index + 1
            )
        })?;
        if value.get("QMP").is_some() {
            saw_greeting = true;
        }
        if value
            .get("execute")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|execute| execute == "query-status")
        {
            saw_query_status_command = true;
            pending_query_status_response = true;
        }
        if let Some(response) = value.get("return") {
            if response
                .get("status")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|status| status == "running")
                && response
                    .get("running")
                    .and_then(serde_json::Value::as_bool)
                    .is_some_and(|running| running)
                && pending_query_status_response
            {
                saw_running_query_status_response = true;
            }
            pending_query_status_response = false;
        }
    }

    if !saw_greeting {
        return Err("QMP transcript evidence missing QMP greeting".to_string());
    }
    if !saw_query_status_command {
        return Err("QMP transcript evidence missing query-status command".to_string());
    }
    if !saw_running_query_status_response {
        return Err("QMP transcript evidence missing running query-status response".to_string());
    }

    Ok(())
}

pub(crate) fn verify_evidence_artifact_sha256(
    root: &Path,
    value: &serde_json::Value,
    path: &[&str],
) -> Result<PathBuf, String> {
    let artifact = json_nested_string(value, path, "path")?;
    let sha256 = json_nested_string(value, path, "sha256")?;
    if !is_sha256_hex(&sha256) {
        return Err(format!(
            "{}.sha256 is not lowercase SHA-256 hex",
            path.join(".")
        ));
    }
    let artifact_path = relative_evidence_artifact_path(root, &artifact, &path.join("."))?;
    let (actual_sha256, bytes) = sha256_file_with_bytes(&artifact_path, "evidence artifact")?;
    if bytes == 0 {
        return Err(format!("{} artifact is empty", path.join(".")));
    }
    if actual_sha256 != sha256 {
        return Err(format!("{}.sha256 does not match artifact", path.join(".")));
    }
    Ok(artifact_path)
}

pub(crate) fn json_nested_string(
    value: &serde_json::Value,
    base_path: &[&str],
    leaf: &str,
) -> Result<String, String> {
    let mut full_path = base_path.to_vec();
    full_path.push(leaf);
    json_string(value, &full_path)
}
