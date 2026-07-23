//! Split out of lib.rs by responsibility.

use crate::*;

pub(crate) fn relative_evidence_artifact_path(
    root: &Path,
    artifact: &str,
    label: &str,
) -> Result<PathBuf, String> {
    if artifact.trim().is_empty() {
        return Err(format!("{label} path is empty"));
    }
    let artifact_path = Path::new(artifact);
    if artifact_path.is_absolute()
        || artifact_path.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir | std::path::Component::Prefix(_)
            )
        })
    {
        return Err(format!(
            "{label} path must be relative and stay inside the evidence bundle"
        ));
    }
    let full_path = root.join(artifact_path);
    let metadata = fs::symlink_metadata(&full_path).map_err(|error| {
        format!(
            "{label} artifact is not a file: {} ({error})",
            full_path.display()
        )
    })?;
    if metadata.file_type().is_symlink() {
        return Err(format!(
            "{label} artifact must not be a symlink: {}",
            full_path.display()
        ));
    }
    if !metadata.is_file() {
        return Err(format!(
            "{label} artifact is not a file: {}",
            full_path.display()
        ));
    }
    Ok(full_path)
}

pub(crate) fn command_option_value<'a>(
    args: &'a [&str],
    option: &str,
    label: &str,
) -> Result<&'a str, String> {
    let index = args
        .iter()
        .position(|arg| *arg == option)
        .ok_or_else(|| format!("{label} is missing {option}"))?;
    args.get(index + 1)
        .copied()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("{label} {option} value is empty"))
}

pub(crate) fn verify_viewer_evidence(root: &Path) -> Result<bool, String> {
    verify_graphical_png_evidence(root, "viewer-evidence.json", "graphical-viewer")
        .map(|evidence| evidence.is_some())
}

pub(crate) fn verify_graphical_boot_progress_evidence(root: &Path) -> Result<bool, String> {
    let Some(evidence) = verify_graphical_png_evidence(
        root,
        "boot-progress-evidence.json",
        "graphical-boot-progress",
    )?
    else {
        return Ok(false);
    };

    let stage = json_string(&evidence, &["stage"])?;
    if stage.trim().is_empty() {
        return Err("boot-progress-evidence.json stage is empty".to_string());
    }
    let progress_marker = json_string(&evidence, &["progress_marker"])?;
    if progress_marker.trim().is_empty() {
        return Err("boot-progress-evidence.json progress_marker is empty".to_string());
    }

    Ok(true)
}

pub(crate) fn verify_graphical_png_evidence(
    root: &Path,
    file_name: &str,
    expected_kind: &str,
) -> Result<Option<serde_json::Value>, String> {
    let evidence_path = root.join(file_name);
    if !evidence_path.exists() {
        return Ok(None);
    }

    let evidence = read_evidence_json(root, file_name)?;
    if !json_bool(&evidence, &["proven"])? {
        return Err(format!(
            "{file_name} does not mark graphical evidence as proven"
        ));
    }
    let kind = json_string(&evidence, &["kind"])?;
    if kind != expected_kind {
        return Err(format!("{file_name} kind is not {expected_kind}: {kind}"));
    }
    let artifact = json_string(&evidence, &["artifact"])?;
    let full_artifact_path = relative_evidence_artifact_path(root, &artifact, file_name)?;
    let (actual_sha256, bytes) =
        sha256_file_with_bytes(&full_artifact_path, &format!("{file_name} artifact"))?;
    if bytes == 0 {
        return Err(format!("{file_name} artifact is empty"));
    }
    let expected_sha256 = json_string(&evidence, &["sha256"])?;
    if !is_sha256_hex(&expected_sha256) {
        return Err(format!("{file_name} sha256 is not lowercase SHA-256 hex"));
    }
    if actual_sha256 != expected_sha256 {
        return Err(format!("{file_name} sha256 does not match artifact"));
    }
    let width = json_u64(&evidence, &["width"])?;
    let height = json_u64(&evidence, &["height"])?;
    if width == 0 || height == 0 {
        return Err(format!("{file_name} width and height must be nonzero"));
    }
    let mut header = [0u8; 24];
    fs::File::open(&full_artifact_path)
        .and_then(|mut file| file.read_exact(&mut header))
        .map_err(|error| {
            format!(
                "failed to read {file_name} artifact header {}: {error}",
                full_artifact_path.display()
            )
        })?;
    let (actual_width, actual_height) = png_dimensions(&header)
        .ok_or_else(|| format!("{file_name} artifact is not a PNG image"))?;
    if actual_width != width || actual_height != height {
        return Err(format!(
            "{file_name} width and height do not match artifact pixels"
        ));
    }
    let observation = json_string(&evidence, &["observation"])?;
    if observation.trim().is_empty() {
        return Err(format!("{file_name} observation is empty"));
    }

    Ok(Some(evidence))
}

pub(crate) fn png_dimensions(bytes: &[u8]) -> Option<(u64, u64)> {
    const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";
    if bytes.len() < 24 || &bytes[..8] != PNG_SIGNATURE {
        return None;
    }
    if &bytes[12..16] != b"IHDR" {
        return None;
    }
    let width = u32::from_be_bytes(bytes[16..20].try_into().ok()?) as u64;
    let height = u32::from_be_bytes(bytes[20..24].try_into().ok()?) as u64;
    if width == 0 || height == 0 {
        return None;
    }
    Some((width, height))
}

pub(crate) fn verify_guest_tools_effects_evidence(root: &Path) -> Result<bool, String> {
    let evidence_path = root.join("guest-tools-effects.json");
    if !evidence_path.exists() {
        return Ok(false);
    }

    let evidence = read_evidence_json(root, "guest-tools-effects.json")?;
    if !json_bool(&evidence, &["proven"])? {
        return Err("guest-tools-effects.json does not mark effects as proven".to_string());
    }
    let backend = json_string(&evidence, &["backend"])?;
    if backend != "bridgevm-tools-linux" {
        return Err(format!(
            "guest-tools-effects.json backend is not bridgevm-tools-linux: {backend}"
        ));
    }
    let command_request_id = json_string(&evidence, &["command", "request_id"])?;
    if command_request_id.trim().is_empty() {
        return Err("guest-tools-effects.json command request_id is empty".to_string());
    }
    let command_status = json_string(&evidence, &["command", "status"])?;
    if command_status != "ok" {
        return Err(format!(
            "guest-tools-effects.json command status is not ok: {command_status}"
        ));
    }
    let effects = json_array(&evidence, &["effects"])?;
    if effects.is_empty() {
        return Err("guest-tools-effects.json has no effect records".to_string());
    }

    let mut artifact_backed_effects = 0usize;
    for (index, effect) in effects.iter().enumerate() {
        let label = format!("guest-tools-effects.json effects[{index}]");
        let kind = json_string(effect, &["kind"])?;
        if kind.trim().is_empty() {
            return Err(format!("{label} has an empty kind"));
        }
        if !json_bool(effect, &["ok"])? {
            return Err(format!("{label} is not ok"));
        }
        let request_id = json_string(effect, &["request_id"])?;
        if request_id.trim().is_empty() {
            return Err(format!("{label} has an empty request_id"));
        }
        if request_id != command_request_id {
            return Err(format!("{label} request_id does not match command"));
        }
        let observation = json_string(effect, &["observation"])?;
        if observation.trim().is_empty() {
            return Err(format!("{label} has an empty observation"));
        }
        if verify_guest_tools_effect_observable(root, effect, &label)? {
            artifact_backed_effects += 1;
        }
    }
    if artifact_backed_effects == 0 {
        return Err(
            "guest-tools-effects.json needs at least one artifact/sha256-backed effect".to_string(),
        );
    }

    Ok(true)
}

pub(crate) fn verify_guest_tools_effect_observable(
    root: &Path,
    effect: &serde_json::Value,
    label: &str,
) -> Result<bool, String> {
    let expected_value = effect
        .get("expected_value")
        .and_then(serde_json::Value::as_str);
    let observed_value = effect
        .get("observed_value")
        .and_then(serde_json::Value::as_str);
    if let (Some(expected), Some(observed)) = (expected_value, observed_value) {
        if expected.trim().is_empty() {
            return Err(format!("{label} expected_value is empty"));
        }
        if observed != expected {
            return Err(format!(
                "{label} observed_value does not match expected_value"
            ));
        }
        return Ok(false);
    }

    let artifact = effect
        .get("artifact")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    let sha256 = effect
        .get("sha256")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    if !artifact.trim().is_empty() || !sha256.trim().is_empty() {
        if artifact.trim().is_empty() {
            return Err(format!("{label} artifact is empty"));
        }
        if !is_sha256_hex(sha256) {
            return Err(format!("{label} sha256 is not lowercase SHA-256 hex"));
        }
        let artifact_path =
            evidence_bundle_file_path(root, artifact, &format!("{label} artifact"))?;
        let (actual_sha256, _) =
            sha256_file_with_bytes(&artifact_path, &format!("{label} artifact"))?;
        if actual_sha256 != sha256 {
            return Err(format!("{label} sha256 does not match artifact"));
        }
        return Ok(true);
    }

    Err(format!(
        "{label} needs expected_value/observed_value or artifact/sha256 evidence"
    ))
}

pub(crate) fn read_evidence_text(root: &Path, name: &str) -> Result<String, String> {
    read_bounded_text_file(&root.join(name), &format!("evidence {name}"))
}

pub(crate) fn read_optional_evidence_text(
    root: &Path,
    name: &str,
) -> Result<Option<String>, String> {
    let path = root.join(name);
    if !path.exists() {
        return Ok(None);
    }
    read_bounded_text_file(&path, &format!("evidence {name}")).map(Some)
}

pub(crate) fn read_bounded_text_file(path: &Path, label: &str) -> Result<String, String> {
    let mut bytes = Vec::new();
    fs::File::open(path)
        .and_then(|file| {
            file.take(MAX_EVIDENCE_TEXT_BYTES + 1)
                .read_to_end(&mut bytes)
        })
        .map_err(|error| format!("failed to read {label} {}: {error}", path.display()))?;
    if bytes.len() as u64 > MAX_EVIDENCE_TEXT_BYTES {
        return Err(format!(
            "{label} {} exceeds the {MAX_EVIDENCE_TEXT_BYTES}-byte limit",
            path.display()
        ));
    }
    String::from_utf8(bytes)
        .map_err(|error| format!("{label} {} is not valid UTF-8: {error}", path.display()))
}

pub(crate) fn sha256_file_with_bytes(path: &Path, label: &str) -> Result<(String, u64), String> {
    let mut file = fs::File::open(path)
        .map_err(|error| format!("failed to read {label} {}: {error}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    let mut bytes = 0u64;
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("failed to read {label} {}: {error}", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        bytes = bytes
            .checked_add(read as u64)
            .ok_or_else(|| format!("{label} {} size overflow", path.display()))?;
    }
    Ok((format!("{:x}", hasher.finalize()), bytes))
}

pub(crate) fn read_evidence_json(root: &Path, name: &str) -> Result<serde_json::Value, String> {
    let content = read_evidence_text(root, name)?;
    serde_json::from_str(&content).map_err(|error| format!("invalid evidence JSON {name}: {error}"))
}

pub(crate) fn require_contains(content: &str, needle: &str, label: &str) -> Result<(), String> {
    if content.contains(needle) {
        Ok(())
    } else {
        Err(format!("{label} missing {needle:?}"))
    }
}

pub(crate) fn parse_environment_values(
    content: &str,
) -> std::collections::BTreeMap<String, String> {
    content
        .lines()
        .filter_map(|line| line.split_once('='))
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evidence_artifact_hashes_across_streaming_chunks() {
        let path = std::env::temp_dir().join(format!(
            "bridgevm-api-streaming-evidence-hash-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let content: Vec<u8> = (0..200_000).map(|index| (index % 251) as u8).collect();
        fs::write(&path, &content).unwrap();

        let (actual, bytes) = sha256_file_with_bytes(&path, "test artifact").unwrap();
        assert_eq!(bytes, content.len() as u64);
        assert_eq!(actual, format!("{:x}", Sha256::digest(&content)));

        let _ = fs::remove_file(path);
    }
}
