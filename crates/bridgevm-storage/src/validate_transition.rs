//! Split out of lib.rs to keep files under 800 lines.

use crate::*;
use bridgevm_config::slug;
use bridgevm_config::VmManifest;
use bridgevm_qemu::QemuImgCommand;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::env;
use std::ffi::OsStr;
use std::fs;
use std::io::Read;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::process::Output;
use std::process::Stdio;
use std::thread;
use std::thread::sleep;
use std::time::Duration;
use std::time::Instant;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

pub(crate) fn validate_transition(
    from: VmRuntimeState,
    to: VmRuntimeState,
) -> Result<(), StorageError> {
    let valid = matches!(
        (from, to),
        (VmRuntimeState::Stopped, VmRuntimeState::Running)
            | (VmRuntimeState::Running, VmRuntimeState::Stopped)
            | (VmRuntimeState::Running, VmRuntimeState::Suspended)
            | (VmRuntimeState::Suspended, VmRuntimeState::Running)
            | (VmRuntimeState::Suspended, VmRuntimeState::Stopped)
    ) || from == to;

    if valid {
        Ok(())
    } else {
        Err(StorageError::InvalidStateTransition { from, to })
    }
}

pub(crate) fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub(crate) fn duration_micros_u64(duration: Duration) -> u64 {
    duration.as_micros().min(u128::from(u64::MAX)) as u64
}

pub(crate) fn read_json_file<T: DeserializeOwned>(path: &Path) -> Result<Option<T>, StorageError> {
    if !path.exists() {
        return Ok(None);
    }
    Ok(Some(read_json_required(path)?))
}

pub(crate) const MAX_METADATA_JSON_BYTES: u64 = 16 * 1024 * 1024;

pub(crate) fn read_json_required<T: DeserializeOwned>(path: &Path) -> Result<T, StorageError> {
    let file = fs::File::open(path)?;
    let size = file.metadata()?.len();
    if size > MAX_METADATA_JSON_BYTES {
        return Err(StorageError::MetadataTooLarge {
            path: path.to_path_buf(),
            actual: size,
            maximum: MAX_METADATA_JSON_BYTES,
        });
    }
    let capacity = usize::try_from(size).map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "metadata size exceeds host address space",
        )
    })?;
    let mut bytes = Vec::with_capacity(capacity);
    file.take(MAX_METADATA_JSON_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_METADATA_JSON_BYTES {
        return Err(StorageError::MetadataTooLarge {
            path: path.to_path_buf(),
            actual: bytes.len() as u64,
            maximum: MAX_METADATA_JSON_BYTES,
        });
    }
    Ok(serde_json::from_slice(&bytes)?)
}

pub(crate) fn metadata_repair_action(
    path: &Path,
    action: impl Into<String>,
    detail: impl Into<String>,
) -> MetadataRepairAction {
    MetadataRepairAction {
        path: path.to_path_buf(),
        action: action.into(),
        detail: detail.into(),
    }
}

pub(crate) fn primary_disk_preparation_metadata(
    bundle: &Path,
    manifest: &VmManifest,
) -> DiskPreparationMetadata {
    let path = resolve_bundle_path(bundle, &manifest.storage.primary.path);
    let format = manifest.storage.primary.format.clone();
    let size = manifest.storage.primary.size.clone();
    let exists = path.exists();
    let create_command = if !exists && format != "raw" {
        Some(QemuImgCommand::create_disk(&path, format.clone(), size.clone()).render_shell_words())
    } else {
        None
    };
    DiskPreparationMetadata {
        path,
        format,
        size: size.clone(),
        size_bytes: parse_size_bytes(&size),
        exists,
        created: false,
        create_command,
        prepared_at_unix: now_unix(),
    }
}

pub(crate) fn new_guest_tools_token() -> Result<GuestToolsTokenMetadata, StorageError> {
    let mut bytes = [0_u8; 32];
    fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    Ok(GuestToolsTokenMetadata {
        token: hex_encode(&bytes),
        created_at_unix: now_unix(),
    })
}

pub(crate) fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 0x0f) as usize] as char);
    }
    encoded
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BundleCopySummary {
    pub(crate) file_count: u64,
    pub(crate) files: Vec<String>,
    pub(crate) manifest_preserved: bool,
    pub(crate) metadata_preserved: bool,
}

pub(crate) fn summarize_bundle_copy(from: &Path) -> Result<BundleCopySummary, StorageError> {
    let mut files = Vec::new();
    collect_regular_files(from, from, &mut files)?;
    files.sort();
    Ok(BundleCopySummary {
        file_count: files.len() as u64,
        manifest_preserved: files.iter().any(|path| path == "manifest.yaml"),
        metadata_preserved: files.iter().any(|path| path.starts_with("metadata/")),
        files,
    })
}

pub(crate) fn collect_regular_files(
    root: &Path,
    current: &Path,
    files: &mut Vec<String>,
) -> Result<(), StorageError> {
    let metadata = fs::symlink_metadata(current)?;
    if !metadata.file_type().is_dir() {
        return Err(StorageError::UnsupportedBundleEntry(current.to_path_buf()));
    }
    let mut entries = fs::read_dir(current)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        if should_skip_bundle_copy_path(&path) {
            continue;
        }
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            collect_regular_files(root, &path, files)?;
        } else if file_type.is_file() {
            let relative = path
                .strip_prefix(root)
                .map_err(|_| StorageError::UnsupportedBundleEntry(path.clone()))?;
            files.push(relative.to_string_lossy().replace('\\', "/"));
        } else {
            return Err(StorageError::UnsupportedBundleEntry(path));
        }
    }
    Ok(())
}

pub(crate) fn copy_dir_all(from: &Path, to: &Path) -> Result<BundleCopySummary, StorageError> {
    let metadata = fs::symlink_metadata(from)?;
    if !metadata.file_type().is_dir() {
        return Err(StorageError::UnsupportedBundleEntry(from.to_path_buf()));
    }
    fs::create_dir_all(to)?;
    let mut copied_files = Vec::new();
    copy_dir_all_inner(from, from, to, &mut copied_files)?;
    copied_files.sort();
    Ok(BundleCopySummary {
        file_count: copied_files.len() as u64,
        manifest_preserved: copied_files.iter().any(|path| path == "manifest.yaml"),
        metadata_preserved: copied_files
            .iter()
            .any(|path| path.starts_with("metadata/")),
        files: copied_files,
    })
}

pub(crate) fn copy_dir_all_inner(
    root: &Path,
    from: &Path,
    to: &Path,
    copied_files: &mut Vec<String>,
) -> Result<(), StorageError> {
    let mut entries = fs::read_dir(from)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let from_path = entry.path();
        if should_skip_bundle_copy_path(&from_path) {
            continue;
        }
        let to_path = to.join(entry.file_name());
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            fs::create_dir_all(&to_path)?;
            copy_dir_all_inner(root, &from_path, &to_path, copied_files)?;
        } else if file_type.is_file() {
            fs::copy(&from_path, &to_path)?;
            let relative = from_path
                .strip_prefix(root)
                .map_err(|_| StorageError::UnsupportedBundleEntry(from_path.clone()))?;
            copied_files.push(relative.to_string_lossy().replace('\\', "/"));
        } else {
            return Err(StorageError::UnsupportedBundleEntry(from_path));
        }
    }
    Ok(())
}

pub(crate) fn should_skip_bundle_copy_path(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.ends_with(".sock") || name.ends_with(".lock"))
}

pub(crate) fn rebase_copied_path(path: &Path, source: &Path, output: &Path) -> PathBuf {
    path.strip_prefix(source)
        .map(|relative| output.join(relative))
        .unwrap_or_else(|_| path.to_path_buf())
}

pub(crate) fn rebase_snapshot_disk_metadata(
    metadata: &mut SnapshotDiskMetadata,
    source: &Path,
    output: &Path,
) {
    metadata.overlay_path = rebase_copied_path(&metadata.overlay_path, source, output);
    metadata.backing_path = rebase_copied_path(&metadata.backing_path, source, output);
    metadata.overlay_exists = metadata.overlay_path.exists();
    metadata.backing_exists = metadata.backing_path.exists();
    metadata.create_command = QemuImgCommand::create_backed_disk(
        &metadata.overlay_path,
        metadata.overlay_format.clone(),
        metadata.backing_format.clone(),
        &metadata.backing_path,
    )
    .render_shell_words();
}

pub(crate) fn export_bundle_tar(
    source: &Path,
    output: &Path,
    metadata: &VmExportMetadata,
) -> Result<(), StorageError> {
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    let staging = unique_temp_path("bridgevm-export-tar");
    let _staging_guard = TempDirGuard::new(staging.clone());
    copy_dir_all(source, &staging)?;
    let metadata_dir = staging.join("metadata");
    fs::create_dir_all(&metadata_dir)?;
    fs::write(
        metadata_dir.join("export.json"),
        serde_json::to_string_pretty(metadata)?,
    )?;

    let file = fs::File::create(output)?;
    let mut builder = tar::Builder::new(file);
    builder.append_dir_all(".", &staging)?;
    builder.finish()?;
    Ok(())
}

pub(crate) fn extract_bundle_tar(input: &Path, output: &Path) -> Result<(), StorageError> {
    fs::create_dir_all(output)?;
    let file = fs::File::open(input)?;
    let mut archive = tar::Archive::new(file);
    for entry in archive.entries()? {
        let mut entry = entry?;
        let raw_path = entry.path()?.into_owned();
        let Some(relative_path) = safe_archive_path(&raw_path) else {
            return Err(StorageError::UnsafeArchiveEntry(raw_path));
        };
        if relative_path.as_os_str().is_empty() {
            continue;
        }
        let destination = output.join(&relative_path);
        let entry_type = entry.header().entry_type();
        if entry_type.is_dir() {
            fs::create_dir_all(&destination)?;
        } else if entry_type.is_file() {
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent)?;
            }
            entry.unpack(&destination)?;
        } else {
            return Err(StorageError::UnsupportedBundleEntry(raw_path));
        }
    }
    Ok(())
}

pub(crate) fn safe_archive_path(path: &Path) -> Option<PathBuf> {
    let mut safe = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::Normal(name) => safe.push(name),
            std::path::Component::ParentDir
            | std::path::Component::RootDir
            | std::path::Component::Prefix(_) => return None,
        }
    }
    Some(safe)
}

pub(crate) fn is_tar_path(path: &Path) -> bool {
    path.extension().and_then(|extension| extension.to_str()) == Some("tar")
}

pub(crate) fn is_unsupported_archive_path(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    matches!(
        path.extension().and_then(|extension| extension.to_str()),
        Some("zip" | "tgz" | "gz")
    ) || name.ends_with(".tar.gz")
}

pub(crate) fn unique_temp_path(prefix: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    std::env::temp_dir().join(format!("{prefix}-{}-{nanos}", std::process::id()))
}

pub(crate) struct TempDirGuard {
    pub(crate) path: PathBuf,
}

impl TempDirGuard {
    pub(crate) fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

pub(crate) fn resolve_path_for_new(path: &Path) -> Result<PathBuf, StorageError> {
    if path.exists() {
        return Ok(fs::canonicalize(path)?);
    }

    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        env::current_dir()?.join(path)
    };
    let mut existing = absolute.as_path();
    let mut missing = Vec::new();
    while !existing.exists() {
        if let Some(name) = existing.file_name() {
            missing.push(name.to_os_string());
        }
        existing = existing.parent().unwrap_or_else(|| Path::new("."));
    }

    let mut resolved = fs::canonicalize(existing)?;
    for component in missing.iter().rev() {
        if component == OsStr::new(".") {
            continue;
        }
        if component == OsStr::new("..") {
            resolved.pop();
        } else {
            resolved.push(component);
        }
    }
    Ok(resolved)
}

pub(crate) fn is_same_or_descendant(path: &Path, ancestor: &Path) -> bool {
    path == ancestor || path.starts_with(ancestor)
}

pub(crate) fn write_json_pretty_atomic<T: Serialize + ?Sized>(
    path: &Path,
    value: &T,
) -> Result<(), StorageError> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let tmp = parent.join(format!(
        ".{}.tmp-{}-{}",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("metadata"),
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    fs::write(&tmp, serde_json::to_string_pretty(value)?)?;
    fs::rename(tmp, path)?;
    Ok(())
}

pub(crate) fn resolve_bundle_path(bundle_path: &Path, relative_or_absolute: &str) -> PathBuf {
    let path = PathBuf::from(relative_or_absolute);
    if path.is_absolute() {
        path
    } else {
        bundle_path.join(path)
    }
}

pub(crate) fn absolutize(path: PathBuf) -> PathBuf {
    if path.is_absolute() {
        path
    } else {
        env::current_dir()
            .map(|cwd| cwd.join(&path))
            .unwrap_or(path)
    }
}

pub(crate) fn snapshot_disk_metadata_path(bundle: &Path, snapshot_name: &str) -> PathBuf {
    bundle
        .join("metadata")
        .join("snapshot-disks")
        .join(format!("{}.json", slug(snapshot_name)))
}

pub(crate) fn snapshot_disk_create_metadata_path(bundle: &Path, snapshot_name: &str) -> PathBuf {
    bundle
        .join("metadata")
        .join("snapshot-disks")
        .join(format!("{}-create.json", slug(snapshot_name)))
}

pub(crate) fn snapshot_suspend_image_metadata_path(bundle: &Path, snapshot_name: &str) -> PathBuf {
    bundle
        .join("metadata")
        .join("suspend-images")
        .join(format!("{}.json", slug(snapshot_name)))
}

pub(crate) fn fast_suspend_image_metadata_path(bundle: &Path, vm_name: &str) -> PathBuf {
    bundle
        .join("metadata")
        .join("suspend-images")
        .join(format!("{}.fast.json", slug(vm_name)))
}

pub(crate) fn application_consistent_snapshot_preflight_path(
    bundle: &Path,
    snapshot_name: &str,
) -> PathBuf {
    bundle
        .join("metadata")
        .join("application-consistent-snapshots")
        .join(format!("{}.json", slug(snapshot_name)))
}

pub(crate) fn guest_tools_token_path(bundle: &Path) -> PathBuf {
    bundle.join("metadata").join("guest-tools-token.json")
}

pub(crate) fn guest_tools_runtime_path(bundle: &Path) -> PathBuf {
    bundle.join("metadata").join("guest-tools-runtime.json")
}

pub(crate) fn runtime_resource_policy_path(bundle: &Path) -> PathBuf {
    bundle.join("metadata").join("runtime-resources.json")
}

pub(crate) fn deletion_metadata_path(bundle: &Path) -> PathBuf {
    bundle.join("metadata").join("deletion.json")
}

pub(crate) fn deletion_metadata_at(
    bundle: &Path,
) -> Result<Option<VmDeletionMetadata>, StorageError> {
    read_json_file(&deletion_metadata_path(bundle))
}

pub(crate) fn qmp_supervisor_path(bundle: &Path) -> PathBuf {
    bundle.join("metadata").join("qmp-supervisor.json")
}

pub(crate) fn live_evidence_metadata_path(bundle: &Path) -> PathBuf {
    bundle.join("metadata").join("live-evidence.json")
}

pub(crate) fn live_evidence_preserved_path(bundle: &Path) -> PathBuf {
    bundle.join("metadata").join("live-evidence").join("latest")
}

pub(crate) fn application_consistent_snapshot_required_capabilities() -> Vec<String> {
    vec!["fs-freeze".to_string(), "fs-thaw".to_string()]
}

pub(crate) const GUEST_TOOLS_CHANNEL_NAME: &str = "org.bridgevm.guest-tools.0";

pub(crate) fn parse_size_bytes(value: &str) -> Option<u64> {
    let trimmed = value.trim();
    let units = [
        ("GiB", 1024_u64.pow(3)),
        ("G", 1024_u64.pow(3)),
        ("MiB", 1024_u64.pow(2)),
        ("M", 1024_u64.pow(2)),
        ("KiB", 1024),
        ("K", 1024),
        ("B", 1),
    ];
    for (suffix, multiplier) in units {
        if let Some(number) = trimmed.strip_suffix(suffix) {
            // checked_mul: a huge value would otherwise panic (debug) or wrap
            // (release) into a wrong set_len size. Overflow -> None.
            return number
                .trim()
                .parse::<u64>()
                .ok()
                .and_then(|n| n.checked_mul(multiplier));
        }
    }
    trimmed.parse::<u64>().ok()
}

pub(crate) fn run_command(program: &str, args: &[String]) -> Result<Output, std::io::Error> {
    const COMMAND_TIMEOUT: Duration = Duration::from_secs(6 * 60 * 60);
    const COMMAND_OUTPUT_LIMIT: usize = 1024 * 1024;

    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| std::io::Error::other("failed to capture command stdout"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| std::io::Error::other("failed to capture command stderr"))?;
    let stdout_thread = thread::spawn(move || drain_command_stream(stdout, COMMAND_OUTPUT_LIMIT));
    let stderr_thread = thread::spawn(move || drain_command_stream(stderr, COMMAND_OUTPUT_LIMIT));

    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = stdout_thread.join();
                let _ = stderr_thread.join();
                return Err(error);
            }
        }
        if started.elapsed() >= COMMAND_TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            let _ = stdout_thread.join();
            let _ = stderr_thread.join();
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                format!(
                    "command exceeded {}-second timeout",
                    COMMAND_TIMEOUT.as_secs()
                ),
            ));
        }
        sleep(Duration::from_millis(100));
    };

    let (stdout, stdout_exceeded) = join_command_stream(stdout_thread, "stdout")?;
    let (stderr, stderr_exceeded) = join_command_stream(stderr_thread, "stderr")?;
    if stdout_exceeded || stderr_exceeded {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("command output exceeded {COMMAND_OUTPUT_LIMIT}-byte per-stream limit"),
        ));
    }
    Ok(Output {
        status,
        stdout,
        stderr,
    })
}

pub(crate) fn drain_command_stream<R: Read>(
    mut stream: R,
    limit: usize,
) -> Result<(Vec<u8>, bool), std::io::Error> {
    let mut retained = Vec::with_capacity(limit.min(8192));
    let mut exceeded = false;
    let mut buffer = [0_u8; 8192];
    loop {
        let read = stream.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        let remaining = limit.saturating_sub(retained.len());
        let keep = remaining.min(read);
        retained.extend_from_slice(&buffer[..keep]);
        exceeded |= keep < read;
    }
    Ok((retained, exceeded))
}

pub(crate) fn join_command_stream(
    handle: thread::JoinHandle<Result<(Vec<u8>, bool), std::io::Error>>,
    name: &str,
) -> Result<(Vec<u8>, bool), std::io::Error> {
    handle
        .join()
        .map_err(|_| std::io::Error::other(format!("command {name} drain thread panicked")))?
}

pub(crate) struct MetadataLock {
    pub(crate) path: PathBuf,
}

impl MetadataLock {
    pub(crate) fn acquire(bundle: &Path, name: &str) -> Result<Self, StorageError> {
        let path = bundle.join("metadata").join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        for _ in 0..100 {
            match fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(_) => return Ok(Self { path }),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    sleep(Duration::from_millis(10));
                }
                Err(error) => return Err(StorageError::Io(error)),
            }
        }
        Err(StorageError::Io(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            format!("timed out waiting for metadata lock {}", path.display()),
        )))
    }
}

impl Drop for MetadataLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}
