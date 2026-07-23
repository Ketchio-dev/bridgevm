//! Split test module.

use crate::*;
use bridgevm_config::Guest;
use bridgevm_config::VmManifest;
use bridgevm_config::VmMode;
use serde_json::json;
use std::fs;
use std::io::BufRead;
use std::io::BufReader;
use std::io::Write;
use std::os::unix::net::UnixListener;
use std::path::Path;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

use super::helpers::*;

#[test]
fn builds_qemu_img_info_json_command() {
    let command = QemuImgCommand::info_json(Path::new("/tmp/root.qcow2"));

    assert_eq!(command.program, "qemu-img");
    assert_eq!(command.args, ["info", "--output=json", "/tmp/root.qcow2"]);
    assert_eq!(
        command.render_shell_words(),
        ["qemu-img", "info", "--output=json", "/tmp/root.qcow2"]
    );
}

#[test]
fn builds_qemu_img_check_json_command() {
    let command = QemuImgCommand::check_json(Path::new("/tmp/root.qcow2"));

    assert_eq!(command.program, "qemu-img");
    assert_eq!(command.args, ["check", "--output=json", "/tmp/root.qcow2"]);
    assert_eq!(
        command.render_shell_words(),
        ["qemu-img", "check", "--output=json", "/tmp/root.qcow2"]
    );
}

#[test]
fn builds_qemu_img_compact_convert_command() {
    let command = QemuImgCommand::convert_compact(
        Path::new("/tmp/root.qcow2"),
        Path::new("/tmp/root.qcow2.compact.tmp"),
        "qcow2",
    );

    assert_eq!(command.program, "qemu-img");
    assert_eq!(
        command.args,
        [
            "convert",
            "-O",
            "qcow2",
            "/tmp/root.qcow2",
            "/tmp/root.qcow2.compact.tmp"
        ]
    );
    assert_eq!(
        command.render_shell_words(),
        [
            "qemu-img",
            "convert",
            "-O",
            "qcow2",
            "/tmp/root.qcow2",
            "/tmp/root.qcow2.compact.tmp"
        ]
    );
}

#[test]
fn builds_qemu_img_backed_disk_command() {
    let command = QemuImgCommand::create_backed_disk(
        Path::new("/tmp/snap.qcow2"),
        "qcow2",
        "qcow2",
        Path::new("/tmp/root.qcow2"),
    );

    assert_eq!(command.program, "qemu-img");
    assert_eq!(
        command.args,
        [
            "create",
            "-f",
            "qcow2",
            "-F",
            "qcow2",
            "-b",
            "/tmp/root.qcow2",
            "/tmp/snap.qcow2"
        ]
    );
}

#[test]
fn rejects_fast_mode_manifest() {
    let manifest = VmManifest::new(
        "fast",
        VmMode::Fast,
        Guest {
            os: "ubuntu".to_string(),
            version: None,
            arch: "arm64".to_string(),
        },
        "80GiB",
    );
    assert!(build_compatibility_command(&manifest, Path::new("/tmp/fast.vmbridge")).is_err());
}

#[test]
fn compatibility_command_names_primary_block_node() {
    let manifest = VmManifest::new(
        "compat",
        VmMode::Compatibility,
        Guest {
            os: "ubuntu".to_string(),
            version: None,
            arch: "x86_64".to_string(),
        },
        "40GiB",
    );
    let command =
        build_compatibility_command(&manifest, Path::new("/tmp/compat.vmbridge")).unwrap();
    let drive = command
        .args
        .iter()
        .find(|arg| arg.contains("if=virtio"))
        .expect("primary drive arg present");
    assert!(
        drive.contains(&format!("node-name={COMPAT_PRIMARY_BLOCK_NODE}")),
        "drive arg should name the primary block node: {drive}"
    );
}

#[test]
fn builds_snapshot_save_command_for_suspend() {
    let devices = vec![COMPAT_PRIMARY_BLOCK_NODE.to_string()];
    let command = QmpCommand::snapshot_save(
        COMPAT_SUSPEND_SNAPSHOT_TAG,
        COMPAT_SUSPEND_SNAPSHOT_TAG,
        COMPAT_PRIMARY_BLOCK_NODE,
        &devices,
    );
    assert_eq!(
        serde_json::to_value(&command).unwrap(),
        json!({
            "execute": "snapshot-save",
            "arguments": {
                "job-id": "bridgevm-suspend",
                "tag": "bridgevm-suspend",
                "vmstate": "bridgevm-root",
                "devices": ["bridgevm-root"],
            }
        })
    );
}

#[test]
fn builds_snapshot_load_command_for_resume() {
    let devices = vec![COMPAT_PRIMARY_BLOCK_NODE.to_string()];
    let command = QmpCommand::snapshot_load(
        COMPAT_SUSPEND_SNAPSHOT_TAG,
        COMPAT_SUSPEND_SNAPSHOT_TAG,
        COMPAT_PRIMARY_BLOCK_NODE,
        &devices,
    );
    assert_eq!(
        serde_json::to_value(&command).unwrap(),
        json!({
            "execute": "snapshot-load",
            "arguments": {
                "job-id": "bridgevm-suspend",
                "tag": "bridgevm-suspend",
                "vmstate": "bridgevm-root",
                "devices": ["bridgevm-root"],
            }
        })
    );
}

#[test]
fn builds_query_jobs_command() {
    assert_eq!(
        serde_json::to_value(QmpCommand::query_jobs()).unwrap(),
        json!({ "execute": "query-jobs" })
    );
}

#[test]
fn job_status_terminal_classification() {
    assert!(job_status_is_terminal("concluded"));
    assert!(job_status_is_terminal("aborting"));
    assert!(!job_status_is_terminal("running"));
    assert!(!job_status_is_terminal("created"));
}

#[test]
fn wait_for_job_returns_when_job_concludes() {
    let socket_path = temp_socket_path();
    let listener = UnixListener::bind(&socket_path).unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .write_all(br#"{"QMP":{"version":{"qemu":{"major":8,"minor":2,"micro":0}}}}"#)
            .unwrap();
        stream.write_all(b"\n").unwrap();

        let mut reader = BufReader::new(stream.try_clone().unwrap());
        // capabilities
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        assert!(line.contains("qmp_capabilities"));
        stream.write_all(br#"{"return":{}}"#).unwrap();
        stream.write_all(b"\n").unwrap();

        // first query-jobs: still running
        line.clear();
        reader.read_line(&mut line).unwrap();
        assert!(line.contains("query-jobs"));
        stream
            .write_all(br#"{"return":[{"id":"bridgevm-suspend","status":"running"}]}"#)
            .unwrap();
        stream.write_all(b"\n").unwrap();

        // second query-jobs: concluded with no error
        line.clear();
        reader.read_line(&mut line).unwrap();
        assert!(line.contains("query-jobs"));
        stream
            .write_all(br#"{"return":[{"id":"bridgevm-suspend","status":"concluded"}]}"#)
            .unwrap();
        stream.write_all(b"\n").unwrap();
    });

    let mut client = QmpClient::connect(&socket_path).unwrap();
    client.negotiate().unwrap();
    wait_for_job(
        &mut client,
        COMPAT_SUSPEND_SNAPSHOT_TAG,
        Duration::from_secs(2),
    )
    .unwrap();

    server.join().unwrap();
    fs::remove_file(socket_path).unwrap();
}

#[test]
fn wait_for_job_surfaces_job_error() {
    let socket_path = temp_socket_path();
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

        line.clear();
        reader.read_line(&mut line).unwrap();
        assert!(line.contains("query-jobs"));
        stream
            .write_all(
                br#"{"return":[{"id":"bridgevm-suspend","status":"concluded","error":"disk full"}]}"#,
            )
            .unwrap();
        stream.write_all(b"\n").unwrap();
    });

    let mut client = QmpClient::connect(&socket_path).unwrap();
    client.negotiate().unwrap();
    let error = wait_for_job(
        &mut client,
        COMPAT_SUSPEND_SNAPSHOT_TAG,
        Duration::from_secs(2),
    )
    .unwrap_err();
    assert!(error.to_string().contains("disk full"));

    server.join().unwrap();
    fs::remove_file(socket_path).unwrap();
}

#[test]
fn serializes_qmp_commands() {
    assert_eq!(
        serde_json::to_value(QmpCommand::query_status()).unwrap(),
        json!({ "execute": "query-status" })
    );
    assert_eq!(
        serde_json::to_value(QmpCommand::stop()).unwrap(),
        json!({ "execute": "stop" })
    );
    assert_eq!(
        serde_json::to_value(QmpCommand::cont()).unwrap(),
        json!({ "execute": "cont" })
    );
    assert_eq!(
        serde_json::to_value(QmpCommand::quit()).unwrap(),
        json!({ "execute": "quit" })
    );
}

#[test]
fn exposes_qmp_socket_path() {
    assert_eq!(
        qmp_socket_path(Path::new("/tmp/example.vmbridge")),
        PathBuf::from("/tmp/example.vmbridge/metadata/qmp.sock")
    );
}

#[test]
fn exposes_guest_tools_socket_path() {
    assert_eq!(
        guest_tools_socket_path(Path::new("/tmp/example.vmbridge")),
        PathBuf::from("/tmp/example.vmbridge/metadata/guest-tools.sock")
    );
}

#[test]
fn qmp_status_ignores_async_events_before_command_return() {
    let socket_path = temp_socket_path();
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

        line.clear();
        reader.read_line(&mut line).unwrap();
        assert!(line.contains("query-status"));
        stream
            .write_all(br#"{"event":"SHUTDOWN","data":{"guest":true}}"#)
            .unwrap();
        stream.write_all(b"\n").unwrap();
        stream
            .write_all(br#"{"return":{"status":"shutdown","running":false}}"#)
            .unwrap();
        stream.write_all(b"\n").unwrap();
    });

    let status = query_status(&socket_path).unwrap();
    assert_eq!(status.status, "shutdown");
    assert!(!status.running);
    assert!(status.is_terminal());

    server.join().unwrap();
    fs::remove_file(socket_path).unwrap();
}

#[test]
fn qmp_stop_and_cont_round_trip_over_fake_socket() {
    for (command_name, execute) in [
        (
            "stop",
            stop as fn(&Path) -> std::result::Result<(), QemuError>,
        ),
        (
            "cont",
            cont as fn(&Path) -> std::result::Result<(), QemuError>,
        ),
    ] {
        let socket_path = temp_socket_path();
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

            line.clear();
            reader.read_line(&mut line).unwrap();
            assert!(line.contains(command_name));
            stream.write_all(br#"{"return":{}}"#).unwrap();
            stream.write_all(b"\n").unwrap();
        });

        execute(&socket_path).unwrap();

        server.join().unwrap();
        fs::remove_file(socket_path).unwrap();
    }
}

#[test]
fn qmp_client_can_read_terminal_event() {
    let socket_path = temp_socket_path();
    let listener = UnixListener::bind(&socket_path).unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .write_all(br#"{"QMP":{"version":{"qemu":{"major":8,"minor":2,"micro":0}}}}"#)
            .unwrap();
        stream.write_all(b"\n").unwrap();
        stream
            .write_all(br#"{"event":"SHUTDOWN","data":{"guest":true}}"#)
            .unwrap();
        stream.write_all(b"\n").unwrap();
    });

    let mut client = QmpClient::connect(&socket_path).unwrap();
    let event = client.read_event().unwrap();

    assert_eq!(event.name, "SHUTDOWN");
    assert_eq!(event.data.as_ref().unwrap(), &json!({ "guest": true }));
    assert!(event.is_terminal());

    server.join().unwrap();
    fs::remove_file(socket_path).unwrap();
}

#[test]
fn qmp_client_rejects_oversized_envelope() {
    let socket_path = temp_socket_path();
    let listener = UnixListener::bind(&socket_path).unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let oversized = vec![b'x'; MAX_QMP_ENVELOPE_BYTES as usize + 1];
        let _ = stream.write_all(&oversized);
    });

    let mut client = QmpClient::connect(&socket_path).unwrap();
    let error = client.read_envelope().unwrap_err();
    assert!(error.to_string().contains("exceeded 1048576 bytes"));

    server.join().unwrap();
    fs::remove_file(socket_path).unwrap();
}

#[test]
fn qmp_client_rejects_incomplete_envelope() {
    let socket_path = temp_socket_path();
    let listener = UnixListener::bind(&socket_path).unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream.write_all(br#"{"event":"SHUTDOWN"}"#).unwrap();
    });

    let mut client = QmpClient::connect(&socket_path).unwrap();
    let error = client.read_envelope().unwrap_err();
    assert!(error.to_string().contains("incomplete envelope"));

    server.join().unwrap();
    fs::remove_file(socket_path).unwrap();
}

#[test]
fn qmp_execute_rejects_event_flood_before_command_return() {
    let socket_path = temp_socket_path();
    let listener = UnixListener::bind(&socket_path).unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream.write_all(b"{\"QMP\":{}}\n").unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut command = String::new();
        reader.read_line(&mut command).unwrap();
        assert!(command.contains("qmp_capabilities"));
        stream.write_all(b"{\"return\":{}}\n").unwrap();

        command.clear();
        reader.read_line(&mut command).unwrap();
        assert!(command.contains("query-status"));
        for _ in 0..MAX_QMP_SKIPPED_ENVELOPES {
            if stream.write_all(b"{\"event\":\"RESUME\"}\n").is_err() {
                break;
            }
        }
    });

    let mut client = QmpClient::connect(&socket_path).unwrap();
    client.negotiate().unwrap();
    let error = client.execute(QmpCommand::query_status()).unwrap_err();
    assert!(error
        .to_string()
        .contains("skipped more than 1024 event envelopes"));

    server.join().unwrap();
    fs::remove_file(socket_path).unwrap();
}

#[test]
fn qmp_event_wait_rejects_non_event_flood() {
    let socket_path = temp_socket_path();
    let listener = UnixListener::bind(&socket_path).unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        for _ in 0..MAX_QMP_SKIPPED_ENVELOPES {
            if stream.write_all(b"{\"return\":{}}\n").is_err() {
                break;
            }
        }
    });

    let mut client = QmpClient::connect(&socket_path).unwrap();
    let error = client.read_event().unwrap_err();
    assert!(error
        .to_string()
        .contains("skipped more than 1024 non-event envelopes"));

    server.join().unwrap();
    fs::remove_file(socket_path).unwrap();
}

#[test]
fn qmp_client_drains_available_events_until_terminal() {
    let socket_path = temp_socket_path();
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

        stream.write_all(br#"{"event":"RESUME"}"#).unwrap();
        stream.write_all(b"\n").unwrap();
        stream
            .write_all(br#"{"event":"SHUTDOWN","data":{"guest":true}}"#)
            .unwrap();
        stream.write_all(b"\n").unwrap();
        stream.write_all(br#"{"event":"RESUME"}"#).unwrap();
        stream.write_all(b"\n").unwrap();
    });

    let mut client =
        QmpClient::connect_with_timeout(&socket_path, Duration::from_millis(25)).unwrap();
    client.negotiate().unwrap();
    let drain = client.drain_events(8).unwrap();

    assert_eq!(drain.envelopes_read, 2);
    assert_eq!(
        drain
            .events
            .iter()
            .map(|event| event.name.as_str())
            .collect::<Vec<_>>(),
        ["RESUME", "SHUTDOWN"]
    );
    assert_eq!(
        drain
            .terminal_event
            .as_ref()
            .unwrap()
            .data
            .as_ref()
            .unwrap(),
        &json!({ "guest": true })
    );
    assert!(drain.has_terminal_event());
    assert!(!drain.limit_reached);

    server.join().unwrap();
    fs::remove_file(socket_path).unwrap();
}

#[test]
fn qmp_client_drain_treats_idle_socket_as_empty() {
    let socket_path = temp_socket_path();
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

        thread::sleep(Duration::from_millis(100));
    });

    let mut client =
        QmpClient::connect_with_timeout(&socket_path, Duration::from_millis(25)).unwrap();
    client.negotiate().unwrap();
    let drain = client.drain_events(8).unwrap();

    assert!(drain.events.is_empty());
    assert_eq!(drain.envelopes_read, 0);
    assert!(!drain.has_terminal_event());
    assert!(!drain.limit_reached);

    server.join().unwrap();
    fs::remove_file(socket_path).unwrap();
}
