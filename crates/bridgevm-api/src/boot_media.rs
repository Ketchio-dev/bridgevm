//! Split out of lib.rs by responsibility.

use crate::*;

pub fn import_boot_media(
    store: &VmStore,
    name: &str,
    source: PathBuf,
    kind: Option<BootMediaKind>,
) -> Result<BootMediaImportMetadata, String> {
    let source_metadata = fs::metadata(&source)
        .map_err(|error| format!("failed to read source media {}: {error}", source.display()))?;
    if !source_metadata.is_file() {
        return Err(format!("source media is not a file: {}", source.display()));
    }

    let (bundle, manifest, _) = store
        .get_vm_with_active_disk(name)
        .map_err(|error| error.to_string())?;
    let plan = build_fast_plan(&manifest, &bundle).map_err(|error| error.to_string())?;
    let (kind, destination) = boot_media_destination(&plan.launch_spec().boot, kind)?;
    ensure_boot_media_write_destination_in_bundle(&bundle, &destination, kind)?;
    let imported_at_unix = now_unix();
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create boot media directory {}: {error}",
                parent.display()
            )
        })?;
    }
    let replaced = destination.exists();
    if source == destination {
        let metadata = BootMediaImportMetadata {
            vm: name.to_string(),
            kind,
            source,
            destination,
            bytes: source_metadata.len(),
            replaced,
            imported_at_unix,
        };
        write_boot_media_import_metadata(&bundle, &metadata)?;
        return Ok(metadata);
    }
    let bytes = fs::copy(&source, &destination).map_err(|error| {
        format!(
            "failed to copy boot media from {} to {}: {error}",
            source.display(),
            destination.display()
        )
    })?;
    let metadata = BootMediaImportMetadata {
        vm: name.to_string(),
        kind,
        source,
        destination,
        bytes,
        replaced,
        imported_at_unix,
    };
    write_boot_media_import_metadata(&bundle, &metadata)?;
    Ok(metadata)
}

pub fn inspect_boot_media_status(store: &VmStore, name: &str) -> Result<BootMediaStatus, String> {
    let (bundle, manifest, _) = store
        .get_vm_with_active_disk(name)
        .map_err(|error| error.to_string())?;
    let plan = build_fast_plan(&manifest, &bundle).map_err(|error| error.to_string())?;
    let mut entries = Vec::new();
    push_boot_media_status_entry(
        &mut entries,
        &bundle,
        BootMediaKind::InstallerImage,
        plan.launch_spec().boot.installer_image.as_ref(),
    )?;
    push_boot_media_status_entry(
        &mut entries,
        &bundle,
        BootMediaKind::Kernel,
        plan.launch_spec().boot.kernel.as_ref(),
    )?;
    push_boot_media_status_entry(
        &mut entries,
        &bundle,
        BootMediaKind::Initrd,
        plan.launch_spec().boot.initrd.as_ref(),
    )?;
    push_boot_media_status_entry(
        &mut entries,
        &bundle,
        BootMediaKind::MacosRestoreImage,
        plan.launch_spec().boot.macos_restore_image.as_ref(),
    )?;
    Ok(BootMediaStatus {
        vm: name.to_string(),
        entries,
    })
}

pub fn verify_boot_media(
    store: &VmStore,
    name: &str,
    expected_sha256: &str,
    kind: Option<BootMediaKind>,
) -> Result<BootMediaVerificationMetadata, String> {
    let expected_sha256 = normalize_sha256(expected_sha256)?;
    let (bundle, manifest, _) = store
        .get_vm_with_active_disk(name)
        .map_err(|error| error.to_string())?;
    let plan = build_fast_plan(&manifest, &bundle).map_err(|error| error.to_string())?;
    let (kind, path) = boot_media_destination(&plan.launch_spec().boot, kind)?;
    let file_metadata = fs::metadata(&path)
        .map_err(|error| format!("failed to read boot media {}: {error}", path.display()))?;
    if !file_metadata.is_file() {
        return Err(format!("boot media is not a file: {}", path.display()));
    }
    let actual_sha256 = sha256_file(&path)?;
    let verified = actual_sha256 == expected_sha256;
    let verification = BootMediaVerificationMetadata {
        vm: name.to_string(),
        kind,
        path,
        bytes: file_metadata.len(),
        expected_sha256,
        actual_sha256,
        verified,
        verified_at_unix: now_unix(),
    };
    write_boot_media_verification_metadata(&bundle, &verification)?;
    if !verification.verified {
        return Err(format!(
            "boot media SHA-256 mismatch for {}: expected {}, got {}",
            verification.path.display(),
            verification.expected_sha256,
            verification.actual_sha256
        ));
    }
    Ok(verification)
}

pub fn plan_boot_media_download(
    store: &VmStore,
    name: &str,
    url: &str,
    expected_sha256: Option<&str>,
    kind: Option<BootMediaKind>,
) -> Result<BootMediaDownloadPlanMetadata, String> {
    let url = normalize_download_url(url)?;
    let expected_sha256 = expected_sha256.map(normalize_sha256).transpose()?;
    let (bundle, manifest, _) = store
        .get_vm_with_active_disk(name)
        .map_err(|error| error.to_string())?;
    let plan = build_fast_plan(&manifest, &bundle).map_err(|error| error.to_string())?;
    let (kind, destination) = boot_media_destination(&plan.launch_spec().boot, kind)?;
    ensure_boot_media_write_destination_in_bundle(&bundle, &destination, kind)?;
    let file_metadata = fs::metadata(&destination).ok();
    let exists = file_metadata
        .as_ref()
        .is_some_and(std::fs::Metadata::is_file);
    let bytes = file_metadata
        .filter(std::fs::Metadata::is_file)
        .map(|metadata| metadata.len());
    let metadata = BootMediaDownloadPlanMetadata {
        vm: name.to_string(),
        kind,
        url,
        destination,
        exists,
        bytes,
        expected_sha256,
        last_import: read_boot_media_import_metadata(&bundle, kind)?,
        last_verification: read_boot_media_verification_metadata(&bundle, kind)?,
        planned_at_unix: now_unix(),
    };
    write_boot_media_download_plan_metadata(&bundle, &metadata)?;
    Ok(metadata)
}

pub fn download_boot_media(
    store: &VmStore,
    name: &str,
    kind: Option<BootMediaKind>,
) -> Result<BootMediaDownloadResultMetadata, String> {
    let (bundle, manifest, _) = store
        .get_vm_with_active_disk(name)
        .map_err(|error| error.to_string())?;
    let plan = build_fast_plan(&manifest, &bundle).map_err(|error| error.to_string())?;
    let (kind, destination) = boot_media_destination(&plan.launch_spec().boot, kind)?;
    ensure_boot_media_write_destination_in_bundle(&bundle, &destination, kind)?;
    let download_plan = read_boot_media_download_plan_metadata(&bundle, kind)?
        .ok_or_else(|| format!("no download plan recorded for boot media kind {kind}"))?;
    if download_plan.destination != destination {
        return Err(format!(
            "download plan destination {} does not match current resolved destination {}",
            download_plan.destination.display(),
            destination.display()
        ));
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create boot media directory {}: {error}",
                parent.display()
            )
        })?;
    }
    let temp_path = boot_media_download_temp_path(&destination);
    if temp_path.exists() {
        fs::remove_file(&temp_path).map_err(|error| {
            format!(
                "failed to remove stale download temp file {}: {error}",
                temp_path.display()
            )
        })?;
    }
    let command = vec![
        "curl".to_string(),
        "--location".to_string(),
        "--fail".to_string(),
        "--silent".to_string(),
        "--show-error".to_string(),
        "--connect-timeout".to_string(),
        BOOT_MEDIA_CURL_CONNECT_TIMEOUT_SECS.to_string(),
        "--max-time".to_string(),
        BOOT_MEDIA_CURL_MAX_TIME_SECS.to_string(),
        "--speed-time".to_string(),
        BOOT_MEDIA_CURL_SPEED_TIME_SECS.to_string(),
        "--speed-limit".to_string(),
        BOOT_MEDIA_CURL_SPEED_LIMIT_BYTES.to_string(),
        "--output".to_string(),
        temp_path.display().to_string(),
        download_plan.url.clone(),
    ];
    let output = run_boot_media_curl(&temp_path, &download_plan.url)?;
    let replaced = destination.exists();
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    if !output.status.success() {
        let metadata = BootMediaDownloadResultMetadata {
            vm: name.to_string(),
            kind,
            url: download_plan.url,
            destination,
            temp_path,
            command,
            exit_status: output.status.code(),
            stdout,
            stderr,
            bytes: None,
            replaced,
            expected_sha256: download_plan.expected_sha256,
            actual_sha256: None,
            verified: None,
            downloaded: false,
            downloaded_at_unix: now_unix(),
        };
        write_boot_media_download_result_metadata(&bundle, &metadata)?;
        return Err(format!(
            "boot media download failed with status {}",
            metadata
                .exit_status
                .map_or("unknown".to_string(), |status| status.to_string())
        ));
    }

    let actual_sha256 = sha256_file(&temp_path)?;
    let verified = download_plan
        .expected_sha256
        .as_ref()
        .map(|expected| expected == &actual_sha256);
    if verified == Some(false) {
        let bytes = fs::metadata(&temp_path).ok().map(|metadata| metadata.len());
        let metadata = BootMediaDownloadResultMetadata {
            vm: name.to_string(),
            kind,
            url: download_plan.url,
            destination,
            temp_path,
            command,
            exit_status: output.status.code(),
            stdout,
            stderr,
            bytes,
            replaced,
            expected_sha256: download_plan.expected_sha256,
            actual_sha256: Some(actual_sha256),
            verified,
            downloaded: false,
            downloaded_at_unix: now_unix(),
        };
        write_boot_media_download_result_metadata(&bundle, &metadata)?;
        return Err(format!(
            "downloaded boot media SHA-256 mismatch for {}",
            metadata.destination.display()
        ));
    }

    fs::rename(&temp_path, &destination).map_err(|error| {
        format!(
            "failed to move downloaded boot media from {} to {}: {error}",
            temp_path.display(),
            destination.display()
        )
    })?;
    let bytes = fs::metadata(&destination)
        .ok()
        .map(|metadata| metadata.len());
    let metadata = BootMediaDownloadResultMetadata {
        vm: name.to_string(),
        kind,
        url: download_plan.url,
        destination,
        temp_path,
        command,
        exit_status: output.status.code(),
        stdout,
        stderr,
        bytes,
        replaced,
        expected_sha256: download_plan.expected_sha256,
        actual_sha256: Some(actual_sha256),
        verified,
        downloaded: true,
        downloaded_at_unix: now_unix(),
    };
    write_boot_media_download_result_metadata(&bundle, &metadata)?;
    Ok(metadata)
}

pub(crate) struct BoundedProcessOutput {
    pub(crate) status: std::process::ExitStatus,
    pub(crate) stdout: Vec<u8>,
    pub(crate) stderr: Vec<u8>,
}

pub(crate) fn run_boot_media_curl(
    temp_path: &Path,
    url: &str,
) -> Result<BoundedProcessOutput, String> {
    let mut child = Command::new("curl")
        .args([
            "--location",
            "--fail",
            "--silent",
            "--show-error",
            "--connect-timeout",
            &BOOT_MEDIA_CURL_CONNECT_TIMEOUT_SECS.to_string(),
            "--max-time",
            &BOOT_MEDIA_CURL_MAX_TIME_SECS.to_string(),
            "--speed-time",
            &BOOT_MEDIA_CURL_SPEED_TIME_SECS.to_string(),
            "--speed-limit",
            &BOOT_MEDIA_CURL_SPEED_LIMIT_BYTES.to_string(),
            "--output",
        ])
        .arg(temp_path)
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("failed to execute curl: {error}"))?;
    let mut stdout = child.stdout.take().ok_or("failed to capture curl stdout")?;
    let mut stderr = child.stderr.take().ok_or("failed to capture curl stderr")?;
    let stdout_drain =
        thread::spawn(move || drain_process_stream(&mut stdout, BOOT_MEDIA_CURL_OUTPUT_BYTES));
    let stderr_drain =
        thread::spawn(move || drain_process_stream(&mut stderr, BOOT_MEDIA_CURL_OUTPUT_BYTES));
    let deadline = Instant::now() + Duration::from_secs(BOOT_MEDIA_CURL_MAX_TIME_SECS + 30);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(100)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = stdout_drain.join();
                let _ = stderr_drain.join();
                return Err(format!(
                    "boot media curl timed out after {} seconds",
                    BOOT_MEDIA_CURL_MAX_TIME_SECS + 30
                ));
            }
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = stdout_drain.join();
                let _ = stderr_drain.join();
                return Err(format!("failed to wait for curl: {error}"));
            }
        }
    };
    let (stdout, stdout_exceeded) = join_process_stream(stdout_drain, "stdout")?;
    let (stderr, stderr_exceeded) = join_process_stream(stderr_drain, "stderr")?;
    if stdout_exceeded || stderr_exceeded {
        return Err(format!(
            "curl {} exceeded the {}-byte output limit",
            if stdout_exceeded { "stdout" } else { "stderr" },
            BOOT_MEDIA_CURL_OUTPUT_BYTES
        ));
    }
    Ok(BoundedProcessOutput {
        status,
        stdout,
        stderr,
    })
}

pub(crate) fn drain_process_stream(
    reader: &mut impl Read,
    output_limit: usize,
) -> std::io::Result<(Vec<u8>, bool)> {
    let mut captured = Vec::new();
    let mut chunk = [0_u8; 8192];
    let mut exceeded = false;
    loop {
        let read = reader.read(&mut chunk)?;
        if read == 0 {
            break;
        }
        let keep = read.min(output_limit.saturating_sub(captured.len()));
        captured.extend_from_slice(&chunk[..keep]);
        exceeded |= keep < read;
    }
    Ok((captured, exceeded))
}

pub(crate) fn join_process_stream(
    drain: thread::JoinHandle<std::io::Result<(Vec<u8>, bool)>>,
    stream: &str,
) -> Result<(Vec<u8>, bool), String> {
    drain
        .join()
        .map_err(|_| format!("curl {stream} drain panicked"))?
        .map_err(|error| format!("failed to read curl {stream}: {error}"))
}

pub(crate) fn boot_media_destination(
    boot: &AppleVzBootSpec,
    requested: Option<BootMediaKind>,
) -> Result<(BootMediaKind, PathBuf), String> {
    let mut candidates = Vec::new();
    push_boot_media_candidate(
        &mut candidates,
        BootMediaKind::InstallerImage,
        boot.installer_image.as_ref(),
    );
    push_boot_media_candidate(&mut candidates, BootMediaKind::Kernel, boot.kernel.as_ref());
    push_boot_media_candidate(&mut candidates, BootMediaKind::Initrd, boot.initrd.as_ref());
    push_boot_media_candidate(
        &mut candidates,
        BootMediaKind::MacosRestoreImage,
        boot.macos_restore_image.as_ref(),
    );

    if let Some(requested) = requested {
        return candidates
            .into_iter()
            .find(|(kind, _)| *kind == requested)
            .ok_or_else(|| format!("boot media kind {requested} is not present in this VM plan"));
    }

    match candidates.len() {
        0 => Err("no importable boot media path is present in this VM plan".to_string()),
        1 => Ok(candidates.remove(0)),
        _ => Err(
            "multiple boot media paths are present; pass --kind to choose which one to import"
                .to_string(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boot_media_process_drain_caps_capture_and_consumes_to_eof() {
        let input = vec![0x41; BOOT_MEDIA_CURL_OUTPUT_BYTES * 2];
        let mut reader = std::io::Cursor::new(input.clone());

        let (captured, exceeded) =
            drain_process_stream(&mut reader, BOOT_MEDIA_CURL_OUTPUT_BYTES).unwrap();

        assert!(exceeded);
        assert_eq!(captured, input[..BOOT_MEDIA_CURL_OUTPUT_BYTES]);
        assert_eq!(reader.position(), input.len() as u64);
    }
}
