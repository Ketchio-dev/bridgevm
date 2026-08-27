//! Guest benchmark sampling waits for the asynchronous tools handshake.

use super::helpers::*;
use super::wait::wait_up_to_ten_seconds;
use crate::*;
use bridgevm_agent_protocol::AgentAuth;
use bridgevm_agent_protocol::AgentCapability;
use bridgevm_agent_protocol::AgentEnvelope;
use bridgevm_agent_protocol::AgentMessage;
use bridgevm_agent_protocol::DEFAULT_BENCHMARK_DURATION_MILLIS;
use bridgevm_agent_protocol::PROTOCOL_VERSION;
use bridgevm_agentd::encode_envelope_line;
use bridgevm_api::BridgeVmRequest;
use bridgevm_api::BridgeVmResponse;
use bridgevm_storage::VmRuntimeState;
use std::fs;
use std::io::BufRead;
use std::io::BufReader;
use std::io::Write;
use std::os::unix::net::UnixListener;
use std::process::Command;
use std::thread;
use std::time::Duration;

#[test]
fn daemon_performance_sample_runs_guest_benchmark_when_session_is_connected() {
    let store = temp_store();
    store.create_vm(&compatibility_manifest("legacy")).unwrap();
    store
        .transition_state("legacy", VmRuntimeState::Running)
        .unwrap();

    let token = store.guest_tools_token("legacy").unwrap().token;
    let guest_tools = store.guest_tools_runner_metadata("legacy").unwrap();
    let listener = UnixListener::bind(&guest_tools.socket_path).unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let hello = AgentEnvelope::new(AgentMessage::GuestHello {
            version: PROTOCOL_VERSION,
            guest_os: "linux".to_string(),
            agent_version: Some("1.0.0".to_string()),
            capabilities: vec![
                AgentCapability {
                    name: "heartbeat".to_string(),
                    version: 1,
                },
                AgentCapability {
                    name: "benchmark".to_string(),
                    version: 1,
                },
            ],
            auth: Some(AgentAuth::ToolsToken { token }),
        });
        stream
            .write_all(encode_envelope_line(&hello).unwrap().as_bytes())
            .unwrap();

        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut command_line = String::new();
        reader.read_line(&mut command_line).unwrap();
        let command: AgentEnvelope = serde_json::from_str(command_line.trim_end()).unwrap();
        assert!(command
            .request_id
            .as_deref()
            .unwrap()
            .starts_with("performance-sample:"));
        assert_eq!(
            command.message,
            AgentMessage::RunBenchmark {
                duration_millis: Some(DEFAULT_BENCHMARK_DURATION_MILLIS)
            }
        );

        let result = AgentEnvelope::new(AgentMessage::CommandResult {
            request_id: command.request_id.unwrap(),
            ok: true,
            error_code: None,
            message: Some("benchmark complete".to_string()),
            result: Some(serde_json::json!({
                "budget_duration_millis": DEFAULT_BENCHMARK_DURATION_MILLIS,
                "cpu": {
                    "iterations": 4096,
                    "elapsed_millis": 1000,
                    "ops_per_sec": 4096,
                    "checksum": 12345
                },
                "disk": {
                    "bytes_written": 4096,
                    "elapsed_millis": 2,
                    "mib_per_sec": 25
                }
            })),
            metadata: None,
        });
        stream
            .write_all(encode_envelope_line(&result).unwrap().as_bytes())
            .unwrap();
        thread::sleep(Duration::from_millis(250));
    });

    let child = Command::new("sh").arg("-c").arg("sleep 15").spawn().unwrap();
    let mut state = DaemonState::new(store.clone());
    state
        .children
        .insert("legacy".to_string(), SupervisedBackend::new(child));

    assert!(
        wait_up_to_ten_seconds(|| {
            if state
                .children
                .get("legacy")
                .is_some_and(|backend| backend.guest_tools.is_some())
            {
                return true;
            }
            state.reconcile_children().unwrap();
            state
                .children
                .get("legacy")
                .is_some_and(|backend| backend.guest_tools.is_some())
        }),
        "guest-tools handshake did not complete before performance sampling"
    );

    let output = store.root().join("daemon-performance-with-benchmark");
    let response = state
        .handle_request(BridgeVmRequest::CreatePerformanceSample {
            name: "legacy".to_string(),
            output,
            artifact_bytes: Some(1024),
            iterations: Some(1),
            sync: false,
        })
        .into_result()
        .unwrap();
    let BridgeVmResponse::PerformanceSample { sample } = response else {
        panic!("expected performance sample response");
    };

    assert!(sample
        .notes
        .iter()
        .any(|note| note.contains("guest benchmark executed")));
    assert!(!sample
        .notes
        .iter()
        .any(|note| note.contains("no guest benchmark workloads")));
    assert!(sample.measurements.iter().any(|measurement| {
        measurement.name == "guest_benchmark_cpu_iterations"
            && measurement.value == 4096
            && !measurement.metadata_only
    }));
    assert!(sample.measurements.iter().any(|measurement| {
        measurement.name == "guest_benchmark_disk_bytes_written"
            && measurement.value == 4096
            && !measurement.metadata_only
    }));
    let artifact = fs::read_to_string(&sample.artifact).unwrap();
    assert!(artifact.contains("guest_benchmark_cpu_ops_per_sec"));
    let runtime = sample
        .guest_tools
        .runtime
        .expect("refreshed guest tools runtime");
    assert_eq!(
        runtime.last_command_result.unwrap().capability.as_deref(),
        Some("benchmark")
    );

    state.cleanup_owned_backend("legacy", false).unwrap();
    server.join().unwrap();
}
