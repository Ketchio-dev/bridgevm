//! `bridgevm hvf …` dispatch.

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
