//! Split out of lib.rs by responsibility.

use crate::*;

pub(crate) fn require_environment_entry<'a>(
    values: &'a std::collections::BTreeMap<String, String>,
    key: &str,
    message: &str,
) -> Result<&'a str, String> {
    values
        .get(key)
        .map(String::as_str)
        .filter(|value| !value.is_empty() && *value != "<unset>")
        .ok_or_else(|| message.to_string())
}

pub(crate) fn require_environment_value(
    values: &std::collections::BTreeMap<String, String>,
    key: &str,
    expected: &str,
    message: &str,
) -> Result<(), String> {
    let actual = require_environment_entry(values, key, message)?;
    if actual == expected {
        Ok(())
    } else {
        Err(message.to_string())
    }
}

pub(crate) fn verify_fixture_entry(
    value: &serde_json::Value,
    key: &str,
    required_existing: bool,
) -> Result<(), String> {
    let exists = json_bool(value, &[key, "exists"])?;
    if required_existing && !exists {
        return Err(format!(
            "fixture manifest entry is not marked existing: {key}"
        ));
    }
    if exists {
        let path = json_string(value, &[key, "path"])?;
        if path.is_empty() {
            return Err(format!("fixture manifest entry has empty path: {key}"));
        }
        let bytes = json_u64(value, &[key, "bytes"])?;
        if bytes == 0 {
            return Err(format!(
                "fixture manifest entry has invalid byte count: {key}"
            ));
        }
        let sha256 = json_string(value, &[key, "sha256"])?;
        if !is_sha256_hex(&sha256) {
            return Err(format!("fixture manifest entry has invalid SHA-256: {key}"));
        }
        let metadata = fs::symlink_metadata(&path).map_err(|error| {
            format!("fixture manifest entry path is not a file: {key} ({error})")
        })?;
        if metadata.file_type().is_symlink() {
            return Err(format!(
                "fixture manifest entry path must not be a symlink: {key}"
            ));
        }
        if !metadata.is_file() {
            return Err(format!("fixture manifest entry path is not a file: {key}"));
        }
        if metadata.len() != bytes {
            return Err(format!(
                "fixture manifest entry byte count does not match file: {key}"
            ));
        }
        let actual_sha256 = sha256_file(Path::new(&path))?;
        if actual_sha256 != sha256 {
            return Err(format!(
                "fixture manifest entry SHA-256 does not match file: {key}"
            ));
        }
    }
    Ok(())
}

pub(crate) fn verify_fixture_pair(
    value: &serde_json::Value,
    source_key: &str,
    bundle_key: &str,
) -> Result<(), String> {
    let source_exists = json_bool(value, &[source_key, "exists"])?;
    let bundle_exists = json_bool(value, &[bundle_key, "exists"])?;
    if source_exists != bundle_exists {
        return Err(format!(
            "source/bundle existence mismatch: {source_key} vs {bundle_key}"
        ));
    }
    if source_exists {
        if json_u64(value, &[source_key, "bytes"])? != json_u64(value, &[bundle_key, "bytes"])? {
            return Err(format!(
                "source/bundle byte count mismatch: {source_key} vs {bundle_key}"
            ));
        }
        if json_string(value, &[source_key, "sha256"])?
            != json_string(value, &[bundle_key, "sha256"])?
        {
            return Err(format!(
                "source/bundle SHA-256 mismatch: {source_key} vs {bundle_key}"
            ));
        }
    }
    Ok(())
}

pub(crate) fn json_at<'a>(
    value: &'a serde_json::Value,
    path: &[&str],
) -> Result<&'a serde_json::Value, String> {
    let mut current = value;
    for segment in path {
        current = current
            .get(*segment)
            .ok_or_else(|| format!("evidence JSON missing {}", path.join(".")))?;
    }
    Ok(current)
}

pub(crate) fn json_string(value: &serde_json::Value, path: &[&str]) -> Result<String, String> {
    json_at(value, path)?
        .as_str()
        .map(ToString::to_string)
        .ok_or_else(|| format!("evidence JSON field is not a string: {}", path.join(".")))
}

pub(crate) fn json_bool(value: &serde_json::Value, path: &[&str]) -> Result<bool, String> {
    json_at(value, path)?
        .as_bool()
        .ok_or_else(|| format!("evidence JSON field is not a bool: {}", path.join(".")))
}

pub(crate) fn apple_vz_handoff_ready(handoff: &serde_json::Value) -> Result<bool, String> {
    if let Ok(ready) = json_bool(handoff, &["readiness", "ready"]) {
        return Ok(ready);
    }
    json_bool(handoff, &["ready"])
}

pub(crate) fn json_u64(value: &serde_json::Value, path: &[&str]) -> Result<u64, String> {
    json_at(value, path)?
        .as_u64()
        .ok_or_else(|| format!("evidence JSON field is not a u64: {}", path.join(".")))
}

pub(crate) fn json_u64_like(value: &serde_json::Value, path: &[&str]) -> Result<u64, String> {
    let value = json_at(value, path)?;
    if let Some(number) = value.as_u64() {
        return Ok(number);
    }
    if let Some(text) = value.as_str() {
        return text
            .parse::<u64>()
            .map_err(|_| format!("evidence JSON field is not a u64: {}", path.join(".")));
    }
    Err(format!(
        "evidence JSON field is not a u64: {}",
        path.join(".")
    ))
}

pub(crate) fn json_array_len(value: &serde_json::Value, path: &[&str]) -> Result<usize, String> {
    json_at(value, path)?
        .as_array()
        .map(Vec::len)
        .ok_or_else(|| format!("evidence JSON field is not an array: {}", path.join(".")))
}

pub(crate) fn json_array<'a>(
    value: &'a serde_json::Value,
    path: &[&str],
) -> Result<&'a Vec<serde_json::Value>, String> {
    json_at(value, path)?
        .as_array()
        .ok_or_else(|| format!("evidence JSON field is not an array: {}", path.join(".")))
}

pub(crate) fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

pub(crate) fn normalize_download_url(value: &str) -> Result<String, String> {
    let normalized = value.trim().to_string();
    if normalized.starts_with("https://") || normalized.starts_with("http://") {
        Ok(normalized)
    } else {
        Err("expected --url to start with http:// or https://".to_string())
    }
}

pub(crate) fn normalize_sha256(value: &str) -> Result<String, String> {
    let normalized = value.trim().to_ascii_lowercase();
    if normalized.len() != 64 || !normalized.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("expected --sha256 to be a 64-character hex digest".to_string());
    }
    Ok(normalized)
}

pub(crate) fn sha256_file(path: &std::path::Path) -> Result<String, String> {
    sha256_file_with_bytes(path, "boot media").map(|(sha256, _)| sha256)
}

pub(crate) fn run_backend(
    store: &VmStore,
    name: &str,
    spawn: bool,
) -> Result<RunnerMetadata, String> {
    let (bundle, mut manifest, _) = store
        .get_vm_with_active_disk(name)
        .map_err(|error| error.to_string())?;
    let runtime_engine = CurrentRuntimeEngine::for_manifest(&manifest);

    let (disk, active_disk) = store
        .prepare_active_disk(name)
        .map_err(|error| error.to_string())?;
    apply_active_disk_to_manifest(&mut manifest, &active_disk);
    if runtime_engine != CurrentRuntimeEngine::AppleVz && spawn && !disk.exists {
        return Err(missing_disk_message(&disk));
    }

    if runtime_engine == CurrentRuntimeEngine::AppleVz {
        // Gated REAL cold-start launch: when `BRIDGEVM_APPLE_VZ_RUNNER` is set
        // and the caller asked to spawn, boot a real Apple VZ VM via
        // `lightvm-runner` (fresh boot, no saved-state restore). When the env
        // is unset, preserve the legacy dry-run + runner-required fallback so
        // all existing metadata-safe smokes/tests stay green.
        if spawn && apple_vz_runner_configured() {
            return spawn_fast_backend(store, name, &bundle, &manifest, None, false, None);
        }
        let plan = build_fast_plan(&manifest, &bundle).map_err(|error| error.to_string())?;
        let launch_spec_path = write_launch_spec_artifact(&bundle, plan.launch_spec())
            .map_err(|error| error.to_string())?;
        let mut readiness = launch_readiness_metadata(&plan.launch_spec().readiness);
        if spawn {
            add_fast_spawn_runner_required_blocker(&mut readiness);
        }
        let spawn_error = spawn.then(|| fast_spawn_runner_required_error(&readiness));
        let metadata = RunnerMetadata {
            engine: runtime_engine.runner_metadata_engine().to_string(),
            pid: None,
            command: plan.render_runner_words_for_launch_spec(&launch_spec_path),
            log_path: plan.launch_spec().logs.runner_log_path.clone().into(),
            started_at_unix: now_unix(),
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
            .map_err(|error| error.to_string())?;
        if let Some(error) = spawn_error {
            return Err(error);
        }
        return Ok(metadata);
    }

    let mut command = build_compatibility_command(&manifest, &bundle)
        .map_err(compatibility_qemu_command_error)?;
    let readiness = compatibility_launch_readiness_metadata(
        &disk,
        compatibility_launch_dependency_blockers(&manifest, &bundle),
    );
    if spawn && !readiness.ready {
        return Err(compatibility_launch_readiness_blocker_summary(&readiness));
    }
    let log_path = bundle.join("logs").join("qemu.log");
    let guest_tools = store
        .guest_tools_runner_metadata(name)
        .map_err(|error| error.to_string())?;

    if !spawn {
        let metadata = RunnerMetadata {
            engine: runtime_engine.runner_metadata_engine().to_string(),
            pid: None,
            command: command.render_shell_words(),
            log_path,
            started_at_unix: now_unix(),
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
            .map_err(|error| error.to_string())?;
        return Ok(metadata);
    }

    // Pin this VM to a free VNC display before recording + spawning, so two
    // Compat VMs running at once don't collide on TCP 5900. (The dry-run path
    // above keeps the deterministic vnc=:0 template since it binds nothing.)
    // Daemon-less launches probe the live ports only; the daemon additionally
    // avoids displays it has already handed to its own children.
    assign_free_vnc_display(&mut command, &[])?;
    fs::create_dir_all(bundle.join("logs")).map_err(|error| error.to_string())?;
    let stdout = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|error| error.to_string())?;
    let stderr = stdout.try_clone().map_err(|error| error.to_string())?;
    let child = Command::new(&command.program)
        .args(&command.args)
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr))
        .spawn()
        .map_err(|error| format!("failed to spawn {}: {error}", command.program))?;
    let metadata = RunnerMetadata {
        engine: runtime_engine.runner_metadata_engine().to_string(),
        pid: Some(child.id()),
        command: command.render_shell_words(),
        log_path,
        started_at_unix: now_unix(),
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
        .map_err(|error| error.to_string())?;
    store
        .transition_state(name, VmRuntimeState::Running)
        .map_err(|error| error.to_string())?;
    Ok(metadata)
}

pub(crate) fn compatibility_qemu_command_error(error: QemuError) -> String {
    format!("failed to build Compatibility Mode QEMU command: {error}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::*;

    #[test]
    fn fast_spawn_without_runner_env_returns_runner_required_error() {
        let _guard = APPLE_VZ_RUNNER_ENV_LOCK.lock().unwrap();
        let _env = EnvVarGuard::capture("BRIDGEVM_APPLE_VZ_RUNNER");
        std::env::remove_var("BRIDGEVM_APPLE_VZ_RUNNER");

        let (store, name) = fast_test_store("fast-spawn-no-env");
        assert!(!apple_vz_runner_configured());

        let error = run_backend(&store, &name, true)
            .expect_err("Fast spawn without BRIDGEVM_APPLE_VZ_RUNNER must stay blocked");
        assert!(
            error.contains("Fast Mode spawn requires BRIDGEVM_APPLE_VZ_RUNNER"),
            "unexpected error: {error}"
        );
        assert!(
            error.contains("apple-vz-runner-unavailable"),
            "unexpected error: {error}"
        );

        // Back-compat: dry-run runner metadata is still written.
        let metadata = store
            .runner_metadata(&name)
            .unwrap()
            .expect("dry-run runner metadata is written when the env is unset");
        assert!(metadata.dry_run);
        assert!(metadata.pid.is_none());
        assert_eq!(metadata.engine, "lightvm");

        let _ = std::fs::remove_dir_all(store.root());
    }
}
