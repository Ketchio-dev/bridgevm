//! Split out of lib.rs by responsibility.

use crate::*;

pub fn create_performance_baseline(
    store: &VmStore,
    name: &str,
    output: PathBuf,
) -> Result<PerformanceBaselineMetadata, String> {
    let (source, _) = store.get_vm(name).map_err(|error| error.to_string())?;
    let created_at_unix = now_unix();
    let baseline_name = format!("bridgevm-performance-{name}-{created_at_unix}");
    let destination = output.join(baseline_name);
    if destination.exists() {
        return Err(format!(
            "performance baseline output already exists: {}",
            destination.display()
        ));
    }
    fs::create_dir_all(&destination)
        .map_err(|error| format!("failed to create performance baseline: {error}"))?;

    let state = store.state(name).map_err(|error| error.to_string())?;
    let runner = store
        .runner_metadata(name)
        .map_err(|error| error.to_string())?;
    let guest_tools = inspect_guest_tools_status(store, name)?;
    let metrics = guest_tools
        .runtime
        .as_ref()
        .and_then(|runtime| runtime.metrics.clone());
    let measurements = performance_measurements(created_at_unix, &state, runner.as_ref(), &metrics);
    let artifact = destination.join("performance-baseline.json");
    let baseline = PerformanceBaselineMetadata {
        vm: name.to_string(),
        source,
        output: destination,
        artifact: artifact.clone(),
        created_at_unix,
        metadata_only: true,
        state,
        runner,
        guest_tools,
        metrics,
        measurements,
        notes: vec![
            "metadata-only baseline; no active benchmark workloads were executed".to_string(),
            "captures existing VM state, runner metadata, and guest-tools runtime metrics"
                .to_string(),
        ],
    };
    fs::write(
        &artifact,
        serde_json::to_string_pretty(&baseline).map_err(|error| error.to_string())?,
    )
    .map_err(|error| format!("failed to write performance baseline metadata: {error}"))?;

    Ok(baseline)
}

pub fn create_performance_sample(
    store: &VmStore,
    name: &str,
    output: PathBuf,
    artifact_bytes: Option<u64>,
    iterations: Option<u16>,
    sync: bool,
) -> Result<PerformanceSampleMetadata, String> {
    let generation_started = Instant::now();
    let (source, manifest) = store.get_vm(name).map_err(|error| error.to_string())?;
    let (bytes, iterations, total_bytes) =
        validate_performance_sample_request(artifact_bytes, iterations)?;
    let created_at_unix = now_unix();
    let sample_name = format!("bridgevm-performance-sample-{name}-{created_at_unix}");
    let destination = output.join(sample_name);
    if destination.exists() {
        return Err(format!(
            "performance sample output already exists: {}",
            destination.display()
        ));
    }
    fs::create_dir_all(&destination)
        .map_err(|error| format!("failed to create performance sample: {error}"))?;

    let state_read_started = Instant::now();
    let state = store.state(name).map_err(|error| error.to_string())?;
    let state_read_latency = state_read_started.elapsed();
    let runner_read_started = Instant::now();
    let runner = store
        .runner_metadata(name)
        .map_err(|error| error.to_string())?;
    let runner_read_latency = runner_read_started.elapsed();
    let guest_tools_started = Instant::now();
    let guest_tools = inspect_guest_tools_status(store, name)?;
    let guest_tools_latency = guest_tools_started.elapsed();
    let metrics = guest_tools
        .runtime
        .as_ref()
        .and_then(|runtime| runtime.metrics.clone());

    let probe_data = vec![0_u8; bytes as usize];
    let mut probes = Vec::new();
    let mut iteration_results = Vec::new();
    for iteration in 1..=iterations {
        let probe = if iterations == 1 {
            destination.join("write-probe.bin")
        } else {
            destination.join(format!("write-probe-{iteration:04}.bin"))
        };
        let latency = write_performance_probe(&probe, &probe_data, sync)?;
        probes.push(probe.clone());
        iteration_results.push(PerformanceSampleIterationRecord {
            iteration,
            probe,
            bytes,
            write_latency_microseconds: duration_micros_u64(latency),
            sync,
        });
    }
    let probe = probes
        .first()
        .cloned()
        .ok_or_else(|| "performance sample did not produce a probe".to_string())?;

    let mut measurements =
        performance_measurements(created_at_unix, &state, runner.as_ref(), &metrics);
    measurements.push(performance_measurement_with_metadata_flag(
        "bridgevm_state_read_latency_microseconds",
        duration_micros_u64(state_read_latency),
        "microseconds",
        "bridgevm.store.state",
        false,
    ));
    measurements.push(performance_measurement_with_metadata_flag(
        "bridgevm_runner_metadata_read_latency_microseconds",
        duration_micros_u64(runner_read_latency),
        "microseconds",
        "bridgevm.store.runner_metadata",
        false,
    ));
    measurements.push(performance_measurement_with_metadata_flag(
        "bridgevm_guest_tools_status_inspect_latency_microseconds",
        duration_micros_u64(guest_tools_latency),
        "microseconds",
        "bridgevm.api.inspect_guest_tools_status",
        false,
    ));
    let write_latencies: Vec<u64> = iteration_results
        .iter()
        .map(|result| result.write_latency_microseconds)
        .collect();
    measurements.push(performance_measurement_with_metadata_flag(
        "host_artifact_write_latency_microseconds",
        mean_u64(&write_latencies),
        "microseconds",
        "host.fs.write_probe",
        false,
    ));
    measurements.push(performance_measurement_with_metadata_flag(
        "host_artifact_write_bytes",
        bytes,
        "bytes",
        "host.fs.write_probe",
        false,
    ));
    measurements.push(performance_measurement_with_metadata_flag(
        "host_artifact_write_iterations",
        u64::from(iterations),
        "count",
        "host.fs.write_probe",
        false,
    ));
    measurements.push(performance_measurement_with_metadata_flag(
        "host_artifact_write_total_bytes",
        total_bytes,
        "bytes",
        "host.fs.write_probe",
        false,
    ));
    measurements.push(performance_measurement_with_metadata_flag(
        "host_artifact_write_latency_min_microseconds",
        *write_latencies.iter().min().unwrap_or(&0),
        "microseconds",
        "host.fs.write_probe",
        false,
    ));
    measurements.push(performance_measurement_with_metadata_flag(
        "host_artifact_write_latency_max_microseconds",
        *write_latencies.iter().max().unwrap_or(&0),
        "microseconds",
        "host.fs.write_probe",
        false,
    ));
    measurements.push(performance_measurement_with_metadata_flag(
        "host_artifact_write_latency_mean_microseconds",
        mean_u64(&write_latencies),
        "microseconds",
        "host.fs.write_probe",
        false,
    ));
    measurements.push(performance_measurement_with_metadata_flag(
        "host_artifact_write_latency_p50_microseconds",
        percentile_u64(write_latencies.clone(), 50),
        "microseconds",
        "host.fs.write_probe",
        false,
    ));
    let mut notes = vec![
        "host-side sample; no guest benchmark workloads were executed".to_string(),
        "write latency is measured for the probe file left in this artifact directory".to_string(),
    ];
    match inspect_sample_primary_disk(store, name, &source, &manifest) {
        Ok(Some(disk)) => {
            measurements.push(performance_measurement_with_metadata_flag(
                "disk_inspect_duration_microseconds",
                disk.inspect_duration_microseconds,
                "microseconds",
                "host.qemu-img.info",
                false,
            ));
            notes.push(
                "disk inspect duration measures host qemu-img info execution, not guest disk I/O"
                    .to_string(),
            );
        }
        Ok(None) => notes.push(
            "disk inspect duration skipped because no existing non-raw primary disk was available"
                .to_string(),
        ),
        Err(error) => notes.push(format!("disk inspect duration skipped: {error}")),
    }
    measurements.push(performance_measurement_with_metadata_flag(
        "sample_generation_duration_microseconds",
        duration_micros_u64(generation_started.elapsed()),
        "microseconds",
        "bridgevm.performance.sample",
        false,
    ));

    let artifact = destination.join("performance-sample.json");
    let sample = PerformanceSampleMetadata {
        vm: name.to_string(),
        source,
        output: destination,
        artifact: artifact.clone(),
        probe,
        probes,
        artifact_bytes: bytes,
        iterations,
        sync,
        iteration_results,
        created_at_unix,
        state,
        runner,
        guest_tools,
        metrics,
        measurements,
        notes,
    };
    fs::write(
        &artifact,
        serde_json::to_string_pretty(&sample).map_err(|error| error.to_string())?,
    )
    .map_err(|error| format!("failed to write performance sample metadata: {error}"))?;

    Ok(sample)
}

pub(crate) fn validate_performance_sample_request(
    artifact_bytes: Option<u64>,
    iterations: Option<u16>,
) -> Result<(u64, u16, u64), String> {
    let bytes = artifact_bytes.unwrap_or(DEFAULT_PERFORMANCE_SAMPLE_ARTIFACT_BYTES);
    if bytes > MAX_PERFORMANCE_SAMPLE_ARTIFACT_BYTES {
        return Err(format!(
            "performance sample artifact is too large: {bytes} bytes (max {MAX_PERFORMANCE_SAMPLE_ARTIFACT_BYTES})"
        ));
    }
    let iterations = iterations.unwrap_or(DEFAULT_PERFORMANCE_SAMPLE_ITERATIONS);
    if iterations == 0 {
        return Err("performance sample iterations must be greater than zero".to_string());
    }
    if iterations > MAX_PERFORMANCE_SAMPLE_ITERATIONS {
        return Err(format!(
            "performance sample iterations is too large: {iterations} (max {MAX_PERFORMANCE_SAMPLE_ITERATIONS})"
        ));
    }
    let total_bytes = bytes
        .checked_mul(u64::from(iterations))
        .ok_or_else(|| "performance sample total bytes overflowed".to_string())?;
    if total_bytes > MAX_PERFORMANCE_SAMPLE_TOTAL_BYTES {
        return Err(format!(
            "performance sample total artifact bytes is too large: {total_bytes} bytes (max {MAX_PERFORMANCE_SAMPLE_TOTAL_BYTES})"
        ));
    }
    Ok((bytes, iterations, total_bytes))
}

pub(crate) fn inspect_sample_primary_disk(
    store: &VmStore,
    name: &str,
    bundle: &Path,
    manifest: &VmManifest,
) -> Result<Option<DiskInspectMetadata>, String> {
    if manifest.storage.primary.format == "raw" {
        return Ok(None);
    }
    let path = resolve_bundle_path(bundle, &manifest.storage.primary.path);
    if !path.exists() {
        return Ok(None);
    }
    store
        .inspect_primary_disk(name)
        .map(Some)
        .map_err(|error| error.to_string())
}

pub(crate) fn resolve_bundle_path(bundle: &Path, relative_or_absolute: &str) -> PathBuf {
    let path = PathBuf::from(relative_or_absolute);
    if path.is_absolute() {
        path
    } else {
        bundle.join(path)
    }
}

pub(crate) fn duration_micros_u64(duration: std::time::Duration) -> u64 {
    duration.as_micros().min(u128::from(u64::MAX)) as u64
}

pub(crate) fn write_performance_probe(
    probe: &Path,
    probe_data: &[u8],
    sync: bool,
) -> Result<std::time::Duration, String> {
    let write_started = Instant::now();
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(probe)
        .map_err(|error| format!("failed to open performance sample probe: {error}"))?;
    file.write_all(probe_data)
        .map_err(|error| format!("failed to write performance sample probe: {error}"))?;
    if sync {
        file.sync_data()
            .map_err(|error| format!("failed to sync performance sample probe: {error}"))?;
    }
    Ok(write_started.elapsed())
}

pub(crate) fn mean_u64(values: &[u64]) -> u64 {
    if values.is_empty() {
        return 0;
    }
    values.iter().sum::<u64>() / values.len() as u64
}

pub(crate) fn percentile_u64(mut values: Vec<u64>, percentile: u8) -> u64 {
    if values.is_empty() {
        return 0;
    }
    values.sort_unstable();
    let index = ((values.len() - 1) * usize::from(percentile)) / 100;
    values[index]
}

pub(crate) fn performance_measurements(
    created_at_unix: u64,
    state: &VmRuntimeMetadata,
    runner: Option<&RunnerMetadata>,
    metrics: &Option<GuestToolsMetricsMetadata>,
) -> Vec<PerformanceMeasurementRecord> {
    let mut measurements = Vec::new();
    if let Some(value) = created_at_unix.checked_sub(state.updated_at_unix) {
        measurements.push(performance_measurement(
            "state_metadata_age_seconds",
            value,
            "seconds",
            "state.updated_at_unix",
        ));
    }
    if let Some(runner) = runner {
        if let Some(value) = created_at_unix.checked_sub(runner.started_at_unix) {
            measurements.push(performance_measurement(
                "runner_observed_uptime_seconds",
                value,
                "seconds",
                "runner.started_at_unix",
            ));
        }
    }
    if let Some(metrics) = metrics {
        measurements.push(performance_measurement(
            "guest_cpu_percent",
            u64::from(metrics.cpu_percent),
            "percent",
            "guest_tools.metrics.cpu_percent",
        ));
        measurements.push(performance_measurement(
            "guest_memory_used_mib",
            metrics.memory_used_mib,
            "MiB",
            "guest_tools.metrics.memory_used_mib",
        ));
        if let Some(value) = created_at_unix.checked_sub(metrics.updated_at_unix) {
            measurements.push(performance_measurement(
                "guest_metrics_age_seconds",
                value,
                "seconds",
                "guest_tools.metrics.updated_at_unix",
            ));
        }
    }
    measurements
}

pub(crate) fn performance_measurement(
    name: &str,
    value: u64,
    unit: &str,
    source: &str,
) -> PerformanceMeasurementRecord {
    performance_measurement_with_metadata_flag(name, value, unit, source, true)
}

pub(crate) fn performance_measurement_with_metadata_flag(
    name: &str,
    value: u64,
    unit: &str,
    source: &str,
    metadata_only: bool,
) -> PerformanceMeasurementRecord {
    PerformanceMeasurementRecord {
        name: name.to_string(),
        value,
        unit: unit.to_string(),
        source: source.to_string(),
        metadata_only,
    }
}

pub(crate) fn copy_diagnostic_dir(
    source: &Path,
    destination: &Path,
    bundle_root: &Path,
    token: Option<&str>,
    files: &mut Vec<PathBuf>,
) -> Result<(), String> {
    if !source.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(source).map_err(|error| {
        format!(
            "failed to read diagnostic directory {}: {error}",
            source.display()
        )
    })? {
        let entry = entry.map_err(|error| error.to_string())?;
        let source_path = entry.path();
        if should_skip_diagnostic_path(&source_path) {
            continue;
        }
        let destination_path = destination.join(entry.file_name());
        let file_type = entry.file_type().map_err(|error| error.to_string())?;
        if file_type.is_dir() {
            copy_diagnostic_dir(&source_path, &destination_path, bundle_root, token, files)?;
        } else if file_type.is_file() {
            copy_diagnostic_file(&source_path, &destination_path, bundle_root, token, files)?;
        }
    }
    Ok(())
}

pub(crate) fn copy_diagnostic_file(
    source: &Path,
    destination: &Path,
    bundle_root: &Path,
    token: Option<&str>,
    files: &mut Vec<PathBuf>,
) -> Result<(), String> {
    if !source.exists() {
        return Ok(());
    }
    if should_skip_diagnostic_path(source) {
        return Ok(());
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create diagnostic directory {}: {error}",
                parent.display()
            )
        })?;
    }
    let mut bytes = Vec::new();
    fs::File::open(source)
        .and_then(|file| {
            file.take(MAX_DIAGNOSTIC_FILE_BYTES + 1)
                .read_to_end(&mut bytes)
        })
        .map_err(|error| {
            format!(
                "failed to read diagnostic file {}: {error}",
                source.display()
            )
        })?;
    let content = if bytes.len() as u64 > MAX_DIAGNOSTIC_FILE_BYTES {
        format!(
            "[bridgevm diagnostic file omitted: source exceeded the {MAX_DIAGNOSTIC_FILE_BYTES}-byte safety limit]\n"
        )
    } else {
        redact_diagnostic_text(&String::from_utf8_lossy(&bytes), token)
    };
    fs::write(destination, content.as_bytes()).map_err(|error| {
        format!(
            "failed to write diagnostic file {}: {error}",
            destination.display()
        )
    })?;
    let relative = destination
        .strip_prefix(bundle_root)
        .map_err(|error| error.to_string())?
        .to_path_buf();
    files.push(relative);
    Ok(())
}

pub(crate) fn should_skip_diagnostic_path(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.ends_with(".sock") || name.ends_with(".lock"))
}

pub(crate) fn redact_diagnostic_text(content: &str, token: Option<&str>) -> String {
    let mut redacted = redact_sensitive_json_keys(content).unwrap_or_else(|| content.to_string());
    if let Some(token) = token.filter(|token| !token.is_empty()) {
        redacted = redacted.replace(token, "<redacted>");
    }
    redacted
}

pub(crate) fn redact_sensitive_json_keys(content: &str) -> Option<String> {
    let mut value: serde_json::Value = serde_json::from_str(content).ok()?;
    redact_sensitive_json_value(&mut value);
    serde_json::to_string_pretty(&value).ok()
}

pub(crate) fn redact_sensitive_json_value(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, value) in map.iter_mut() {
                if is_sensitive_diagnostic_key(key) {
                    *value = serde_json::Value::String("<redacted>".to_string());
                } else {
                    redact_sensitive_json_value(value);
                }
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                redact_sensitive_json_value(item);
            }
        }
        serde_json::Value::String(text) => {
            if let Some(redacted) = redact_url_query(text) {
                *text = redacted;
            }
        }
        _ => {}
    }
}

pub(crate) fn redact_url_query(value: &str) -> Option<String> {
    if !(value.starts_with("http://") || value.starts_with("https://")) {
        return None;
    }
    let (before_fragment, fragment) = value
        .split_once('#')
        .map_or((value, ""), |(before_fragment, fragment)| {
            (before_fragment, fragment)
        });
    let (base, _) = before_fragment.split_once('?')?;
    let mut redacted = format!("{base}?<redacted>");
    if !fragment.is_empty() {
        redacted.push('#');
        redacted.push_str(fragment);
    }
    Some(redacted)
}

pub(crate) fn is_sensitive_diagnostic_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    ["token", "password", "secret", "authorization", "credential"]
        .iter()
        .any(|sensitive| key.contains(sensitive))
}
