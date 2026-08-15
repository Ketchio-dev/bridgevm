//! Split test module.

use super::helpers::*;
use crate::*;
use serde_json::json;
use std::fs;
use std::io::BufRead;
use std::io::BufReader;
use std::io::Write;
use std::os::unix::net::UnixListener;
use std::thread;
use std::time::Duration;

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
