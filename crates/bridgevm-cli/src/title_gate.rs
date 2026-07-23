//! Split out of main.rs by responsibility.

use crate::*;

pub(crate) fn run_title_gate_report(args: HvfTitleGateReportArgs) -> Result<()> {
    let trace = analyze_virtio_gpu_trace(&args.trace)?;
    let pre_run_state = read_title_pre_run_state(args.pre_run_state.as_deref())?;
    let driver_state_log = args.guest_logs.join("viogpu3d-cleanup.log");
    let driver_state_pass = fs::read_to_string(&driver_state_log)
        .map(|contents| contents.contains("BVGPU-DRIVER-STATE-PASS"))
        .unwrap_or(false);

    let mut results = Vec::with_capacity(args.manifests.len());
    let mut ids = std::collections::BTreeSet::new();
    for manifest_path in &args.manifests {
        let manifest = read_title_gate_manifest(manifest_path)?;
        if !ids.insert(manifest.id.clone()) {
            bail!("duplicate title manifest id '{}'", manifest.id);
        }
        results.push(evaluate_title_gate(
            manifest,
            &args.guest_logs,
            &pre_run_state,
            trace.resource_flush_commands,
            driver_state_pass,
        )?);
    }

    println!("BridgeVM HVF title gate report");
    println!("Trace: {}", args.trace.display());
    println!("Guest logs: {}", args.guest_logs.display());
    println!("Driver state pass: {driver_state_pass}");
    for result in &results {
        println!("Title: {}", result.manifest.id);
        println!("  API: {}", result.manifest.api);
        println!("  Architecture: {}", result.manifest.architecture);
        println!("  Log: {}", result.log_path.display());
        println!("  Fresh log: {}", result.fresh_log);
        println!(
            "  Runtime ms: {}",
            result
                .elapsed_ms
                .map(|value| value.to_string())
                .unwrap_or_else(|| "missing".to_string())
        );
        println!("  RESOURCE_FLUSH commands: {}", result.resource_flushes);
        println!("  Gate: {}", if result.passed() { "PASS" } else { "FAIL" });
        for blocker in &result.blockers {
            println!("    Blocker: {blocker}");
        }
    }

    let passed = results.iter().all(TitleGateResult::passed);
    if let Some(path) = args.json_output {
        let report = serde_json::json!({
            "version": 1,
            "trace": args.trace,
            "guest_logs": args.guest_logs,
            "driver_state_log": driver_state_log,
            "driver_state_pass": driver_state_pass,
            "passed": passed,
            "titles": results.iter().map(TitleGateResult::as_json).collect::<Vec<_>>(),
        });
        let bytes = serde_json::to_vec_pretty(&report)?;
        fs::write(&path, bytes)
            .with_context(|| format!("failed to write title gate report {}", path.display()))?;
    }

    if args.require_title_gates && !passed {
        let blockers = results
            .iter()
            .flat_map(|result| {
                result
                    .blockers
                    .iter()
                    .map(|blocker| format!("{}: {blocker}", result.manifest.id))
            })
            .collect::<Vec<_>>();
        bail!("HVF title gate failed: {}", blockers.join("; "));
    }
    Ok(())
}

pub(crate) fn read_title_pre_run_state(
    path: Option<&Path>,
) -> Result<std::collections::BTreeMap<String, String>> {
    let Some(path) = path else {
        return Ok(std::collections::BTreeMap::new());
    };
    let bytes = fs::read(path)
        .with_context(|| format!("failed to read title pre-run state {}", path.display()))?;
    let value: serde_json::Value = serde_json::from_slice(&bytes)
        .with_context(|| format!("failed to parse title pre-run state {}", path.display()))?;
    let object = value
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("title pre-run state must be a JSON object"))?;
    let mut state = std::collections::BTreeMap::new();
    for (id, hash) in object {
        let hash = hash.as_str().ok_or_else(|| {
            anyhow::anyhow!("title pre-run state value for '{id}' must be a string")
        })?;
        if hash != "missing" && !valid_sha256(hash) {
            bail!("title pre-run state hash for '{id}' is not a SHA-256 digest");
        }
        state.insert(id.clone(), hash.to_ascii_lowercase());
    }
    Ok(state)
}

pub(crate) fn read_title_gate_manifest(path: &Path) -> Result<TitleGateManifest> {
    let bytes = fs::read(path)
        .with_context(|| format!("failed to read title manifest {}", path.display()))?;
    let value: serde_json::Value = serde_json::from_slice(&bytes)
        .with_context(|| format!("failed to parse title manifest {}", path.display()))?;
    let object = value
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("title manifest must be a JSON object"))?;

    let version = manifest_u64(object, "version")?;
    if version != 1 {
        bail!(
            "unsupported title manifest version {version} in {}",
            path.display()
        );
    }
    let id = manifest_string(object, "id")?;
    if id.is_empty()
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        bail!("title manifest id must contain only ASCII letters, digits, '.', '-', or '_'");
    }
    let api = manifest_string(object, "api")?.to_ascii_lowercase();
    if !matches!(api.as_str(), "vulkan" | "d3d11" | "d3d12") {
        bail!("title manifest api must be vulkan, d3d11, or d3d12");
    }
    let architecture = manifest_string(object, "architecture")?.to_ascii_lowercase();
    if !matches!(architecture.as_str(), "arm64" | "x64") {
        bail!("title manifest architecture must be arm64 or x64");
    }
    let log = PathBuf::from(manifest_string(object, "log")?);
    if log.as_os_str().is_empty()
        || log.is_absolute()
        || log.file_name() != Some(log.as_os_str())
        || log
            .components()
            .any(|component| !matches!(component, std::path::Component::Normal(_)))
    {
        bail!("title manifest log must be a file name without directories or traversal");
    }
    let pass_marker = manifest_string(object, "pass_marker")?;
    if pass_marker.trim().is_empty() || pass_marker.contains(['\r', '\n']) {
        bail!("title manifest pass_marker must be a non-empty single-line string");
    }
    let executable_sha256 = object
        .get("executable_sha256")
        .and_then(serde_json::Value::as_str)
        .map(str::to_ascii_lowercase);
    if executable_sha256
        .as_deref()
        .is_some_and(|digest| !valid_sha256(digest))
    {
        bail!("title manifest executable_sha256 must be a 64-character hex digest");
    }
    let executable = manifest_optional_single_line_string(object, "executable")?;
    let working_directory = manifest_optional_single_line_string(object, "working_directory")?;
    let arguments = object
        .get("arguments")
        .map(|value| {
            value
                .as_array()
                .ok_or_else(|| anyhow::anyhow!("title manifest arguments must be an array"))?
                .iter()
                .map(|argument| {
                    argument
                        .as_str()
                        .filter(|argument| !argument.contains(['\r', '\n']))
                        .map(str::to_string)
                        .ok_or_else(|| {
                            anyhow::anyhow!(
                                "title manifest arguments entries must be single-line strings"
                            )
                        })
                })
                .collect::<Result<Vec<_>>>()
        })
        .transpose()?
        .unwrap_or_default();
    let required_modules = object
        .get("required_modules")
        .map(|value| {
            value
                .as_array()
                .ok_or_else(|| anyhow::anyhow!("title manifest required_modules must be an array"))?
                .iter()
                .map(|module| {
                    module
                        .as_str()
                        .filter(|module| !module.trim().is_empty())
                        .map(str::to_string)
                        .ok_or_else(|| {
                            anyhow::anyhow!(
                                "title manifest required_modules entries must be non-empty strings"
                            )
                        })
                })
                .collect::<Result<Vec<_>>>()
        })
        .transpose()?
        .unwrap_or_default();

    Ok(TitleGateManifest {
        path: path.to_path_buf(),
        id,
        api,
        architecture,
        executable,
        working_directory,
        arguments,
        executable_sha256,
        log,
        pass_marker,
        minimum_runtime_seconds: manifest_u64(object, "minimum_runtime_seconds")?,
        required_modules,
        require_main_window: object
            .get("require_main_window")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(true),
        minimum_resource_flushes: manifest_u64(object, "minimum_resource_flushes")?,
    })
}

pub(crate) fn manifest_optional_single_line_string(
    object: &serde_json::Map<String, serde_json::Value>,
    field: &str,
) -> Result<Option<String>> {
    let Some(value) = object.get(field) else {
        return Ok(None);
    };
    let value = value
        .as_str()
        .filter(|value| !value.trim().is_empty() && !value.contains(['\r', '\n']))
        .ok_or_else(|| {
            anyhow::anyhow!("title manifest field '{field}' must be a non-empty single-line string")
        })?;
    Ok(Some(value.to_string()))
}

pub(crate) fn manifest_string(
    object: &serde_json::Map<String, serde_json::Value>,
    field: &str,
) -> Result<String> {
    object
        .get(field)
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| anyhow::anyhow!("title manifest field '{field}' must be a string"))
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn title_gate_manifest_rejects_log_traversal() {
        let mut path = unique_trace_path("bridgevm-title-manifest-traversal");
        path.set_extension("json");
        fs::write(
            &path,
            r#"{
  "version": 1,
  "id": "bad",
  "api": "d3d11",
  "architecture": "x64",
  "log": "../escape.log",
  "pass_marker": "PASS",
  "minimum_runtime_seconds": 30,
  "minimum_resource_flushes": 1
}"#,
        )
        .unwrap();
        let error = read_title_gate_manifest(&path).unwrap_err().to_string();
        fs::remove_file(path).unwrap();
        assert!(error.contains("without directories or traversal"));
    }
}
