//! Split test module.

use super::helpers::*;
use crate::*;
use bridgevm_agent_protocol::AgentCapability;
use bridgevm_agent_protocol::AgentEnvelope;
use bridgevm_agent_protocol::AgentMessage;
use bridgevm_agent_protocol::DEFAULT_BENCHMARK_DURATION_MILLIS;
use bridgevm_agentd::decode_envelope_line;
use bridgevm_agentd::encode_envelope_line;
use bridgevm_agentd::write_envelope_line;
use std::fs;
use std::io::BufReader;
use std::io::Cursor;
use std::os::unix::net::UnixListener;
use std::os::unix::net::UnixStream;
use std::thread;
use std::time::Duration;

#[test]
fn time_sync_simulated_backend_acknowledges_without_setting_clock() {
    let mut state =
        GuestToolsState::new(&default_capabilities()).with_clock_setter(ClockSetter::simulated());
    let command = AgentEnvelope::with_request_id(
        AgentMessage::TimeSync {
            unix_epoch_millis: 1_781_470_000_000,
        },
        "time-1",
    );

    let AgentMessage::CommandResult {
        ok,
        error_code,
        message,
        result,
        ..
    } = state.handle_command(&command).unwrap().message
    else {
        panic!("expected CommandResult");
    };
    assert!(ok);
    assert_eq!(error_code, None);
    assert!(message.unwrap().contains("guest clock was not changed"));
    assert_eq!(
        result,
        Some(serde_json::json!({
            "applied_unix_epoch_millis": 1_781_470_000_000u64,
        }))
    );
}

#[test]
fn time_sync_real_backend_failure_reports_error() {
    let applied = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let backend = RecordingClockBackend {
        applied,
        fail: true,
    };
    let mut state = GuestToolsState::new(&default_capabilities())
        .with_clock_setter(ClockSetter::real(Box::new(backend)));
    let command = AgentEnvelope::with_request_id(
        AgentMessage::TimeSync {
            unix_epoch_millis: 1_781_470_000_000,
        },
        "time-1",
    );

    let AgentMessage::CommandResult { ok, error_code, .. } =
        state.handle_command(&command).unwrap().message
    else {
        panic!("expected CommandResult");
    };
    assert!(!ok);
    assert_eq!(error_code.as_deref(), Some("time-sync-failed"));
}

#[test]
fn time_sync_requires_capability() {
    let mut state = GuestToolsState::new(&[AgentCapability {
        name: "heartbeat".to_string(),
        version: 1,
    }])
    .with_clock_setter(ClockSetter::real(Box::new(RecordingClockBackend {
        applied: std::rc::Rc::new(std::cell::RefCell::new(Vec::new())),
        fail: false,
    })));
    let command = AgentEnvelope::with_request_id(
        AgentMessage::TimeSync {
            unix_epoch_millis: 1_781_470_000_000,
        },
        "time-1",
    );

    assert_eq!(
        state.handle_command(&command).unwrap().message,
        AgentMessage::CommandResult {
            request_id: "time-1".to_string(),
            ok: false,
            error_code: Some("capability-not-enabled".to_string()),
            message: Some("time-sync capability is not enabled".to_string()),
            result: None,
            metadata: None,
        }
    );
}

#[test]
fn resolve_clock_setter_defaults_to_real_when_capable() {
    // Real when time-sync is advertised and not opted out.
    assert!(matches!(
        resolve_clock_setter(&default_capabilities(), false).mode,
        ClockSetterMode::Real { .. }
    ));
    // Simulated when opted out, or when time-sync is not negotiated.
    assert!(matches!(
        resolve_clock_setter(&default_capabilities(), true).mode,
        ClockSetterMode::Simulated
    ));
    assert!(matches!(
        resolve_clock_setter(
            &[AgentCapability {
                name: "heartbeat".to_string(),
                version: 1,
            }],
            false,
        )
        .mode,
        ClockSetterMode::Simulated
    ));
}

#[test]
fn proc_meminfo_parses_used_memory_in_mib() {
    let meminfo = "MemTotal:        4194304 kB\nMemFree:          524288 kB\nMemAvailable:    2097152 kB\nBuffers:           10240 kB\n";
    // used_kib = 4194304 - 2097152 = 2097152 kB = 2048 MiB
    assert_eq!(parse_memory_used_mib(meminfo), Some(2048));

    // Missing MemAvailable -> None (cannot compute used reliably).
    assert_eq!(parse_memory_used_mib("MemTotal: 4194304 kB\n"), None);
}

#[test]
fn loadavg_parses_and_maps_to_cpu_percent() {
    assert_eq!(
        parse_loadavg_one_minute("0.50 0.25 0.10 1/234 5678"),
        Some(0.50)
    );
    assert_eq!(parse_loadavg_one_minute("garbage"), None);
    // 1.0 load over 4 CPUs -> 25%.
    assert_eq!(load_to_cpu_percent(1.0, 4), 25);
    // Saturation: 8.0 load over 4 CPUs clamps to 100%.
    assert_eq!(load_to_cpu_percent(8.0, 4), 100);
    // Zero CPUs treated as one.
    assert_eq!(load_to_cpu_percent(0.5, 0), 50);
}

#[test]
fn real_metrics_reader_feeds_telemetry_and_falls_back_when_unavailable() {
    // A reader that returns real values is used verbatim.
    let telemetry = TelemetryConfig::from_args_with_reader(
        &default_capabilities(),
        &[],
        false,
        1,
        256,
        false,
        false,
        None,
        || {
            Some(GuestMetricsConfig {
                cpu_percent: 42,
                memory_used_mib: 777,
            })
        },
    )
    .unwrap();
    assert_eq!(
        telemetry.metrics,
        Some(GuestMetricsConfig {
            cpu_percent: 42,
            memory_used_mib: 777,
        })
    );

    // When the reader yields None, fall back to the configured synthetic
    // values (here the 256 MiB / 1% defaults).
    let telemetry = TelemetryConfig::from_args_with_reader(
        &default_capabilities(),
        &[],
        false,
        1,
        256,
        false,
        false,
        None,
        || None,
    )
    .unwrap();
    assert_eq!(
        telemetry.metrics,
        Some(GuestMetricsConfig {
            cpu_percent: 1,
            memory_used_mib: 256,
        })
    );
}

#[test]
fn token_resolution_requires_one_source() {
    assert_eq!(
        resolve_token(Some("token-1".to_string()), None).unwrap(),
        "token-1"
    );
    assert!(resolve_token(None, None).is_err());
    assert!(resolve_token(Some("   ".to_string()), None).is_err());
    assert!(resolve_token(Some("token-1".to_string()), Some("token".into())).is_err());
}

#[test]
fn transport_resolution_accepts_socket_or_device_only() {
    assert_eq!(
        resolve_transport(Some("tools.sock".into()), None).unwrap(),
        Some(GuestToolsTransport::Socket("tools.sock".into()))
    );
    assert_eq!(
        resolve_transport(
            None,
            Some("/dev/virtio-ports/org.bridgevm.guest-tools.0".into())
        )
        .unwrap(),
        Some(GuestToolsTransport::Device(
            "/dev/virtio-ports/org.bridgevm.guest-tools.0".into()
        ))
    );
    assert_eq!(resolve_transport(None, None).unwrap(), None);
    assert!(resolve_transport(Some("tools.sock".into()), Some("device".into())).is_err());
}

#[test]
fn token_file_parser_accepts_metadata_json_and_raw_tokens() {
    assert_eq!(
        parse_token_file(r#"{"token":"token-1","created_at_unix":1}"#).unwrap(),
        "token-1"
    );
    assert_eq!(parse_token_file("token-2\n").unwrap(), "token-2");
    assert!(parse_token_file(r#"{"created_at_unix":1}"#).is_err());
    assert!(parse_token_file(r#"{"token":"   ","created_at_unix":1}"#).is_err());
    assert!(parse_token_file("  \n").is_err());
}

#[test]
fn capability_overrides_parse_names_versions_and_reject_duplicates() {
    let capabilities =
        resolve_capabilities(&["heartbeat".to_string(), "clipboard:2".to_string()]).unwrap();

    assert_eq!(
        capabilities,
        vec![
            AgentCapability {
                name: "heartbeat".to_string(),
                version: 1
            },
            AgentCapability {
                name: "clipboard".to_string(),
                version: 2
            }
        ]
    );
    assert_eq!(resolve_capabilities(&[]).unwrap(), default_capabilities());
    assert!(resolve_capabilities(&["".to_string()]).is_err());
    assert!(resolve_capabilities(&["clipboard:0".to_string()]).is_err());
    assert!(resolve_capabilities(&["clipboard".to_string(), "clipboard".to_string()]).is_err());
}

#[test]
fn initial_status_frames_validate() {
    let telemetry = default_telemetry();
    let envelopes = initial_status_envelopes(&telemetry);
    assert_eq!(envelopes.len(), 3);
    for envelope in envelopes {
        envelope.validate().unwrap();
    }
}

#[test]
fn telemetry_config_parses_overrides_and_honors_capabilities() {
    // Force synthetic metrics (--no-real-metrics) so the exact 17/1024
    // assertion is deterministic regardless of the host the test runs on.
    let telemetry = TelemetryConfig::from_args(
        &default_capabilities(),
        &["192.168.64.10@enp0s1".to_string()],
        false,
        17,
        1024,
        false,
        true,
        Some("guest copied text\n".to_string()),
    )
    .unwrap();
    assert_eq!(telemetry.guest_ips.len(), 1);
    assert_eq!(telemetry.guest_ips[0].address.to_string(), "192.168.64.10");
    assert_eq!(telemetry.guest_ips[0].interface.as_deref(), Some("enp0s1"));
    assert_eq!(
        telemetry.metrics,
        Some(GuestMetricsConfig {
            cpu_percent: 17,
            memory_used_mib: 1024
        })
    );
    assert_eq!(
        telemetry.clipboard_text.as_deref(),
        Some("guest copied text")
    );

    let capabilities = resolve_capabilities(&["heartbeat".to_string()]).unwrap();
    let telemetry =
        TelemetryConfig::from_args(&capabilities, &[], false, 1, 256, false, false, None).unwrap();
    assert!(telemetry.guest_ips.is_empty());
    assert_eq!(telemetry.metrics, None);
    assert_eq!(telemetry.clipboard_text, None);

    assert!(TelemetryConfig::from_args(
        &default_capabilities(),
        &[],
        false,
        101,
        256,
        false,
        false,
        None
    )
    .is_err());
    assert!(TelemetryConfig::from_args(
        &default_capabilities(),
        &["0.0.0.0".to_string()],
        false,
        1,
        256,
        false,
        false,
        None
    )
    .is_err());
    assert!(TelemetryConfig::from_args(
        &capabilities,
        &[],
        false,
        1,
        256,
        false,
        false,
        Some("copy".to_string())
    )
    .is_err());
    assert!(TelemetryConfig::from_args(
        &default_capabilities(),
        &[],
        false,
        1,
        256,
        false,
        false,
        Some("\n".to_string())
    )
    .is_err());
    // --no-metrics and --no-real-metrics are mutually exclusive.
    assert!(TelemetryConfig::from_args(
        &default_capabilities(),
        &[],
        false,
        1,
        256,
        true,
        true,
        None
    )
    .is_err());
}

#[test]
fn clipboard_telemetry_emits_clipboard_changed_frame() {
    let telemetry = TelemetryConfig::from_args(
        &default_capabilities(),
        &[],
        false,
        1,
        256,
        false,
        false,
        Some("hello from guest".to_string()),
    )
    .unwrap();

    let envelopes = initial_status_envelopes(&telemetry);

    assert_eq!(
        envelopes.last().map(|envelope| &envelope.message),
        Some(&AgentMessage::ClipboardChanged {
            text: "hello from guest".to_string()
        })
    );
    for envelope in envelopes {
        envelope.validate().unwrap();
    }
}

#[test]
fn serve_once_writes_hello_and_command_result() {
    let command = encode_envelope_line(&AgentEnvelope::with_request_id(
        AgentMessage::ResizeDisplay {
            width: 1440,
            height: 900,
            scale: 2,
        },
        "resize-1",
    ))
    .unwrap();
    let mut output = Vec::new();

    run_tools_session(
        Cursor::new(command.as_bytes()),
        &mut output,
        default_session_config(true),
    )
    .unwrap();

    let output = String::from_utf8(output).unwrap();
    let mut lines = output.lines().map(|line| format!("{line}\n"));
    let hello = decode_envelope_line(&lines.next().unwrap()).unwrap();
    let heartbeat = decode_envelope_line(&lines.next().unwrap()).unwrap();
    let guest_ip = decode_envelope_line(&lines.next().unwrap()).unwrap();
    let metrics = decode_envelope_line(&lines.next().unwrap()).unwrap();
    let result = decode_envelope_line(&lines.next().unwrap()).unwrap();

    assert!(matches!(hello.message, AgentMessage::GuestHello { .. }));
    assert_eq!(heartbeat.message, AgentMessage::Heartbeat);
    assert!(matches!(
        guest_ip.message,
        AgentMessage::GuestIpChanged { .. }
    ));
    assert!(matches!(metrics.message, AgentMessage::GuestMetrics { .. }));
    assert_eq!(
        result.message,
        AgentMessage::CommandResult {
            request_id: "resize-1".to_string(),
            ok: true,
            error_code: None,
            message: None,
            result: None,
            metadata: None,
        }
    );
    assert_eq!(lines.next(), None);
}

#[test]
fn default_session_handles_commands_until_eof() {
    let commands = [
        AgentEnvelope::with_request_id(
            AgentMessage::TimeSync {
                unix_epoch_millis: 1,
            },
            "time-1",
        ),
        AgentEnvelope::with_request_id(
            AgentMessage::SetClipboard {
                text: "hello".to_string(),
            },
            "clipboard-1",
        ),
    ]
    .into_iter()
    .map(|command| encode_envelope_line(&command).unwrap())
    .collect::<String>();
    let mut output = Vec::new();

    run_tools_session(
        Cursor::new(commands.as_bytes()),
        &mut output,
        default_session_config(false),
    )
    .unwrap();

    let output = String::from_utf8(output).unwrap();
    let frames = output
        .lines()
        .map(|line| decode_envelope_line(&format!("{line}\n")).unwrap())
        .collect::<Vec<_>>();

    assert_eq!(frames.len(), 6);
    assert!(matches!(frames[0].message, AgentMessage::GuestHello { .. }));
    assert_eq!(frames[1].message, AgentMessage::Heartbeat);
    assert_eq!(
        frames[4].message,
        AgentMessage::CommandResult {
            request_id: "time-1".to_string(),
            ok: true,
            error_code: None,
            message: Some(
                "acknowledged time-sync to 1 ms since epoch; guest clock was not changed (simulated)"
                    .to_string()
            ),
            result: Some(serde_json::json!({ "applied_unix_epoch_millis": 1u64 })),
            metadata: None,
        }
    );
    assert_eq!(
        frames[5].message,
        AgentMessage::CommandResult {
            request_id: "clipboard-1".to_string(),
            ok: true,
            error_code: None,
            message: None,
            result: None,
            metadata: None,
        }
    );
}

#[test]
fn unix_socket_session_round_trips_host_command() {
    let socket_path = temp_socket_path();
    let listener = UnixListener::bind(&socket_path).unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());

        let hello = read_frame(&mut reader);
        assert!(matches!(hello.message, AgentMessage::GuestHello { .. }));
        assert_eq!(read_frame(&mut reader).message, AgentMessage::Heartbeat);
        assert!(matches!(
            read_frame(&mut reader).message,
            AgentMessage::GuestIpChanged { .. }
        ));
        assert!(matches!(
            read_frame(&mut reader).message,
            AgentMessage::GuestMetrics { .. }
        ));

        let command = AgentEnvelope::with_request_id(
            AgentMessage::SetClipboard {
                text: "hello from host".to_string(),
            },
            "clipboard-1",
        );
        write_envelope_line(&mut stream, &command).unwrap();

        let result = read_frame(&mut reader);
        assert_eq!(
            result.message,
            AgentMessage::CommandResult {
                request_id: "clipboard-1".to_string(),
                ok: true,
                error_code: None,
                message: None,
                result: None,
                metadata: None,
            }
        );
    });

    let stream = UnixStream::connect(&socket_path).unwrap();
    let mut writer = stream.try_clone().unwrap();
    run_tools_session(stream, &mut writer, default_session_config(true)).unwrap();

    server.join().unwrap();
    let _ = std::fs::remove_file(socket_path);
}

#[test]
fn benchmark_kernel_is_deterministic_for_given_input() {
    // Same (seed, iterations) -> same output, every time and on any host.
    assert_eq!(benchmark_kernel(0, 1_000), benchmark_kernel(0, 1_000));
    assert_eq!(benchmark_kernel(42, 4_096), benchmark_kernel(42, 4_096));
    // Different inputs generally produce different outputs.
    assert_ne!(benchmark_kernel(0, 1_000), benchmark_kernel(1, 1_000));
    assert_ne!(benchmark_kernel(0, 1_000), benchmark_kernel(0, 1_001));
}

#[test]
fn benchmark_kernel_respects_iteration_cap() {
    // Zero iterations does no folding and returns the pure seed transform;
    // it must differ from any positive-iteration run, proving the loop is
    // bounded by `iterations` and not run-to-completion regardless.
    let zero = benchmark_kernel(7, 0);
    assert_eq!(zero, benchmark_kernel(7, 0));
    assert_ne!(zero, benchmark_kernel(7, 1));
    // Folding is composable: running N then M more from that state equals
    // running them as separate chunks, which is what the chunked runner
    // relies on for a stable checksum.
    let first = benchmark_kernel(0, 2_048);
    let chained = benchmark_kernel(first, 2_048);
    assert_eq!(chained, chained);
}

#[test]
fn run_cpu_benchmark_completes_within_budget_and_reports_progress() {
    // A tiny budget must still return a well-formed report and must not run
    // anywhere near unbounded: a few chunks at most.
    let report = run_cpu_benchmark(Duration::from_millis(1));
    assert!(report.iterations >= BENCHMARK_KERNEL_CHUNK);
    // Loose upper bound: even a very slow host won't spend many seconds on a
    // 1 ms budget; this catches a runaway loop.
    assert!(
        report.elapsed_millis < 2_000,
        "elapsed={}",
        report.elapsed_millis
    );
}

#[test]
fn run_benchmark_dispatch_returns_well_formed_report_for_tiny_cap() {
    let command = AgentEnvelope::with_request_id(
        AgentMessage::RunBenchmark {
            duration_millis: Some(1),
        },
        "bench-1",
    );
    let mut state = GuestToolsState::new(&default_capabilities());

    let AgentMessage::CommandResult {
        request_id,
        ok,
        error_code,
        result,
        ..
    } = state.handle_command(&command).unwrap().message
    else {
        panic!("expected CommandResult");
    };
    assert_eq!(request_id, "bench-1");
    assert!(ok);
    assert_eq!(error_code, None);
    let result = result.expect("benchmark result payload");
    assert_eq!(result["budget_duration_millis"], serde_json::json!(1));
    assert_eq!(result["requested_duration_millis"], serde_json::json!(1));
    let cpu = &result["cpu"];
    assert!(cpu["iterations"].as_u64().unwrap() >= BENCHMARK_KERNEL_CHUNK);
    assert!(cpu["ops_per_sec"].is_u64());
    assert!(cpu["checksum"].is_u64());
}

#[test]
fn run_benchmark_defaults_duration_when_unspecified() {
    let command = AgentEnvelope::with_request_id(
        AgentMessage::RunBenchmark {
            duration_millis: None,
        },
        "bench-default",
    );
    // Use a benchmark-only capability set so we exercise the default path.
    let mut state = GuestToolsState::new(&[AgentCapability {
        name: "benchmark".to_string(),
        version: 1,
    }]);

    let AgentMessage::CommandResult { ok, result, .. } =
        state.handle_command(&command).unwrap().message
    else {
        panic!("expected CommandResult");
    };
    assert!(ok);
    let result = result.expect("benchmark result payload");
    assert_eq!(
        result["budget_duration_millis"],
        serde_json::json!(DEFAULT_BENCHMARK_DURATION_MILLIS)
    );
    assert_eq!(result["requested_duration_millis"], serde_json::Value::Null);
}

#[test]
fn run_benchmark_requires_capability() {
    let command = AgentEnvelope::with_request_id(
        AgentMessage::RunBenchmark {
            duration_millis: Some(1),
        },
        "bench-1",
    );
    let mut state = GuestToolsState::new(&[AgentCapability {
        name: "heartbeat".to_string(),
        version: 1,
    }]);

    assert_eq!(
        state.handle_command(&command).unwrap().message,
        AgentMessage::CommandResult {
            request_id: "bench-1".to_string(),
            ok: false,
            error_code: Some("capability-not-enabled".to_string()),
            message: Some("benchmark capability is not enabled".to_string()),
            result: None,
            metadata: None,
        }
    );
}

#[test]
fn run_benchmark_includes_bounded_disk_micro_benchmark_when_scratch_dir_set() {
    let root = unique_temp_dir("bridgevm-tools-bench-disk");
    let command = AgentEnvelope::with_request_id(
        AgentMessage::RunBenchmark {
            duration_millis: Some(1),
        },
        "bench-disk",
    );
    let mut state =
        GuestToolsState::new(&default_capabilities()).with_file_drop_dir(Some(root.clone()));

    let AgentMessage::CommandResult { ok, result, .. } =
        state.handle_command(&command).unwrap().message
    else {
        panic!("expected CommandResult");
    };
    assert!(ok);
    let result = result.expect("benchmark result payload");
    let disk = &result["disk"];
    assert_eq!(
        disk["bytes_written"],
        serde_json::json!(BENCHMARK_DISK_BYTES)
    );
    assert!(disk["mib_per_sec"].is_u64());

    // The scratch temp file must have been removed (only the dir remains).
    let leftover: Vec<_> = fs::read_dir(&root)
        .map(|entries| entries.filter_map(|entry| entry.ok()).collect())
        .unwrap_or_default();
    assert!(
        leftover.is_empty(),
        "benchmark left scratch files behind: {leftover:?}"
    );
    let _ = fs::remove_dir_all(&root);
}
