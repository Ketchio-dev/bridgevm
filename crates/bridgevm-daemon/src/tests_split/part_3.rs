//! Split test module.

use super::helpers::*;
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
use bridgevm_qemu::qmp_socket_path;
use bridgevm_storage::RunnerMetadata;
use bridgevm_storage::VmRuntimeState;
use std::fs;
use std::io::BufRead;
use std::io::BufReader;
use std::io::Write;
use std::os::unix::net::UnixListener;
use std::path::PathBuf;
use std::process::Command;
use std::thread;
use std::time::Duration;

#[test]
fn daemon_fast_spawn_opt_in_supervises_lightvm_runner_child() {
    let store = temp_store();
    store.create_vm(&ready_fast_manifest("fast-linux")).unwrap();
    let bundle = store.bundle_path("fast-linux");
    fs::create_dir_all(bundle.join("boot")).unwrap();
    fs::create_dir_all(bundle.join("disks")).unwrap();
    fs::write(bundle.join("boot").join("vmlinuz"), b"kernel").unwrap();
    fs::write(bundle.join("disks").join("root.raw"), b"disk").unwrap();

    let lightvm_runner = store.root().join("fake-lightvm-runner");
    let argv_log = store.root().join("fake-lightvm-argv.txt");
    let apple_vz_runner = store.root().join("fake-AppleVzRunner");
    write_executable(
        &lightvm_runner,
        &format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\nsleep 2\n",
            argv_log.display()
        ),
    );
    write_executable(&apple_vz_runner, "#!/bin/sh\ncat >/dev/null\n");

    let mut state = DaemonState::new(store.clone());
    let response = state.spawn_fast_backend(
        "fast-linux",
        bundle.clone(),
        ready_fast_manifest("fast-linux"),
        FastModeSpawnConfig {
            lightvm_runner: lightvm_runner.clone(),
            apple_vz_runner: apple_vz_runner.clone(),
            stop_after_seconds: Some(5),
            force_stop_grace_seconds: Some(1),
            verify_apple_vz_runner_entitlement: false,
        },
    );
    let BridgeVmResponse::RunnerStatus {
        metadata: Some(metadata),
        ..
    } = response.unwrap()
    else {
        panic!("expected supervised Fast Mode runner metadata");
    };

    assert_eq!(metadata.engine, "lightvm");
    assert!(!metadata.dry_run);
    assert!(metadata.pid.is_some());
    assert_eq!(
        metadata.command.first().unwrap(),
        &lightvm_runner.display().to_string()
    );
    assert!(metadata.command.contains(&"--launch".to_string()));
    assert!(metadata.command.contains(&"--require-ready".to_string()));
    assert!(metadata
        .command
        .contains(&"--apple-vz-allow-real-start".to_string()));
    assert!(metadata
        .command
        .contains(&apple_vz_runner.display().to_string()));
    let expected_launch_spec = bundle.join("metadata").join("apple-vz-launch.json");
    assert_eq!(
        metadata.launch_spec_path.as_ref(),
        Some(&expected_launch_spec)
    );
    assert_eq!(
        store.state("fast-linux").unwrap().state,
        VmRuntimeState::Running
    );
    assert!(state.children.contains_key("fast-linux"));

    state.cleanup_owned_backend("fast-linux", false).unwrap();
    assert_eq!(store.runner_metadata("fast-linux").unwrap(), None);
}

#[test]
fn daemon_fast_spawn_immediate_exit_reconcile_clears_runtime_state() {
    let store = temp_store();
    store.create_vm(&ready_fast_manifest("fast-linux")).unwrap();
    let bundle = store.bundle_path("fast-linux");
    fs::create_dir_all(bundle.join("boot")).unwrap();
    fs::create_dir_all(bundle.join("disks")).unwrap();
    fs::write(bundle.join("boot").join("vmlinuz"), b"kernel").unwrap();
    fs::write(bundle.join("disks").join("root.raw"), b"disk").unwrap();

    let lightvm_runner = store.root().join("fake-lightvm-runner");
    let apple_vz_runner = store.root().join("fake-AppleVzRunner");
    write_executable(&lightvm_runner, "#!/bin/sh\necho fast-fail >&2\nexit 7\n");
    write_executable(&apple_vz_runner, "#!/bin/sh\ncat >/dev/null\n");

    let mut state = DaemonState::new(store.clone());
    let response = state
        .spawn_fast_backend(
            "fast-linux",
            bundle.clone(),
            ready_fast_manifest("fast-linux"),
            FastModeSpawnConfig {
                lightvm_runner,
                apple_vz_runner,
                stop_after_seconds: None,
                force_stop_grace_seconds: None,
                verify_apple_vz_runner_entitlement: false,
            },
        )
        .unwrap();
    let BridgeVmResponse::RunnerStatus {
        metadata: Some(metadata),
        ..
    } = response
    else {
        panic!("expected supervised Fast Mode runner metadata");
    };

    assert_eq!(
        store.state("fast-linux").unwrap().state,
        VmRuntimeState::Running
    );
    assert!(state.children.contains_key("fast-linux"));

    for _ in 0..120 {
        state.reconcile_children().unwrap();
        if !state.children.contains_key("fast-linux") {
            break;
        }
        thread::sleep(Duration::from_millis(25));
    }

    assert!(!state.children.contains_key("fast-linux"));
    assert_eq!(
        store.state("fast-linux").unwrap().state,
        VmRuntimeState::Stopped
    );
    assert_eq!(store.runner_metadata("fast-linux").unwrap(), None);
    assert!(
        fs::read_to_string(metadata.log_path)
            .unwrap()
            .contains("fast-fail"),
        "runner stderr should be captured in the Fast Mode log"
    );

    fs::remove_dir_all(store.root()).unwrap();
}

#[test]
fn daemon_connection_creates_redacted_diagnostic_bundle() {
    let store = temp_store();
    store.create_vm(&compatibility_manifest("legacy")).unwrap();
    let token = store.guest_tools_token("legacy").unwrap().token;
    let bundle_path = store.bundle_path("legacy");
    fs::write(
        bundle_path.join("logs").join("qemu.log"),
        format!("guest tools token {token}\n"),
    )
    .unwrap();
    fs::write(
        bundle_path.join("metadata").join("download.json"),
        r#"{"url":"https://example.invalid/image.iso?signature=secret"}"#,
    )
    .unwrap();

    let output = store.root().join("daemon-diagnostics");
    let request = BridgeVmRequest::CreateDiagnosticBundle {
        name: "legacy".to_string(),
        output,
    };
    let response = daemon_request(store.clone(), request);
    let BridgeVmResponse::DiagnosticBundle { bundle } = response else {
        panic!("expected diagnostic bundle response");
    };

    assert!(bundle.output.exists());
    assert!(bundle.files.contains(&PathBuf::from("manifest.yaml")));
    assert!(bundle.files.contains(&PathBuf::from("logs/qemu.log")));
    assert!(bundle
        .files
        .contains(&PathBuf::from("metadata/download.json")));
    assert!(bundle
        .files
        .contains(&PathBuf::from("diagnostic-bundle.json")));

    let log = fs::read_to_string(bundle.output.join("logs").join("qemu.log")).unwrap();
    assert!(!log.contains(&token));
    assert!(log.contains("<redacted>"));
    let download =
        fs::read_to_string(bundle.output.join("metadata").join("download.json")).unwrap();
    assert!(!download.contains("signature=secret"));
    assert!(download.contains("https://example.invalid/image.iso?<redacted>"));
}

#[test]
fn daemon_connection_creates_performance_sample() {
    let store = temp_store();
    store.create_vm(&compatibility_manifest("legacy")).unwrap();

    let output = store.root().join("daemon-performance");
    let request = BridgeVmRequest::CreatePerformanceSample {
        name: "legacy".to_string(),
        output,
        artifact_bytes: Some(1024),
        iterations: Some(2),
        sync: false,
    };
    let response = daemon_request(store, request);
    let BridgeVmResponse::PerformanceSample { sample } = response else {
        panic!("expected performance sample response");
    };

    assert!(sample.output.exists());
    assert!(sample.artifact.exists());
    assert_eq!(sample.artifact_bytes, 1024);
    assert_eq!(sample.iterations, 2);
    assert_eq!(sample.probes.len(), 2);
    assert!(sample
        .measurements
        .iter()
        .any(
            |measurement| measurement.name == "host_artifact_write_total_bytes"
                && measurement.value == 2048
                && !measurement.metadata_only
        ));
}

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
                "requested_duration_millis": DEFAULT_BENCHMARK_DURATION_MILLIS,
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

    let child = Command::new("sh").arg("-c").arg("sleep 5").spawn().unwrap();
    let mut state = DaemonState::new(store.clone());
    state
        .children
        .insert("legacy".to_string(), SupervisedBackend::new(child));

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

#[test]
fn reconcile_children_clears_exited_backend_state() {
    let store = temp_store();
    store.create_vm(&compatibility_manifest("legacy")).unwrap();
    store
        .transition_state("legacy", VmRuntimeState::Running)
        .unwrap();
    store
        .write_runner_metadata(
            "legacy",
            &RunnerMetadata {
                engine: "fullvm".to_string(),
                pid: Some(0),
                command: vec!["sh".to_string(), "-c".to_string(), "exit 0".to_string()],
                log_path: store.bundle_path("legacy").join("logs").join("qemu.log"),
                started_at_unix: now_unix(),
                dry_run: false,
                launch_spec_path: None,
                guest_tools: None,
                disk: None,
                active_disk: None,
                launch_readiness: None,
                runtime_control: None,
            },
        )
        .unwrap();

    let child = Command::new("sh").arg("-c").arg("exit 0").spawn().unwrap();
    let mut state = DaemonState::new(store.clone());
    state
        .children
        .insert("legacy".to_string(), SupervisedBackend::new(child));

    for _ in 0..40 {
        state.reconcile_children().unwrap();
        if state.children.is_empty() {
            break;
        }
        thread::sleep(Duration::from_millis(25));
    }

    assert!(state.children.is_empty());
    assert_eq!(
        store.state("legacy").unwrap().state,
        VmRuntimeState::Stopped
    );
    assert_eq!(store.runner_metadata("legacy").unwrap(), None);
}

#[test]
fn cleanup_owned_backend_clears_already_exited_child_state() {
    let store = temp_store();
    store.create_vm(&compatibility_manifest("legacy")).unwrap();
    store
        .transition_state("legacy", VmRuntimeState::Running)
        .unwrap();
    store
        .write_runner_metadata(
            "legacy",
            &RunnerMetadata {
                engine: "fullvm".to_string(),
                pid: Some(0),
                command: vec!["sh".to_string(), "-c".to_string(), "exit 0".to_string()],
                log_path: store.bundle_path("legacy").join("logs").join("qemu.log"),
                started_at_unix: now_unix(),
                dry_run: false,
                launch_spec_path: None,
                guest_tools: None,
                disk: None,
                active_disk: None,
                launch_readiness: None,
                runtime_control: None,
            },
        )
        .unwrap();

    let child = Command::new("sh").arg("-c").arg("exit 0").spawn().unwrap();
    let mut state = DaemonState::new(store.clone());
    state
        .children
        .insert("legacy".to_string(), SupervisedBackend::new(child));

    for _ in 0..40 {
        if state
            .children
            .get_mut("legacy")
            .unwrap()
            .child
            .try_wait()
            .unwrap()
            .is_some()
        {
            break;
        }
        thread::sleep(Duration::from_millis(25));
    }

    state.cleanup_owned_backend("legacy", false).unwrap();

    assert!(state.children.is_empty());
    assert_eq!(
        store.state("legacy").unwrap().state,
        VmRuntimeState::Stopped
    );
    assert_eq!(store.runner_metadata("legacy").unwrap(), None);
}

#[test]
fn daemon_routes_compat_resume_to_supervised_handler() {
    // Without a suspend marker the supervised compat resume reports the
    // marker error. This proves the daemon routes ResumeBackend through the
    // supervised path (the generic api fallback would have produced the
    // same marker error only via resume_compatibility_backend, never the
    // legacy "not wired yet" message).
    let store = temp_store();
    store.create_vm(&compatibility_manifest("legacy")).unwrap();
    store
        .transition_state("legacy", VmRuntimeState::Running)
        .unwrap();

    let mut state = DaemonState::new(store.clone());
    let error = state.resume_backend_supervised("legacy").unwrap_err();
    let message = format!("{error:#}");
    assert!(
        message.contains("no saved Compatibility Mode state to resume from"),
        "{message}"
    );
    assert!(!state.children.contains_key("legacy"));
    fs::remove_dir_all(store.root()).unwrap();
}

#[test]
fn daemon_routes_fast_resume_to_supervised_handler() {
    // Fast resume with no saved state and no real-start env reports the Fast
    // state-missing error, proving the request reached the supervised Fast
    // resume branch (not the compat branch and not "not wired yet").
    let store = temp_store();
    store.create_vm(&fast_manifest("fast-linux")).unwrap();
    store
        .transition_state("fast-linux", VmRuntimeState::Running)
        .unwrap();

    let mut state = DaemonState::new(store.clone());
    let error = state.resume_backend_supervised("fast-linux").unwrap_err();
    let message = format!("{error:#}");
    assert!(
        message.contains("no saved Fast Mode state to resume from"),
        "{message}"
    );
    assert!(!state.children.contains_key("fast-linux"));
    fs::remove_dir_all(store.root()).unwrap();
}

#[test]
fn reconcile_children_cleans_up_terminal_qmp_event() {
    let store = temp_store();
    store.create_vm(&compatibility_manifest("legacy")).unwrap();
    store
        .transition_state("legacy", VmRuntimeState::Running)
        .unwrap();
    store
        .write_runner_metadata(
            "legacy",
            &RunnerMetadata {
                engine: "fullvm".to_string(),
                pid: Some(0),
                command: vec!["sh".to_string(), "-c".to_string(), "sleep 5".to_string()],
                log_path: store.bundle_path("legacy").join("logs").join("qemu.log"),
                started_at_unix: now_unix(),
                dry_run: false,
                launch_spec_path: None,
                guest_tools: None,
                disk: None,
                active_disk: None,
                launch_readiness: None,
                runtime_control: None,
            },
        )
        .unwrap();

    let (bundle, _) = store.get_vm("legacy").unwrap();
    let socket_path = qmp_socket_path(&bundle);
    let listener = UnixListener::bind(&socket_path).unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .write_all(br#"{"QMP":{"version":{"qemu":{"major":8,"minor":2,"micro":0}}}}"#)
            .unwrap();
        stream.write_all(b"\n").unwrap();

        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        assert!(line.contains("qmp_capabilities"));
        stream.write_all(br#"{"return":{}}"#).unwrap();
        stream.write_all(b"\n").unwrap();

        stream
            .write_all(br#"{"event":"BLOCK_JOB_COMPLETED","data":{"device":"drive0"}}"#)
            .unwrap();
        stream.write_all(b"\n").unwrap();
        stream
            .write_all(br#"{"event":"SHUTDOWN","data":{"guest":true}}"#)
            .unwrap();
        stream.write_all(b"\n").unwrap();
    });

    let child = Command::new("sh").arg("-c").arg("sleep 5").spawn().unwrap();
    let mut state = DaemonState::new(store.clone());
    state
        .children
        .insert("legacy".to_string(), SupervisedBackend::new(child));

    state.reconcile_children().unwrap();

    assert!(state.children.is_empty());
    assert_eq!(
        store.state("legacy").unwrap().state,
        VmRuntimeState::Stopped
    );
    assert_eq!(store.runner_metadata("legacy").unwrap(), None);
    let qmp = store
        .qmp_supervisor_metadata("legacy")
        .unwrap()
        .expect("qmp supervisor metadata");
    assert_eq!(qmp.envelopes_read, 2);
    assert_eq!(
        qmp.events
            .iter()
            .map(|event| event.name.as_str())
            .collect::<Vec<_>>(),
        ["BLOCK_JOB_COMPLETED", "SHUTDOWN"]
    );
    assert_eq!(qmp.terminal_event.as_ref().unwrap().name, "SHUTDOWN");
    assert_eq!(
        qmp.terminal_event.as_ref().unwrap().data.as_ref().unwrap(),
        &serde_json::json!({"guest": true})
    );
    assert!(!qmp.limit_reached);
    server.join().unwrap();
}

#[test]
fn reconcile_children_records_nonterminal_qmp_events_without_cleanup() {
    let store = temp_store();
    store.create_vm(&compatibility_manifest("legacy")).unwrap();
    store
        .transition_state("legacy", VmRuntimeState::Running)
        .unwrap();
    store
        .write_runner_metadata(
            "legacy",
            &RunnerMetadata {
                engine: "fullvm".to_string(),
                pid: Some(0),
                command: vec!["sh".to_string(), "-c".to_string(), "sleep 5".to_string()],
                log_path: store.bundle_path("legacy").join("logs").join("qemu.log"),
                started_at_unix: now_unix(),
                dry_run: false,
                launch_spec_path: None,
                guest_tools: None,
                disk: None,
                active_disk: None,
                launch_readiness: None,
                runtime_control: None,
            },
        )
        .unwrap();

    let (bundle, _) = store.get_vm("legacy").unwrap();
    let socket_path = qmp_socket_path(&bundle);
    let listener = UnixListener::bind(&socket_path).unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .write_all(br#"{"QMP":{"version":{"qemu":{"major":8,"minor":2,"micro":0}}}}"#)
            .unwrap();
        stream.write_all(b"\n").unwrap();

        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        assert!(line.contains("qmp_capabilities"));
        stream.write_all(br#"{"return":{}}"#).unwrap();
        stream.write_all(b"\n").unwrap();
        stream
            .write_all(br#"{"event":"RESUME","data":{"status":"running"}}"#)
            .unwrap();
        stream.write_all(b"\n").unwrap();
        thread::sleep(Duration::from_millis(100));
    });

    let child = Command::new("sh").arg("-c").arg("sleep 5").spawn().unwrap();
    let mut state = DaemonState::new(store.clone());
    state
        .children
        .insert("legacy".to_string(), SupervisedBackend::new(child));

    state.reconcile_children().unwrap();

    assert!(state.children.contains_key("legacy"));
    assert_eq!(
        store.state("legacy").unwrap().state,
        VmRuntimeState::Running
    );
    let qmp = store
        .qmp_supervisor_metadata("legacy")
        .unwrap()
        .expect("qmp supervisor metadata");
    assert_eq!(qmp.envelopes_read, 1);
    assert_eq!(qmp.events.len(), 1);
    assert_eq!(qmp.events[0].name, "RESUME");
    assert_eq!(
        qmp.events[0].data.as_ref().unwrap(),
        &serde_json::json!({"status": "running"})
    );
    assert!(qmp.terminal_event.is_none());
    assert!(!qmp.limit_reached);

    state.cleanup_owned_backend("legacy", false).unwrap();
    server.join().unwrap();
}
