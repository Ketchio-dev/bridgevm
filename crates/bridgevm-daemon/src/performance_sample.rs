//! Performance-sample creation and folding guest benchmark results into it.

use crate::*;
use anyhow::Context;
use anyhow::Result;
use bridgevm_agent_protocol::AgentEnvelope;
use bridgevm_agent_protocol::AgentMessage;
use bridgevm_agent_protocol::DEFAULT_BENCHMARK_DURATION_MILLIS;
use bridgevm_agent_protocol::MAX_BENCHMARK_DURATION_MILLIS;
use bridgevm_api::create_performance_sample;
use bridgevm_api::inspect_guest_tools_status;
use bridgevm_api::BridgeVmResponse;
use bridgevm_api::PerformanceMeasurementRecord;
use bridgevm_api::PerformanceSampleMetadata;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

pub(crate) fn record_guest_benchmark_result(
    sample: &mut PerformanceSampleMetadata,
    completed: &CompletedGuestToolsCommand,
) {
    sample
        .notes
        .retain(|note| note != "host-side sample; no guest benchmark workloads were executed");
    if !completed.ok {
        let reason = completed
            .error_code
            .as_deref()
            .or(completed.message.as_deref())
            .unwrap_or("command-result-not-ok");
        sample.notes.push(format!(
            "guest benchmark command did not produce measurements: {reason}"
        ));
        return;
    }

    sample.notes.push(format!(
        "guest benchmark executed over daemon-owned guest-tools session (request id {})",
        completed.request_id
    ));
    let Some(result) = completed.result.as_ref() else {
        sample
            .notes
            .push("guest benchmark completed without a result payload".to_string());
        return;
    };

    push_guest_benchmark_measurement(
        &mut sample.measurements,
        result,
        "/budget_duration_millis",
        "guest_benchmark_budget_millis",
        "milliseconds",
        "guest_tools.benchmark.budget_duration_millis",
    );
    push_guest_benchmark_measurement(
        &mut sample.measurements,
        result,
        "/cpu/iterations",
        "guest_benchmark_cpu_iterations",
        "count",
        "guest_tools.benchmark.cpu.iterations",
    );
    push_guest_benchmark_measurement(
        &mut sample.measurements,
        result,
        "/cpu/elapsed_millis",
        "guest_benchmark_cpu_elapsed_millis",
        "milliseconds",
        "guest_tools.benchmark.cpu.elapsed_millis",
    );
    push_guest_benchmark_measurement(
        &mut sample.measurements,
        result,
        "/cpu/ops_per_sec",
        "guest_benchmark_cpu_ops_per_sec",
        "ops_per_second",
        "guest_tools.benchmark.cpu.ops_per_sec",
    );
    push_guest_benchmark_measurement(
        &mut sample.measurements,
        result,
        "/disk/bytes_written",
        "guest_benchmark_disk_bytes_written",
        "bytes",
        "guest_tools.benchmark.disk.bytes_written",
    );
    push_guest_benchmark_measurement(
        &mut sample.measurements,
        result,
        "/disk/elapsed_millis",
        "guest_benchmark_disk_elapsed_millis",
        "milliseconds",
        "guest_tools.benchmark.disk.elapsed_millis",
    );
    push_guest_benchmark_measurement(
        &mut sample.measurements,
        result,
        "/disk/mib_per_sec",
        "guest_benchmark_disk_mib_per_sec",
        "MiB_per_second",
        "guest_tools.benchmark.disk.mib_per_sec",
    );
    if let Some(error) = result.get("disk_error").and_then(|value| value.as_str()) {
        sample.notes.push(format!(
            "guest benchmark disk micro-benchmark skipped: {error}"
        ));
    }
}

pub(crate) fn push_guest_benchmark_measurement(
    measurements: &mut Vec<PerformanceMeasurementRecord>,
    result: &serde_json::Value,
    pointer: &str,
    name: &str,
    unit: &str,
    source: &str,
) {
    if let Some(value) = result.pointer(pointer).and_then(|value| value.as_u64()) {
        measurements.push(PerformanceMeasurementRecord {
            name: name.to_string(),
            value,
            unit: unit.to_string(),
            source: source.to_string(),
            metadata_only: false,
        });
    }
}

impl DaemonState {
    pub(crate) fn create_performance_sample_with_optional_guest_benchmark(
        &mut self,
        name: &str,
        output: PathBuf,
        artifact_bytes: Option<u64>,
        iterations: Option<u16>,
        sync: bool,
    ) -> Result<BridgeVmResponse> {
        let mut sample =
            create_performance_sample(&self.store, name, output, artifact_bytes, iterations, sync)
                .map_err(anyhow::Error::msg)?;

        match self.run_guest_benchmark_for_sample(name, sample.created_at_unix) {
            Ok(Some(completed)) => record_guest_benchmark_result(&mut sample, &completed),
            Ok(None) => sample.notes.push(
                "guest benchmark skipped because no benchmark-capable guest-tools session was connected"
                    .to_string(),
            ),
            Err(error) => sample
                .notes
                .push(format!("guest benchmark skipped: {error}")),
        }

        if let Ok(status) = inspect_guest_tools_status(&self.store, name) {
            sample.metrics = status
                .runtime
                .as_ref()
                .and_then(|runtime| runtime.metrics.clone());
            sample.guest_tools = status;
        }
        fs::write(
            &sample.artifact,
            serde_json::to_string_pretty(&sample).context("failed to serialize sample")?,
        )
        .with_context(|| {
            format!(
                "failed to update performance sample metadata at {}",
                sample.artifact.display()
            )
        })?;

        Ok(BridgeVmResponse::PerformanceSample { sample })
    }

    pub(crate) fn run_guest_benchmark_for_sample(
        &mut self,
        name: &str,
        created_at_unix: u64,
    ) -> Result<Option<CompletedGuestToolsCommand>> {
        let supports_benchmark = self
            .children
            .get(name)
            .and_then(|backend| backend.guest_tools.as_ref())
            .is_some_and(|session| session.supports("benchmark"));
        if !supports_benchmark {
            return Ok(None);
        }

        let request_id = format!("performance-sample:{created_at_unix}:guest-benchmark");
        let envelope = AgentEnvelope::with_request_id(
            AgentMessage::RunBenchmark {
                duration_millis: Some(DEFAULT_BENCHMARK_DURATION_MILLIS),
            },
            request_id.clone(),
        );
        self.send_guest_tools_command_record(name, envelope)?;
        self.wait_for_guest_tools_command_result(
            name,
            &request_id,
            Duration::from_millis(MAX_BENCHMARK_DURATION_MILLIS.saturating_add(5_000)),
        )
        .map(Some)
    }
}
