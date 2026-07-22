//! KD (kernel-debug) serial bridge: PL011 <-> host Unix-domain socket.
//!
//! Both remaining bring-up frontiers — the HDA codec (why hdaudio.sys aborts
//! its function-group start before reading SUBORDINATE_NODE_COUNT) and the
//! venus/viogpu3d WDDM driver (why it fails post-start) — are blocked on the
//! same missing tool: guest-kernel visibility. Windows kernel debugging
//! (KDCOM over serial) provides it. Our PL011 already models the register
//! surface KDCOM needs: writes to UARTDR are captured (guest TX) and UARTDR
//! reads consume queued bytes (guest RX), with UARTFR reporting non-blocking
//! idle FIFOs. This bridge connects that serial to a host socket so a WinDbg
//! instance (running on another machine or a VZ Windows VM) can attach.
//!
//! The debuggee side is this transport; the debugger side (WinDbg + Windows
//! configured with `bcdedit /debug on` + `/dbgsettings serial`) is external.
//! Enable with `BRIDGEVM_KD_SERIAL_SOCKET=<path>`: the probe binds a Unix
//! listener there, accepts one non-blocking connection, and on each drain
//! forwards guest-TX bytes to the socket and socket bytes to guest-RX.
//!
//! The pure byte-plumbing (`KdSerialPump`) is separated from the socket I/O so
//! it can be unit-tested without a live guest or a real WinDbg peer.

use std::io::{ErrorKind, Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};

use bridgevm_hvf::platform_virt::VirtPlatform;

/// The transfer a single bridge tick moves in each direction. Pure and
/// side-effect-free so it can be exercised without sockets or a guest.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct KdSerialTransfer {
    /// Guest transmit bytes to forward to the debugger socket.
    pub to_debugger: Vec<u8>,
    /// Debugger bytes to inject into the guest UART receive path.
    pub to_guest: Vec<u8>,
}

/// Pure byte pump: given what the guest emitted and what the debugger sent,
/// decide what crosses in each direction. Kept trivial on purpose — KDCOM does
/// its own framing/checksums end to end, so the transport is a raw byte pipe;
/// the value of isolating it is a deterministic unit test of the plumbing.
#[derive(Debug, Default)]
pub struct KdSerialPump;

impl KdSerialPump {
    pub fn tick(guest_tx: Vec<u8>, debugger_rx: Vec<u8>) -> KdSerialTransfer {
        KdSerialTransfer {
            to_debugger: guest_tx,
            to_guest: debugger_rx,
        }
    }
}

/// Live bridge over a Unix-domain socket. `None` (bridge disabled) unless
/// `BRIDGEVM_KD_SERIAL_SOCKET` is set.
pub struct KdSerialBridge {
    listener: UnixListener,
    path: PathBuf,
    peer: Option<UnixStream>,
    recv_scratch: [u8; 4096],
}

impl KdSerialBridge {
    /// Bind the listener from the env config, or return `None` when KD serial
    /// bridging is not requested.
    pub fn from_env() -> Option<Self> {
        let path = std::env::var_os("BRIDGEVM_KD_SERIAL_SOCKET")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)?;
        match Self::bind(&path) {
            Ok(bridge) => {
                eprintln!("kd-serial: listening on {}", path.display());
                Some(bridge)
            }
            Err(error) => {
                eprintln!("kd-serial: failed to bind {}: {error}", path.display());
                None
            }
        }
    }

    fn bind(path: &Path) -> std::io::Result<Self> {
        // A stale socket file from a previous run would make bind fail with
        // EADDRINUSE; the debuggee owns this path for the run's lifetime.
        let _ = std::fs::remove_file(path);
        let listener = UnixListener::bind(path)?;
        listener.set_nonblocking(true)?;
        Ok(Self {
            listener,
            path: path.to_path_buf(),
            peer: None,
            recv_scratch: [0u8; 4096],
        })
    }

    /// Move one tick of bytes in both directions. Non-blocking throughout: a
    /// missing/half-open debugger never stalls the vCPU drain that calls this.
    pub fn pump(&mut self, platform: &mut VirtPlatform) {
        self.accept_if_needed();
        let Some(mut peer) = self.peer.take() else {
            // No debugger yet: still drain guest TX so the UART buffer cannot
            // grow without bound while waiting for an attach.
            let _ = platform.take_uart_output();
            return;
        };

        let mut debugger_rx = Vec::new();
        let mut peer_alive = true;
        loop {
            match peer.read(&mut self.recv_scratch) {
                Ok(0) => {
                    peer_alive = false;
                    break;
                }
                Ok(n) => debugger_rx.extend_from_slice(&self.recv_scratch[..n]),
                Err(ref e) if e.kind() == ErrorKind::WouldBlock => break,
                Err(ref e) if e.kind() == ErrorKind::Interrupted => continue,
                Err(_) => {
                    peer_alive = false;
                    break;
                }
            }
        }

        if std::env::var_os("BRIDGEVM_TRACE_PL011").is_some() && !debugger_rx.is_empty() {
            eprintln!(
                "kd-serial: peer.read {} byte(s) from debugger first=0x{:02x}",
                debugger_rx.len(),
                debugger_rx[0]
            );
        }
        let transfer = KdSerialPump::tick(platform.take_uart_output(), debugger_rx);
        if !transfer.to_guest.is_empty() {
            platform.push_uart_input(&transfer.to_guest);
        }
        if peer_alive && !transfer.to_debugger.is_empty() {
            match peer.write_all(&transfer.to_debugger) {
                Ok(()) => {}
                Err(ref e) if e.kind() == ErrorKind::WouldBlock => {}
                Err(_) => peer_alive = false,
            }
        }

        if peer_alive {
            self.peer = Some(peer);
        } else {
            eprintln!("kd-serial: debugger disconnected");
        }
    }

    fn accept_if_needed(&mut self) {
        if self.peer.is_some() {
            return;
        }
        match self.listener.accept() {
            Ok((stream, _)) => {
                if stream.set_nonblocking(true).is_ok() {
                    eprintln!("kd-serial: debugger attached");
                    self.peer = Some(stream);
                }
            }
            Err(ref e) if e.kind() == ErrorKind::WouldBlock => {}
            Err(_) => {}
        }
    }
}

impl Drop for KdSerialBridge {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pump_forwards_guest_tx_to_debugger_and_debugger_rx_to_guest() {
        let transfer = KdSerialPump::tick(b"guest->dbg".to_vec(), b"dbg->guest".to_vec());
        assert_eq!(transfer.to_debugger, b"guest->dbg");
        assert_eq!(transfer.to_guest, b"dbg->guest");
    }

    #[test]
    fn pump_is_a_raw_byte_pipe_that_preserves_empty_directions() {
        let only_tx = KdSerialPump::tick(b"\x00\xff\x5a".to_vec(), Vec::new());
        assert_eq!(only_tx.to_debugger, vec![0x00, 0xff, 0x5a]);
        assert!(only_tx.to_guest.is_empty());

        let only_rx = KdSerialPump::tick(Vec::new(), b"\x62\x30".to_vec());
        assert!(only_rx.to_debugger.is_empty());
        assert_eq!(only_rx.to_guest, vec![0x62, 0x30]);
    }

    #[test]
    fn bridge_round_trips_bytes_through_a_real_socket() {
        let dir = std::env::temp_dir();
        // Deterministic-ish unique path without Date/rand (unavailable here):
        // process id + the listener addr are enough for a test-local socket.
        let path = dir.join(format!("bridgevm-kd-test-{}.sock", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let mut bridge = KdSerialBridge::bind(&path).expect("bind kd bridge");

        // A debugger connects and sends a byte.
        let mut client = UnixStream::connect(&path).expect("connect debugger");
        client.set_nonblocking(true).expect("nonblock client");
        client.write_all(b"\x62\x62\x62\x62").expect("send breakin");

        // Give the accept + first read a couple of ticks; the platform is not
        // needed for the socket half, so drive the pump's socket path directly.
        bridge.accept_if_needed();
        assert!(bridge.peer.is_some(), "bridge accepted the debugger");

        let mut got = Vec::new();
        for _ in 0..8 {
            let mut buf = [0u8; 64];
            match bridge.peer.as_mut().unwrap().read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    got.extend_from_slice(&buf[..n]);
                    break;
                }
                Err(ref e) if e.kind() == ErrorKind::WouldBlock => {
                    std::thread::yield_now();
                }
                Err(e) => panic!("read: {e}"),
            }
        }
        assert_eq!(
            got, b"\x62\x62\x62\x62",
            "debugger bytes reached the bridge"
        );

        // Bridge writes guest TX back to the debugger.
        bridge
            .peer
            .as_mut()
            .unwrap()
            .write_all(b"KDBG")
            .expect("write to debugger");
        let mut client_got = Vec::new();
        for _ in 0..8 {
            let mut buf = [0u8; 64];
            match client.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    client_got.extend_from_slice(&buf[..n]);
                    break;
                }
                Err(ref e) if e.kind() == ErrorKind::WouldBlock => std::thread::yield_now(),
                Err(e) => panic!("client read: {e}"),
            }
        }
        assert_eq!(client_got, b"KDBG");
    }
}
