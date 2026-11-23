//! `bridgevm hvf …` dispatch and the shared virtio-gpu / title-gate report types.

use crate::*;

pub(crate) fn hvf(command: HvfCommand) -> Result<()> {
    match command {
        HvfCommand::VirtioGpu3dHostPreflight(args) => {
            let probe = probe_virtio_gpu_3d_host_preflight_for(args.protocol.into());
            print!("{}", probe.render_text());
            Ok(())
        }
        HvfCommand::VirtioGpuTraceReport(args) => {
            let report = analyze_virtio_gpu_trace(&args.trace)?;
            let blockers = report.p3_blockers(args.protocol);
            print_virtio_gpu_trace_report(&args.trace, args.protocol, &report, &blockers);
            if args.require_p3_gate && !blockers.is_empty() {
                bail!("virtio-gpu P3 trace gate failed: {}", blockers.join("; "));
            }
            Ok(())
        }
        HvfCommand::TitleGateReport(args) => run_title_gate_report(args),
    }
}

pub(crate) const VIRTIO_GPU_TRACE_FEATURE_VIRGL: u64 = 1 << 0;
pub(crate) const VIRTIO_GPU_TRACE_FEATURE_RESOURCE_BLOB: u64 = 1 << 3;
pub(crate) const VIRTIO_GPU_TRACE_FEATURE_CONTEXT_INIT: u64 = 1 << 4;
pub(crate) const VIRTIO_TRACE_FEATURE_VERSION_1: u64 = 1 << 0;
pub(crate) const VIRTIO_GPU_TRACE_CAPSET_VIRGL: u64 = 1;
pub(crate) const VIRTIO_GPU_TRACE_CAPSET_VIRGL2: u64 = 2;
pub(crate) const VIRTIO_GPU_TRACE_CAPSET_VENUS: u64 = 4;

#[derive(Debug)]
pub(crate) struct TitleGateManifest {
    pub(crate) path: PathBuf,
    pub(crate) id: String,
    pub(crate) api: String,
    pub(crate) architecture: String,
    pub(crate) executable: Option<String>,
    pub(crate) working_directory: Option<String>,
    pub(crate) arguments: Vec<String>,
    pub(crate) executable_sha256: Option<String>,
    pub(crate) log: PathBuf,
    pub(crate) pass_marker: String,
    pub(crate) minimum_runtime_seconds: u64,
    pub(crate) required_modules: Vec<String>,
    pub(crate) require_main_window: bool,
    pub(crate) minimum_resource_flushes: u64,
}

#[derive(Debug)]
pub(crate) struct TitleGateResult {
    pub(crate) manifest: TitleGateManifest,
    pub(crate) log_path: PathBuf,
    pub(crate) log_sha256: Option<String>,
    pub(crate) fresh_log: bool,
    pub(crate) elapsed_ms: Option<u64>,
    pub(crate) resource_flushes: u64,
    pub(crate) blockers: Vec<String>,
}

impl TitleGateResult {
    pub(crate) fn passed(&self) -> bool {
        self.blockers.is_empty()
    }

    pub(crate) fn as_json(&self) -> serde_json::Value {
        serde_json::json!({
            "id": self.manifest.id,
            "api": self.manifest.api,
            "architecture": self.manifest.architecture,
            "executable": self.manifest.executable,
            "working_directory": self.manifest.working_directory,
            "arguments": self.manifest.arguments,
            "manifest": self.manifest.path,
            "log": self.log_path,
            "log_sha256": self.log_sha256,
            "fresh_log": self.fresh_log,
            "elapsed_ms": self.elapsed_ms,
            "resource_flushes": self.resource_flushes,
            "minimum_resource_flushes": self.manifest.minimum_resource_flushes,
            "passed": self.passed(),
            "blockers": self.blockers,
        })
    }
}
