//! Diagnostics and bounded log views.

use crate::*;

pub fn create_diagnostic_bundle(
    store: &VmStore,
    name: &str,
    output: PathBuf,
) -> Result<DiagnosticBundleMetadata, String> {
    let (source, manifest) = store.get_vm(name).map_err(|error| error.to_string())?;
    let created_at_unix = now_unix();
    let destination = output.join(format!("bridgevm-diagnostics-{created_at_unix}"));
    if destination.exists() {
        return Err(format!(
            "diagnostic bundle output already exists: {}",
            destination.display()
        ));
    }
    fs::create_dir_all(&output).map_err(|error| error.to_string())?;
    let staging = output.join(format!(
        ".bridgevm-diagnostics-staging-{}-{created_at_unix}",
        std::process::id()
    ));
    fs::create_dir(&staging)
        .map_err(|error| format!("failed to create diagnostic bundle staging: {error}"))?;
    let mut metadata = diagnostic_allowlist::build_bundle(
        name,
        source,
        manifest,
        staging.clone(),
        created_at_unix,
    )
    .map_err(|error| diagnostic_allowlist::cleanup_error(&staging, error))?;
    fs::rename(&staging, &destination).map_err(|error| {
        diagnostic_allowlist::cleanup_error(
            &staging,
            format!("failed to publish diagnostic bundle: {error}"),
        )
    })?;
    metadata.output = destination;
    Ok(metadata)
}

pub fn view_vm_log(
    store: &VmStore,
    name: &str,
    kind: VmLogKind,
    max_bytes: Option<u64>,
) -> Result<VmLogViewRecord, String> {
    let (bundle, _) = store.get_vm(name).map_err(|error| error.to_string())?;
    let path = bundle.join("logs").join(kind.file_name());
    if !path.exists() {
        return Ok(VmLogViewRecord {
            vm: name.to_string(),
            kind,
            path,
            exists: false,
            bytes: 0,
            returned_bytes: 0,
            truncated: false,
            content: String::new(),
        });
    }
    let bytes_to_read = max_bytes
        .unwrap_or(DEFAULT_LOG_VIEW_BYTES)
        .clamp(1, MAX_LOG_VIEW_BYTES);
    let mut file =
        fs::File::open(&path).map_err(|error| format!("failed to open log file: {error}"))?;
    let bytes = file
        .metadata()
        .map_err(|error| format!("failed to inspect log file: {error}"))?
        .len();
    let start = bytes.saturating_sub(bytes_to_read);
    file.seek(SeekFrom::Start(start))
        .map_err(|error| format!("failed to seek log file: {error}"))?;
    let capacity = usize::try_from(bytes_to_read)
        .map_err(|_| "log read limit exceeds host address space".to_string())?;
    let mut buffer = Vec::with_capacity(capacity);
    file.take(bytes_to_read)
        .read_to_end(&mut buffer)
        .map_err(|error| format!("failed to read log file: {error}"))?;
    let returned_bytes = buffer.len() as u64;
    Ok(VmLogViewRecord {
        vm: name.to_string(),
        kind,
        path,
        exists: true,
        bytes,
        returned_bytes,
        truncated: start > 0,
        content: String::from_utf8_lossy(&buffer).to_string(),
    })
}
