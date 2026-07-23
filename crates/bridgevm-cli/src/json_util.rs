//! Split out of main.rs by responsibility.

use crate::*;

pub(crate) fn manifest_u64(
    object: &serde_json::Map<String, serde_json::Value>,
    field: &str,
) -> Result<u64> {
    object
        .get(field)
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| {
            anyhow::anyhow!("title manifest field '{field}' must be an unsigned integer")
        })
}

pub(crate) fn evaluate_title_gate(
    manifest: TitleGateManifest,
    guest_logs: &Path,
    pre_run_state: &std::collections::BTreeMap<String, String>,
    resource_flushes: u64,
    driver_state_pass: bool,
) -> Result<TitleGateResult> {
    let log_path = guest_logs.join(&manifest.log);
    let mut blockers = Vec::new();
    let contents = match fs::read_to_string(&log_path) {
        Ok(contents) => Some(contents),
        Err(error) => {
            blockers.push(format!("guest log is missing or unreadable: {error}"));
            None
        }
    };
    let log_sha256 = if log_path.is_file() {
        Some(sha256_path(&log_path)?)
    } else {
        None
    };
    let fresh_log = match (pre_run_state.get(&manifest.id), log_sha256.as_deref()) {
        (Some(previous), Some(current)) => previous == "missing" || previous != current,
        _ => false,
    };
    if !fresh_log {
        blockers.push("guest log was not proven fresh for this run".to_string());
    }
    if !driver_state_pass {
        blockers.push("clean single-generation driver state was not proven".to_string());
    }

    let elapsed_ms = contents
        .as_deref()
        .and_then(|contents| log_u64_field(contents, "elapsed_ms"));
    if let Some(contents) = &contents {
        if !contents.contains(&manifest.pass_marker) {
            blockers.push(format!("pass marker '{}' is missing", manifest.pass_marker));
        }
        if manifest.require_main_window && !contents.contains("main_window_observed=true") {
            blockers.push("main window was not observed".to_string());
        }
        let contents_lower = contents.to_ascii_lowercase();
        for module in &manifest.required_modules {
            if !contents_lower.contains(&module.to_ascii_lowercase()) {
                blockers.push(format!("required module '{module}' was not observed"));
            }
        }
        if let Some(expected) = &manifest.executable_sha256 {
            let observed =
                log_string_field(contents, "executable_sha256").map(str::to_ascii_lowercase);
            if observed.as_deref() != Some(expected.as_str()) {
                blockers.push(format!(
                    "executable SHA-256 mismatch: expected {expected}, observed {}",
                    observed.as_deref().unwrap_or("missing")
                ));
            }
        }
    }
    let minimum_runtime_ms = manifest.minimum_runtime_seconds.saturating_mul(1_000);
    if elapsed_ms.is_none_or(|elapsed| elapsed < minimum_runtime_ms) {
        blockers.push(format!(
            "runtime did not reach required {minimum_runtime_ms} ms"
        ));
    }
    if resource_flushes < manifest.minimum_resource_flushes {
        blockers.push(format!(
            "RESOURCE_FLUSH count {resource_flushes} is below required {}",
            manifest.minimum_resource_flushes
        ));
    }

    Ok(TitleGateResult {
        manifest,
        log_path,
        log_sha256,
        fresh_log,
        elapsed_ms,
        resource_flushes,
        blockers,
    })
}

pub(crate) fn valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

pub(crate) fn sha256_path(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path)
        .with_context(|| format!("failed to open {} for SHA-256", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .with_context(|| format!("failed to hash {}", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

pub(crate) fn log_u64_field(contents: &str, field: &str) -> Option<u64> {
    log_string_field(contents, field)?.parse().ok()
}

pub(crate) fn log_string_field<'a>(contents: &'a str, field: &str) -> Option<&'a str> {
    let prefix = format!("{field}=");
    contents
        .split_whitespace()
        .rev()
        .find_map(|token| token.strip_prefix(&prefix))
}

#[derive(Debug, Default)]
pub(crate) struct VirtioGpuTraceReport {
    pub(crate) lines: usize,
    pub(crate) events: usize,
    pub(crate) invalid_lines: Vec<usize>,
    pub(crate) device_init: bool,
    pub(crate) backend_3d: bool,
    pub(crate) backend_attached: bool,
    pub(crate) queue_notify: bool,
    pub(crate) device_features_word0: Option<u64>,
    pub(crate) device_features_word1: Option<u64>,
    pub(crate) driver_features_word0: Option<u64>,
    pub(crate) driver_features_word1: Option<u64>,
    pub(crate) capset_info_ok: bool,
    pub(crate) virgl_capset_info_ok: bool,
    pub(crate) venus_capset_info_ok: bool,
    pub(crate) capset_ok: bool,
    pub(crate) virgl_capset_ok: bool,
    pub(crate) venus_capset_ok: bool,
    pub(crate) resource_create_3d_ok: bool,
    pub(crate) resource_attach_backing_ok: bool,
    pub(crate) blob_create_ok: bool,
    pub(crate) ctx_create_ok: bool,
    pub(crate) virgl_ctx_create_ok: bool,
    pub(crate) venus_ctx_create_ok: bool,
    pub(crate) submit_3d_ok: bool,
    pub(crate) submit_3d_nonzero_ok: bool,
    pub(crate) fenced_command: bool,
    pub(crate) fence_create: bool,
    pub(crate) backend_fence_parked: bool,
    pub(crate) fence_complete: bool,
    pub(crate) fence_deliver: bool,
    pub(crate) resource_flush_commands: u64,
    pub(crate) scanout_readbacks: u64,
    pub(crate) scanout_readback_throttled: u64,
    pub(crate) scanout_readback_bytes: u64,
    pub(crate) scanout_readback_nanoseconds: u64,
    pub(crate) scanout_readback_max_nanoseconds: u64,
    pub(crate) scanout_readback_transfer_nanoseconds: u64,
    pub(crate) scanout_readback_composite_nanoseconds: u64,
    pub(crate) scanout_readbacks_deferred: u64,
    pub(crate) scanout_blits: u64,
    pub(crate) scanout_blit_nanoseconds: u64,
    pub(crate) iosurface_verify_matched: u64,
    pub(crate) iosurface_verify_mismatched: u64,
    pub(crate) error_responses: Vec<String>,
}

impl VirtioGpuTraceReport {
    pub(crate) fn observe(&mut self, value: &serde_json::Value, line_number: usize) {
        match json_str(value, "event") {
            Some("device_init") => {
                self.device_init = true;
                self.backend_3d |= json_bool(value, "backend_3d").unwrap_or(false);
            }
            Some("backend_attached") => {
                self.backend_attached = true;
            }
            Some("common_read") => {
                if json_str(value, "field") == Some("device_features") {
                    match json_u64(value, "device_features_sel") {
                        Some(0) => self.device_features_word0 = json_u64(value, "value"),
                        Some(1) => self.device_features_word1 = json_u64(value, "value"),
                        _ => {}
                    }
                }
            }
            Some("driver_features") => match json_u64(value, "select") {
                Some(0) => self.driver_features_word0 = json_u64(value, "accepted"),
                Some(1) => self.driver_features_word1 = json_u64(value, "accepted"),
                _ => {}
            },
            Some("queue_notify") => {
                self.queue_notify |= json_bool(value, "valid").unwrap_or(true);
            }
            Some("command") => self.observe_command(value, line_number),
            Some("fence_create") => {
                self.fence_create = true;
                self.backend_fence_parked |= json_bool(value, "backend_accepted").unwrap_or(false)
                    && json_str(value, "outcome") == Some("parked");
            }
            Some("fence_complete") => self.fence_complete = true,
            Some("fence_deliver") => self.fence_deliver = true,
            Some("scanout_readback") => {
                self.scanout_readbacks = self.scanout_readbacks.saturating_add(1);
                self.scanout_readback_bytes = self
                    .scanout_readback_bytes
                    .saturating_add(json_u64(value, "bytes").unwrap_or(0));
                let duration_ns = json_u64(value, "duration_ns").unwrap_or(0);
                self.scanout_readback_nanoseconds = self
                    .scanout_readback_nanoseconds
                    .saturating_add(duration_ns);
                self.scanout_readback_max_nanoseconds =
                    self.scanout_readback_max_nanoseconds.max(duration_ns);
                self.scanout_readback_transfer_nanoseconds = self
                    .scanout_readback_transfer_nanoseconds
                    .saturating_add(json_u64(value, "transfer_ns").unwrap_or(0));
                self.scanout_readback_composite_nanoseconds = self
                    .scanout_readback_composite_nanoseconds
                    .saturating_add(json_u64(value, "composite_ns").unwrap_or(0));
                if json_u64(value, "deferred").unwrap_or(0) == 1 {
                    self.scanout_readbacks_deferred =
                        self.scanout_readbacks_deferred.saturating_add(1);
                }
            }
            Some("scanout_readback_throttled") => {
                self.scanout_readback_throttled = self.scanout_readback_throttled.saturating_add(1);
            }
            Some("scanout_blit") => {
                self.scanout_blits = self.scanout_blits.saturating_add(1);
                self.scanout_blit_nanoseconds = self
                    .scanout_blit_nanoseconds
                    .saturating_add(json_u64(value, "duration_ns").unwrap_or(0));
            }
            Some("scanout_iosurface_verify") => {
                if json_bool(value, "matched").unwrap_or(false) {
                    self.iosurface_verify_matched = self.iosurface_verify_matched.saturating_add(1);
                } else {
                    self.iosurface_verify_mismatched =
                        self.iosurface_verify_mismatched.saturating_add(1);
                }
            }
            _ => {}
        }
    }

    pub(crate) fn observe_command(&mut self, value: &serde_json::Value, line_number: usize) {
        let name = json_str(value, "name").unwrap_or("UNKNOWN");
        let response = json_str(value, "response_name").unwrap_or("UNKNOWN");
        if name == "RESOURCE_FLUSH" {
            self.resource_flush_commands = self.resource_flush_commands.saturating_add(1);
        }
        if json_bool(value, "fenced").unwrap_or(false) {
            self.fenced_command = true;
        }
        match (name, response) {
            ("GET_CAPSET_INFO", "OK_CAPSET_INFO") => {
                self.capset_info_ok = true;
                if let Some(capset_id) = json_u64(value, "response_capset_id") {
                    self.virgl_capset_info_ok |= is_virgl_capset(capset_id);
                    self.venus_capset_info_ok |= capset_id == VIRTIO_GPU_TRACE_CAPSET_VENUS;
                }
            }
            ("GET_CAPSET", "OK_CAPSET") => {
                self.capset_ok = true;
                if let Some(capset_id) = json_u64(value, "capset_id") {
                    self.virgl_capset_ok |= is_virgl_capset(capset_id);
                    self.venus_capset_ok |= capset_id == VIRTIO_GPU_TRACE_CAPSET_VENUS;
                }
            }
            ("RESOURCE_CREATE_BLOB", "OK_NODATA") => self.blob_create_ok = true,
            ("RESOURCE_CREATE_3D", "OK_NODATA") => self.resource_create_3d_ok = true,
            ("RESOURCE_ATTACH_BACKING", "OK_NODATA") => self.resource_attach_backing_ok = true,
            ("CTX_CREATE", "OK_NODATA") => {
                self.ctx_create_ok = true;
                if let Some(context_init) = json_u64(value, "context_init") {
                    let capset_id = context_init & 0xff;
                    self.virgl_ctx_create_ok |= is_virgl_capset(capset_id);
                    self.venus_ctx_create_ok |= capset_id == VIRTIO_GPU_TRACE_CAPSET_VENUS;
                }
            }
            ("SUBMIT_3D", "OK_NODATA") => {
                self.submit_3d_ok = true;
                self.submit_3d_nonzero_ok |=
                    json_u64(value, "submit_size").is_some_and(|size| size > 0);
            }
            _ => {}
        }
        if response.starts_with("ERR_") {
            let seq = json_u64(value, "seq")
                .map(|seq| seq.to_string())
                .unwrap_or_else(|| "?".to_string());
            self.error_responses.push(format!(
                "line {line_number}, seq {seq}: {name} -> {response}"
            ));
        }
    }

    pub(crate) fn has_3d_backend(&self) -> bool {
        self.backend_3d || self.backend_attached
    }

    pub(crate) fn accepted_venus_features(&self) -> bool {
        let required = VIRTIO_GPU_TRACE_FEATURE_VIRGL
            | VIRTIO_GPU_TRACE_FEATURE_RESOURCE_BLOB
            | VIRTIO_GPU_TRACE_FEATURE_CONTEXT_INIT;
        self.driver_features_word0
            .is_some_and(|features| features & required == required)
    }

    pub(crate) fn accepted_version_1(&self) -> bool {
        self.driver_features_word1
            .is_some_and(|features| features & VIRTIO_TRACE_FEATURE_VERSION_1 != 0)
    }

    pub(crate) fn fence_lifecycle_observed(&self) -> bool {
        self.fenced_command && self.fence_create && (self.fence_complete || self.fence_deliver)
    }

    pub(crate) fn scanout_readback_average_us(&self) -> f64 {
        if self.scanout_readbacks == 0 {
            return 0.0;
        }
        self.scanout_readback_nanoseconds as f64 / self.scanout_readbacks as f64 / 1_000.0
    }

    pub(crate) fn scanout_readback_phase_average_us(&self, phase_nanoseconds: u64) -> f64 {
        if self.scanout_readbacks == 0 {
            return 0.0;
        }
        phase_nanoseconds as f64 / self.scanout_readbacks as f64 / 1_000.0
    }

    pub(crate) fn scanout_readback_effective_gbps(&self) -> f64 {
        if self.scanout_readback_nanoseconds == 0 {
            return 0.0;
        }
        self.scanout_readback_bytes as f64 / self.scanout_readback_nanoseconds as f64
    }

    pub(crate) fn scanout_throttle_percent(&self) -> f64 {
        let observed = self
            .scanout_readbacks
            .saturating_add(self.scanout_readback_throttled);
        if observed == 0 {
            return 0.0;
        }
        self.scanout_readback_throttled as f64 / observed as f64 * 100.0
    }

    pub(crate) fn p3_blockers(&self, protocol: VirtioGpuTraceProtocolChoice) -> Vec<String> {
        let mut blockers = Vec::new();
        if !self.invalid_lines.is_empty() {
            blockers.push(format!(
                "invalid JSONL trace lines present: {}",
                self.invalid_lines
                    .iter()
                    .map(|line| line.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        if self.events == 0 {
            blockers.push("trace contains no parsed events".to_string());
        }
        if !self.device_init {
            blockers.push("missing device_init event".to_string());
        }
        if !self.has_3d_backend() {
            blockers.push("3D backend not attached in trace".to_string());
        }
        if !self.accepted_version_1() {
            blockers.push("driver did not accept VIRTIO_F_VERSION_1".to_string());
        }
        if !self.queue_notify {
            blockers.push("missing valid virtio-gpu queue_notify".to_string());
        }
        if !self.capset_info_ok {
            blockers.push("missing successful GET_CAPSET_INFO".to_string());
        }
        if !self.capset_ok {
            blockers.push("missing successful GET_CAPSET".to_string());
        }
        if !self.ctx_create_ok {
            blockers.push("missing successful CTX_CREATE".to_string());
        }
        if !self.submit_3d_ok {
            blockers.push("missing successful SUBMIT_3D".to_string());
        }
        if !self.submit_3d_nonzero_ok {
            blockers.push("missing successful non-empty SUBMIT_3D".to_string());
        }
        if !self.backend_fence_parked {
            blockers.push("missing backend-parked renderer fence".to_string());
        }
        if !self.fence_lifecycle_observed() {
            blockers
                .push("missing fenced command plus fence create/completion/delivery".to_string());
        }
        blockers.extend(self.protocol_blockers(protocol));
        blockers
    }

    pub(crate) fn protocol_blockers(&self, protocol: VirtioGpuTraceProtocolChoice) -> Vec<String> {
        match protocol {
            VirtioGpuTraceProtocolChoice::Venus => self.venus_protocol_blockers(),
            VirtioGpuTraceProtocolChoice::Virgl => self.virgl_protocol_blockers(),
            VirtioGpuTraceProtocolChoice::Auto => {
                let venus = self.venus_protocol_blockers();
                let virgl = self.virgl_protocol_blockers();
                if venus.is_empty() || virgl.is_empty() {
                    Vec::new()
                } else {
                    vec![format!(
                        "trace did not satisfy VENUS or VIRGL protocol identity (VENUS: {}; VIRGL: {})",
                        venus.join(", "),
                        virgl.join(", ")
                    )]
                }
            }
        }
    }

    pub(crate) fn venus_protocol_blockers(&self) -> Vec<String> {
        let mut blockers = Vec::new();
        if !self.accepted_venus_features() {
            blockers
                .push("driver did not accept VIRGL, RESOURCE_BLOB, and CONTEXT_INIT".to_string());
        }
        if !self.blob_create_ok {
            blockers.push("missing successful RESOURCE_CREATE_BLOB".to_string());
        }
        if !self.venus_capset_info_ok {
            blockers.push("GET_CAPSET_INFO did not report VENUS capset id 4".to_string());
        }
        if !self.venus_capset_ok {
            blockers.push("missing successful GET_CAPSET for VENUS capset id 4".to_string());
        }
        if !self.venus_ctx_create_ok {
            blockers.push("missing CTX_CREATE with VENUS context_init low byte 4".to_string());
        }
        blockers
    }

    pub(crate) fn virgl_protocol_blockers(&self) -> Vec<String> {
        let mut blockers = Vec::new();
        if !self.resource_create_3d_ok {
            blockers.push("missing successful RESOURCE_CREATE_3D".to_string());
        }
        if !self.resource_attach_backing_ok {
            blockers.push("missing successful RESOURCE_ATTACH_BACKING".to_string());
        }
        if !self.virgl_capset_info_ok {
            blockers
                .push("GET_CAPSET_INFO did not report VIRGL/VIRGL2 capset id 1 or 2".to_string());
        }
        if !self.virgl_capset_ok {
            blockers.push(
                "missing successful GET_CAPSET for VIRGL/VIRGL2 capset id 1 or 2".to_string(),
            );
        }
        if !self.virgl_ctx_create_ok {
            blockers.push(
                "missing CTX_CREATE with VIRGL/VIRGL2 context_init low byte 1 or 2".to_string(),
            );
        }
        blockers
    }

    pub(crate) fn selected_protocol(&self, protocol: VirtioGpuTraceProtocolChoice) -> &'static str {
        let venus_ok = self.venus_protocol_blockers().is_empty();
        let virgl_ok = self.virgl_protocol_blockers().is_empty();
        match protocol {
            VirtioGpuTraceProtocolChoice::Venus if venus_ok => "venus",
            VirtioGpuTraceProtocolChoice::Venus => "venus-missing",
            VirtioGpuTraceProtocolChoice::Virgl if virgl_ok => "virgl",
            VirtioGpuTraceProtocolChoice::Virgl => "virgl-missing",
            VirtioGpuTraceProtocolChoice::Auto if venus_ok && virgl_ok => "venus+virgl",
            VirtioGpuTraceProtocolChoice::Auto if venus_ok => "venus",
            VirtioGpuTraceProtocolChoice::Auto if virgl_ok => "virgl",
            VirtioGpuTraceProtocolChoice::Auto => "unknown",
        }
    }
}

pub(crate) fn is_virgl_capset(capset_id: u64) -> bool {
    capset_id == VIRTIO_GPU_TRACE_CAPSET_VIRGL || capset_id == VIRTIO_GPU_TRACE_CAPSET_VIRGL2
}

pub(crate) fn analyze_virtio_gpu_trace(path: &Path) -> Result<VirtioGpuTraceReport> {
    let file = fs::File::open(path)
        .with_context(|| format!("failed to open virtio-gpu trace {}", path.display()))?;
    let mut report = VirtioGpuTraceReport::default();
    for (index, line) in BufReader::new(file).lines().enumerate() {
        let line_number = index + 1;
        let line = line.with_context(|| format!("failed to read trace line {line_number}"))?;
        if line.trim().is_empty() {
            continue;
        }
        report.lines += 1;
        match serde_json::from_str::<serde_json::Value>(&line) {
            Ok(value) => {
                report.events += 1;
                report.observe(&value, line_number);
            }
            Err(_) => report.invalid_lines.push(line_number),
        }
    }
    Ok(report)
}

pub(crate) fn print_virtio_gpu_trace_report(
    path: &Path,
    protocol: VirtioGpuTraceProtocolChoice,
    report: &VirtioGpuTraceReport,
    blockers: &[String],
) {
    println!("BridgeVM HVF virtio-gpu trace report");
    println!("Trace: {}", path.display());
    println!("Requested protocol: {}", protocol.label());
    println!("Selected protocol: {}", report.selected_protocol(protocol));
    println!("Non-empty lines: {}", report.lines);
    println!("Parsed events: {}", report.events);
    println!("Invalid lines: {}", report.invalid_lines.len());
    println!("Device initialized: {}", report.device_init);
    println!("3D backend attached: {}", report.has_3d_backend());
    println!(
        "Device feature word0: {}",
        hex_option(report.device_features_word0)
    );
    println!(
        "Device feature word1: {}",
        hex_option(report.device_features_word1)
    );
    println!(
        "Driver feature word0: {}",
        hex_option(report.driver_features_word0)
    );
    println!(
        "Driver feature word1: {}",
        hex_option(report.driver_features_word1)
    );
    println!(
        "VENUS feature set accepted: {}",
        report.accepted_venus_features()
    );
    println!(
        "VIRTIO_F_VERSION_1 accepted: {}",
        report.accepted_version_1()
    );
    println!("Queue notify observed: {}", report.queue_notify);
    println!("GET_CAPSET_INFO OK: {}", report.capset_info_ok);
    println!(
        "GET_CAPSET_INFO VIRGL/VIRGL2 id 1/2: {}",
        report.virgl_capset_info_ok
    );
    println!(
        "GET_CAPSET_INFO VENUS id 4: {}",
        report.venus_capset_info_ok
    );
    println!("GET_CAPSET OK: {}", report.capset_ok);
    println!("GET_CAPSET VIRGL/VIRGL2 id 1/2: {}", report.virgl_capset_ok);
    println!("GET_CAPSET VENUS id 4: {}", report.venus_capset_ok);
    println!("RESOURCE_CREATE_3D OK: {}", report.resource_create_3d_ok);
    println!(
        "RESOURCE_ATTACH_BACKING OK: {}",
        report.resource_attach_backing_ok
    );
    println!("RESOURCE_CREATE_BLOB OK: {}", report.blob_create_ok);
    println!("CTX_CREATE OK: {}", report.ctx_create_ok);
    println!(
        "CTX_CREATE VIRGL/VIRGL2 context_init: {}",
        report.virgl_ctx_create_ok
    );
    println!(
        "CTX_CREATE VENUS context_init: {}",
        report.venus_ctx_create_ok
    );
    println!("SUBMIT_3D OK: {}", report.submit_3d_ok);
    println!("SUBMIT_3D non-empty: {}", report.submit_3d_nonzero_ok);
    println!("Fenced command observed: {}", report.fenced_command);
    println!("Fence create observed: {}", report.fence_create);
    println!(
        "Backend-parked fence observed: {}",
        report.backend_fence_parked
    );
    println!("Fence complete observed: {}", report.fence_complete);
    println!("Fence deliver observed: {}", report.fence_deliver);
    println!("Scanout readbacks: {}", report.scanout_readbacks);
    println!(
        "Scanout throttled flushes: {}",
        report.scanout_readback_throttled
    );
    println!("Scanout readback bytes: {}", report.scanout_readback_bytes);
    println!(
        "Scanout readback duration ns: {}",
        report.scanout_readback_nanoseconds
    );
    println!(
        "Scanout readback average us: {:.3}",
        report.scanout_readback_average_us()
    );
    println!(
        "Scanout readback max us: {:.3}",
        report.scanout_readback_max_nanoseconds as f64 / 1_000.0
    );
    println!(
        "Scanout readback transfer avg us: {:.3}",
        report.scanout_readback_phase_average_us(report.scanout_readback_transfer_nanoseconds)
    );
    println!(
        "Scanout readback composite avg us: {:.3}",
        report.scanout_readback_phase_average_us(report.scanout_readback_composite_nanoseconds)
    );
    println!(
        "Scanout readbacks deferred-serviced: {}",
        report.scanout_readbacks_deferred
    );
    println!("Scanout IOSurface blits: {}", report.scanout_blits);
    println!(
        "Scanout IOSurface blit avg us: {:.3}",
        if report.scanout_blits == 0 {
            0.0
        } else {
            report.scanout_blit_nanoseconds as f64 / report.scanout_blits as f64 / 1_000.0
        }
    );
    println!(
        "Scanout IOSurface verify: {} matched / {} mismatched",
        report.iosurface_verify_matched, report.iosurface_verify_mismatched
    );
    println!(
        "Scanout readback effective GB/s: {:.3}",
        report.scanout_readback_effective_gbps()
    );
    println!(
        "Scanout throttle ratio: {:.2}%",
        report.scanout_throttle_percent()
    );
    if report.error_responses.is_empty() {
        println!("Error responses: none");
    } else {
        println!("Error responses: {}", report.error_responses.len());
        for response in report.error_responses.iter().take(5) {
            println!("- {response}");
        }
    }
    println!(
        "P3 Windows 3D trace gate: {}",
        if blockers.is_empty() { "PASS" } else { "FAIL" }
    );
    if blockers.is_empty() {
        println!("Blockers: none");
    } else {
        println!("Blockers:");
        for blocker in blockers {
            println!("- {blocker}");
        }
    }
}

pub(crate) fn json_str<'a>(value: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    value.get(key)?.as_str()
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
    fn title_gate_requires_fresh_log_runtime_module_window_driver_and_flushes() {
        let mut root = unique_trace_path("bridgevm-title-gate");
        root.set_extension("dir");
        let guest_logs = root.join("guest-logs");
        fs::create_dir_all(&guest_logs).unwrap();
        let manifest_path = root.join("ppsspp.json");
        fs::write(
            &manifest_path,
            r#"{
  "version": 1,
  "id": "ppsspp-vulkan-arm64",
  "api": "vulkan",
  "architecture": "arm64",
  "log": "ppsspp.log",
  "pass_marker": "BVGPU-REAL-TITLE-PASS",
  "minimum_runtime_seconds": 30,
  "required_modules": ["vulkan_virtio.dll"],
  "require_main_window": true,
  "minimum_resource_flushes": 300
}"#,
        )
        .unwrap();
        fs::write(
            guest_logs.join("ppsspp.log"),
            "status=PASS elapsed_ms=30001 venus_icd=C:\\BridgeVM\\vulkan_virtio.dll main_window_observed=true\nBVGPU-REAL-TITLE-PASS\n",
        )
        .unwrap();
        fs::write(
            guest_logs.join("viogpu3d-cleanup.log"),
            "BVGPU-DRIVER-STATE-PASS\n",
        )
        .unwrap();

        let manifest = read_title_gate_manifest(&manifest_path).unwrap();
        let mut pre_run = std::collections::BTreeMap::new();
        pre_run.insert(manifest.id.clone(), "missing".to_string());
        let result = evaluate_title_gate(manifest, &guest_logs, &pre_run, 300, true).unwrap();
        assert!(result.passed(), "{:?}", result.blockers);

        let manifest = read_title_gate_manifest(&manifest_path).unwrap();
        pre_run.insert(
            manifest.id.clone(),
            sha256_path(&guest_logs.join("ppsspp.log")).unwrap(),
        );
        let stale = evaluate_title_gate(manifest, &guest_logs, &pre_run, 299, true).unwrap();
        assert!(!stale.passed());
        assert!(stale
            .blockers
            .iter()
            .any(|blocker| blocker.contains("not proven fresh")));
        assert!(stale
            .blockers
            .iter()
            .any(|blocker| blocker.contains("below required 300")));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn virtio_gpu_trace_report_aggregates_scanout_readbacks() {
        let path = unique_trace_path("bridgevm-cli-virtio-gpu-scanout");
        fs::write(
            &path,
            r#"{"event":"scanout_readback","bytes":4096000,"duration_ns":800000}
{"event":"scanout_readback_throttled"}
{"event":"scanout_readback","bytes":4096000,"duration_ns":1200000}
"#,
        )
        .unwrap();

        let report = analyze_virtio_gpu_trace(&path).unwrap();
        let _ = fs::remove_file(path);

        assert_eq!(report.scanout_readbacks, 2);
        assert_eq!(report.scanout_readback_throttled, 1);
        assert_eq!(report.scanout_readback_bytes, 8_192_000);
        assert_eq!(report.scanout_readback_nanoseconds, 2_000_000);
        assert_eq!(report.scanout_readback_max_nanoseconds, 1_200_000);
        assert!((report.scanout_readback_average_us() - 1_000.0).abs() < f64::EPSILON);
        assert!((report.scanout_readback_effective_gbps() - 4.096).abs() < f64::EPSILON);
        assert!((report.scanout_throttle_percent() - 100.0 / 3.0).abs() < 1e-12);
    }
}
