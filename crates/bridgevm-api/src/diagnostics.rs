//! Split out of lib.rs by responsibility.

use crate::*;

pub fn create_diagnostic_bundle(
    store: &VmStore,
    name: &str,
    output: PathBuf,
) -> Result<DiagnosticBundleMetadata, String> {
    let (source, _) = store.get_vm(name).map_err(|error| error.to_string())?;
    let created_at_unix = now_unix();
    let bundle_name = format!("bridgevm-diagnostics-{name}-{created_at_unix}");
    let destination = output.join(bundle_name);
    if destination.exists() {
        return Err(format!(
            "diagnostic bundle output already exists: {}",
            destination.display()
        ));
    }
    fs::create_dir_all(&destination)
        .map_err(|error| format!("failed to create diagnostic bundle: {error}"))?;

    let token = store
        .guest_tools_token(name)
        .map(|metadata| metadata.token)
        .ok();
    let mut files = Vec::new();
    copy_diagnostic_file(
        &source.join("manifest.yaml"),
        &destination.join("manifest.yaml"),
        &destination,
        token.as_deref(),
        &mut files,
    )?;
    copy_diagnostic_dir(
        &source.join("metadata"),
        &destination.join("metadata"),
        &destination,
        token.as_deref(),
        &mut files,
    )?;
    copy_diagnostic_dir(
        &source.join("logs"),
        &destination.join("logs"),
        &destination,
        token.as_deref(),
        &mut files,
    )?;

    let mut metadata = DiagnosticBundleMetadata {
        vm: name.to_string(),
        source,
        output: destination.clone(),
        files,
        created_at_unix,
    };
    let metadata_path = destination.join("diagnostic-bundle.json");
    fs::write(
        &metadata_path,
        serde_json::to_string_pretty(&metadata).map_err(|error| error.to_string())?,
    )
    .map_err(|error| format!("failed to write diagnostic bundle metadata: {error}"))?;
    metadata.files.push(PathBuf::from("diagnostic-bundle.json"));
    fs::write(
        &metadata_path,
        serde_json::to_string_pretty(&metadata).map_err(|error| error.to_string())?,
    )
    .map_err(|error| format!("failed to write diagnostic bundle metadata: {error}"))?;

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
