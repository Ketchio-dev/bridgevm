//! Fail-closed support-bundle policy: fixed structural records, no recursive copying.

use crate::*;

pub(crate) const RECORDS: [(&str, &str); 6] = [
    ("qmp-supervisor.json", "record-01.json"),
    ("runner.json", "record-02.json"),
    ("apple-vz-launch.json", "record-03.json"),
    ("live-evidence.json", "record-04.json"),
    ("state.json", "record-05.json"),
    ("runtime-resources.json", "record-06.json"),
];
const SAFE_KEYS: &[&str] = &[
    "available",
    "backend",
    "code",
    "enabled",
    "exists",
    "kind",
    "limit_reached",
    "mode",
    "outcome",
    "pass",
    "ready",
    "scope",
    "state",
    "status",
    "terminal_event",
];

pub(crate) fn build_bundle(
    name: &str,
    source: PathBuf,
    manifest: VmManifest,
    destination: PathBuf,
    created_at_unix: u64,
) -> Result<DiagnosticBundleMetadata, String> {
    let mut files = Vec::new();
    write_vm_summary(&manifest, &destination, &mut files)?;
    copy_records(&source, &destination, &mut files)?;
    write_log_summary(&source, &destination, &mut files)?;
    let mut metadata = DiagnosticBundleMetadata {
        vm: name.to_string(),
        source,
        output: destination.clone(),
        files,
        created_at_unix,
    };
    metadata.files.push(PathBuf::from("diagnostic-bundle.json"));
    let public = DiagnosticBundleMetadata {
        vm: "<redacted>".to_string(),
        source: PathBuf::from("<redacted>"),
        output: PathBuf::from("<redacted>"),
        files: metadata.files.clone(),
        created_at_unix,
    };
    fs::write(
        destination.join("diagnostic-bundle.json"),
        serde_json::to_string_pretty(&public).map_err(|e| e.to_string())?,
    )
    .map_err(|e| format!("failed to write diagnostic bundle metadata: {e}"))?;
    Ok(metadata)
}

pub(crate) fn cleanup_error(staging: &Path, error: String) -> String {
    let _ = fs::remove_dir_all(staging);
    error
}

fn copy_records(source: &Path, destination: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    for (source_name, output_name) in RECORDS {
        let input = source.join("metadata").join(source_name);
        if !input.exists() {
            continue;
        }
        let bytes = diagnostic_record_reader::read(&input)?;
        let value: serde_json::Value = serde_json::from_slice(&bytes)
            .map_err(|_| "diagnostic allowlist input is not valid JSON".to_string())?;
        let projected = project(value);
        fs::write(
            destination.join(output_name),
            serde_json::to_vec_pretty(&projected).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        files.push(PathBuf::from(output_name));
    }
    Ok(())
}

fn write_vm_summary(
    manifest: &bridgevm_config::VmManifest,
    destination: &Path,
    files: &mut Vec<PathBuf>,
) -> Result<(), String> {
    let summary = serde_json::json!({
        "schema_version": bridgevm_config::SCHEMA_VERSION,
        "mode": allow(&format!("{:?}", manifest.mode).to_ascii_lowercase(), &["fast", "compatibility"]),
        "guest_os": allow(&manifest.guest.os, &["ubuntu", "debian", "windows", "macos"]),
        "guest_arch": allow(&manifest.guest.arch, &["arm64", "aarch64", "x86_64"]),
        "backend_engine": allow(&manifest.backend.engine, &["apple-vz", "qemu", "hvf", "lightvm"]),
        "display_renderer": allow(&manifest.display.renderer, &["native", "vnc", "cocoa", "none"]),
        "network_mode": allow(&manifest.network.mode, &["nat", "host-only", "bridged", "isolated"]),
        "firmware": {"nvme_target": manifest.firmware.nvme_target,
            "tpm": manifest.firmware.tpm, "secure_boot": manifest.firmware.secure_boot}
    });
    write_json(destination, "vm-summary.json", &summary, files)
}

fn write_log_summary(
    source: &Path,
    destination: &Path,
    files: &mut Vec<PathBuf>,
) -> Result<(), String> {
    let mut count = 0u64;
    let mut bytes = 0u64;
    if let Ok(entries) = fs::read_dir(source.join("logs")) {
        for entry in entries {
            let entry = entry.map_err(|e| e.to_string())?;
            if !entry.file_type().map_err(|e| e.to_string())?.is_file() {
                continue;
            }
            let metadata = entry.metadata().map_err(|e| e.to_string())?;
            count += 1;
            bytes = bytes.saturating_add(metadata.len());
        }
    }
    write_json(
        destination,
        "log-summary.json",
        &serde_json::json!({"regular_file_count": count, "total_bytes": bytes}),
        files,
    )
}

fn project(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => serde_json::Value::Object(
            map.into_iter()
                .filter(|(key, _)| SAFE_KEYS.contains(&key.as_str()))
                .map(|(key, value)| (key, project(value)))
                .collect(),
        ),
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.into_iter().take(128).map(project).collect())
        }
        serde_json::Value::String(value) => serde_json::Value::String(
            allow(
                &value,
                &[
                    "running",
                    "paused",
                    "suspended",
                    "stopped",
                    "error",
                    "planned",
                    "ready",
                    "blocked",
                    "completed",
                    "failed",
                    "accepted",
                    "pass",
                    "qemu",
                    "lightvm",
                    "apple-vz",
                    "compatibility",
                    "fast",
                ],
            )
            .to_string(),
        ),
        serde_json::Value::Bool(value) => serde_json::Value::Bool(value),
        _ => serde_json::Value::Null,
    }
}

fn allow<'a>(value: &'a str, choices: &[&str]) -> &'a str {
    if choices.contains(&value) {
        value
    } else {
        "<redacted>"
    }
}

fn write_json(
    destination: &Path,
    name: &str,
    value: &serde_json::Value,
    files: &mut Vec<PathBuf>,
) -> Result<(), String> {
    fs::write(
        destination.join(name),
        serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    files.push(PathBuf::from(name));
    Ok(())
}
