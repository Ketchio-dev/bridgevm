//! Raw swtpm backend tests: the control-channel power cycle, and the guest-visible
//! TIS result of data- and control-socket failures.

use super::*;
use crate::tpm_tis::{TpmTis, ACCESS_ACTIVE_LOCALITY, ACCESS_REQUEST_USE, LOCALITY_SIZE};
use crate::tpm_tis::{REG_ACCESS, REG_DATA_FIFO, REG_STS, TPM2_RC_FAILURE};
use crate::tpm_tis::{STS_COMMAND_READY, STS_DATA_AVAILABLE, STS_TPM_GO};
use std::thread::JoinHandle;

const READ_CLOCK: [u8; TPM2_HEADER_SIZE] = [0x80, 0x01, 0, 0, 0, 10, 0, 0, 0x01, 0x81];
const GET_TEST_RESULT: [u8; TPM2_HEADER_SIZE] = [0x80, 0x01, 0, 0, 0, 10, 0, 0, 0x01, 0x7c];
const SUCCESS: [u8; TPM2_HEADER_SIZE] = [0x80, 0x01, 0, 0, 0, 10, 0, 0, 0, 0];
/// A complete success response. A stream that lost its framing hands it to the
/// guest as the answer to whichever command is sent next.
const STALE_SUCCESS: [u8; 14] = [0x80, 0x01, 0, 0, 0, 14, 0, 0, 0, 0, 0, 2, 0xa5, 0x5a];

fn response_header(declared_size: usize) -> Vec<u8> {
    let mut header = SUCCESS.to_vec();
    header[2..6].copy_from_slice(&(declared_size as u32).to_be_bytes());
    header
}

/// A TIS with locality 0 active over a raw swtpm backend whose data and
/// control peers the test plays. Both backend sockets use `timeout`.
fn tis_with_peers(timeout: Duration) -> (TpmTis, UnixStream, UnixStream) {
    let (data, data_peer) = UnixStream::pair().unwrap();
    let (control, control_peer) = UnixStream::pair().unwrap();
    for stream in [&data, &control] {
        stream.set_read_timeout(Some(timeout)).unwrap();
        stream.set_write_timeout(Some(timeout)).unwrap();
    }
    for peer in [&data_peer, &control_peer] {
        peer.set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
    }
    let backend = SwtpmUnixBackend::from_streams(data, Some(control));
    let mut tis = TpmTis::new(Box::new(backend));
    tis.mmio_write(REG_ACCESS, 1, ACCESS_REQUEST_USE as u64);
    (tis, data_peer, control_peer)
}

/// Send one command through the guest-visible FIFO of `locality` and read the
/// response until the TIS stops reporting data available.
fn fifo_command(tis: &mut TpmTis, locality: u8, command: &[u8]) -> Vec<u8> {
    let base = u64::from(locality) * LOCALITY_SIZE;
    tis.mmio_write(base + REG_STS, 1, STS_COMMAND_READY as u64);
    for &byte in command {
        tis.mmio_write(base + REG_DATA_FIFO, 1, u64::from(byte));
    }
    tis.mmio_write(base + REG_STS, 1, STS_TPM_GO as u64);
    let mut response = Vec::new();
    while tis.mmio_read(base + REG_STS, 1) as u8 & STS_DATA_AVAILABLE != 0 {
        assert!(
            response.len() < MAX_BUFFER_SIZE,
            "the TIS response never completed"
        );
        response.push(tis.mmio_read(base + REG_DATA_FIFO, 1) as u8);
    }
    response
}

/// Play swtpm's data socket: answer command `n` with `responses[n]` (the last
/// entry repeats) until the backend closes it; return how many commands came.
fn serve_data(mut peer: UnixStream, responses: Vec<Vec<u8>>) -> JoinHandle<usize> {
    std::thread::spawn(move || {
        let mut command = [0u8; TPM2_HEADER_SIZE];
        let mut received = 0;
        while peer.read_exact(&mut command).is_ok() {
            peer.write_all(&responses[received.min(responses.len() - 1)])
                .unwrap();
            received += 1;
        }
        received
    })
}

/// Play swtpm's control socket: answer every CMD_INIT with `result` until the
/// backend closes it; return how many requests came.
fn serve_control(mut peer: UnixStream, result: u32) -> JoinHandle<usize> {
    std::thread::spawn(move || {
        let mut request = [0u8; 8];
        let mut received = 0;
        while peer.read_exact(&mut request).is_ok() {
            assert_eq!(request, [0, 0, 0, 2, 0, 0, 0, 0], "CMD_INIT, init_flags 0");
            peer.write_all(&result.to_be_bytes()).unwrap();
            received += 1;
        }
        received
    })
}

#[test]
fn timeout_mid_response_fails_this_and_later_commands_without_stale_data() {
    // swtpm sends the header, then the body only after a later command shows
    // that the backend reused the stream once its read had timed out.
    let (mut tis, data_peer, _control_peer) = tis_with_peers(Duration::from_millis(200));
    let header = response_header(TPM2_HEADER_SIZE + STALE_SUCCESS.len());
    let swtpm = serve_data(data_peer, vec![header, STALE_SUCCESS.to_vec()]);

    assert_eq!(fifo_command(&mut tis, 0, &READ_CLOCK), TPM2_RC_FAILURE);
    assert_eq!(fifo_command(&mut tis, 0, &GET_TEST_RESULT), TPM2_RC_FAILURE);
    let stats = tis.stats();
    drop(tis);
    assert_eq!(
        swtpm.join().unwrap(),
        1,
        "only the first command reaches swtpm"
    );
    assert_eq!(
        (
            stats.commands,
            stats.backend_failures,
            stats.error_responses
        ),
        (2, 2, 2)
    );
    assert_eq!(stats.successful_responses, 0);
}

#[test]
fn out_of_range_response_size_fails_this_and_later_commands_without_stale_data() {
    for declared in [MAX_BUFFER_SIZE + 1, TPM2_HEADER_SIZE - 1] {
        let (mut tis, data_peer, _control_peer) = tis_with_peers(Duration::from_secs(5));
        // The bytes after the refused header parse as a complete success.
        let mut response = response_header(declared);
        response.extend_from_slice(&STALE_SUCCESS);
        let swtpm = serve_data(data_peer, vec![response, Vec::new()]);

        let first = fifo_command(&mut tis, 0, &READ_CLOCK);
        assert_eq!(first, TPM2_RC_FAILURE, "declared size {declared}");
        let second = fifo_command(&mut tis, 0, &GET_TEST_RESULT);
        assert_eq!(second, TPM2_RC_FAILURE, "declared size {declared}");
        drop(tis);
        assert_eq!(swtpm.join().unwrap(), 1, "declared size {declared}");
    }
}

#[test]
fn failed_cmd_init_refuses_later_commands_and_resets() {
    let (mut tis, data_peer, control_peer) = tis_with_peers(Duration::from_secs(5));
    let swtpm = serve_data(data_peer, vec![SUCCESS.to_vec()]);
    let control = serve_control(control_peer, 1);

    tis.reset()
        .expect_err("a rejected CMD_INIT fails the reset");
    // After a failed power cycle swtpm may still hold the previous boot's
    // volatile state, so no command may reach it.
    assert_eq!(fifo_command(&mut tis, 0, &READ_CLOCK), TPM2_RC_FAILURE);
    tis.reset()
        .expect_err("a failed backend is not power-cycled again");
    drop(tis);
    assert_eq!(swtpm.join().unwrap(), 0, "no command reaches swtpm");
    assert_eq!(
        control.join().unwrap(),
        1,
        "only the first CMD_INIT is sent"
    );
}

#[test]
fn reset_after_data_failure_is_refused_without_cmd_init() {
    let (mut tis, data_peer, control_peer) = tis_with_peers(Duration::from_secs(5));
    let swtpm = serve_data(data_peer, vec![response_header(MAX_BUFFER_SIZE + 1)]);
    let control = serve_control(control_peer, 0);

    assert_eq!(fifo_command(&mut tis, 0, &READ_CLOCK), TPM2_RC_FAILURE);
    // CMD_INIT does not drain the data socket, so it cannot restore pairing.
    tis.reset()
        .expect_err("a reset cannot revive a desynchronised data socket");
    drop(tis);
    assert_eq!(swtpm.join().unwrap(), 1);
    assert_eq!(control.join().unwrap(), 0, "no CMD_INIT is sent");
}

#[test]
fn refused_locality_does_not_latch_the_backend() {
    let (mut tis, data_peer, _control_peer) = tis_with_peers(Duration::from_secs(5));
    let swtpm = serve_data(data_peer, vec![SUCCESS.to_vec()]);

    // The raw data socket carries locality 0 only; locality 1 is refused
    // before any socket I/O, so the stream stays paired.
    tis.mmio_write(REG_ACCESS, 1, ACCESS_ACTIVE_LOCALITY as u64);
    tis.mmio_write(LOCALITY_SIZE + REG_ACCESS, 1, ACCESS_REQUEST_USE as u64);
    assert_eq!(fifo_command(&mut tis, 1, &READ_CLOCK), TPM2_RC_FAILURE);
    tis.mmio_write(LOCALITY_SIZE + REG_ACCESS, 1, ACCESS_ACTIVE_LOCALITY as u64);
    tis.mmio_write(REG_ACCESS, 1, ACCESS_REQUEST_USE as u64);
    assert_eq!(fifo_command(&mut tis, 0, &READ_CLOCK), SUCCESS);
    drop(tis);
    assert_eq!(swtpm.join().unwrap(), 1);
}

#[test]
fn swtpm_backend_reset_power_cycles_via_control_socket_cmd_init() {
    use std::io::{Read, Write};
    use std::os::unix::net::UnixListener;
    use std::thread;

    let dir = std::env::temp_dir().join(format!(
        "bridgevm-swtpm-ctrl-{}-{}",
        std::process::id(),
        std::time::Instant::now().elapsed().as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let data_path = dir.join("data.sock");
    let control_path = dir.join("control.sock");
    let data_listener = UnixListener::bind(&data_path).unwrap();
    let control_listener = UnixListener::bind(&control_path).unwrap();

    // The data server just accepts and holds the connection; the backend
    // does not send data-channel traffic during reset.
    let data_thread = thread::spawn(move || {
        let (_stream, _addr) = data_listener.accept().unwrap();
        // Keep the connection open until the test drops the backend.
        thread::sleep(Duration::from_millis(200));
    });

    // The control server captures the CMD_INIT request and replies success.
    let control_thread = thread::spawn(move || {
        let (mut stream, _addr) = control_listener.accept().unwrap();
        let mut request = [0u8; 8];
        stream.read_exact(&mut request).unwrap();
        stream.write_all(&0u32.to_be_bytes()).unwrap();
        request
    });

    let mut backend =
        SwtpmUnixBackend::connect_with_control(&data_path, Some(&control_path)).unwrap();
    backend.reset().expect("power cycle should succeed");

    let request = control_thread.join().unwrap();
    data_thread.join().unwrap();

    // CMD_INIT (0x00000002) big-endian, then init_flags = 0.
    assert_eq!(request, [0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00]);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn swtpm_backend_reset_fails_closed_when_control_reports_error() {
    use std::io::{Read, Write};
    use std::os::unix::net::UnixListener;
    use std::thread;

    let dir = std::env::temp_dir().join(format!(
        "bridgevm-swtpm-ctrl-err-{}-{}",
        std::process::id(),
        std::time::Instant::now().elapsed().as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let data_path = dir.join("data.sock");
    let control_path = dir.join("control.sock");
    let data_listener = UnixListener::bind(&data_path).unwrap();
    let control_listener = UnixListener::bind(&control_path).unwrap();

    let data_thread = thread::spawn(move || {
        let (_stream, _addr) = data_listener.accept().unwrap();
        thread::sleep(Duration::from_millis(200));
    });
    let control_thread = thread::spawn(move || {
        let (mut stream, _addr) = control_listener.accept().unwrap();
        let mut request = [0u8; 8];
        stream.read_exact(&mut request).unwrap();
        // Non-zero control result → swtpm rejected the power cycle.
        stream.write_all(&1u32.to_be_bytes()).unwrap();
    });

    let mut backend =
        SwtpmUnixBackend::connect_with_control(&data_path, Some(&control_path)).unwrap();
    let err = backend
        .reset()
        .expect_err("non-zero control result must fail closed");
    assert!(matches!(err, TpmBackendError::Protocol(_)));

    control_thread.join().unwrap();
    data_thread.join().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}
