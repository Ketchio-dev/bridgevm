//! Why the NAT reset a guest TCP connection. Each reason has a `NatStats`
//! counter, and the first resets of a VM run are logged with the host error
//! and the ports (no addresses or payload), so a reset the guest reports can
//! be traced from the normal log.

use super::*;

use std::{fmt::Display, io, net::TcpStream};

/// Resets logged per VM run; later ones are only counted.
pub(crate) const LOGGED_TCP_RESETS: u64 = 8;

/// The host socket error behind a reset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HostError {
    pub(crate) kind: io::ErrorKind,
    pub(crate) errno: Option<i32>,
}

impl HostError {
    pub(crate) fn os(errno: i32) -> Self {
        Self::from(&io::Error::from_raw_os_error(errno))
    }
}

impl From<&io::Error> for HostError {
    fn from(error: &io::Error) -> Self {
        Self {
            kind: error.kind(),
            errno: error.raw_os_error(),
        }
    }
}

/// Why the NAT sent the guest a RST.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TcpResetReason {
    /// The host connect for the guest's SYN failed, at once or later.
    Connect,
    /// Reading the host socket of an established flow failed.
    HostRead,
    /// The NAT dropped the flow: idle past its limit, or the oldest when the
    /// flow table was full.
    Evicted,
}

fn or_dash(value: Option<impl Display>) -> String {
    match value {
        Some(value) => value.to_string(),
        None => "-".to_owned(),
    }
}

/// Counts a reset sent to the guest and logs the first `LOGGED_TCP_RESETS`
/// of the VM run. `host_port` is the NAT's own port on the host, the client
/// port a host server sees.
pub(crate) fn note_tcp_reset(
    stats: &mut NatStats,
    reason: TcpResetReason,
    key: &TcpFlowKey,
    stream: Option<&TcpStream>,
    error: Option<HostError>,
) {
    let earlier = stats
        .rst_connect
        .saturating_add(stats.rst_hostread)
        .saturating_add(stats.rst_evicted);
    let (name, count) = match reason {
        TcpResetReason::Connect => ("connect", &mut stats.rst_connect),
        TcpResetReason::HostRead => ("hostread", &mut stats.rst_hostread),
        TcpResetReason::Evicted => ("evicted", &mut stats.rst_evicted),
    };
    *count = count.saturating_add(1);
    if earlier >= LOGGED_TCP_RESETS {
        return;
    }
    let kind = or_dash(error.map(|e| format!("{:?}", e.kind)));
    let errno = or_dash(error.and_then(|e| e.errno));
    let host_port = or_dash(stream.and_then(|s| s.local_addr().ok()).map(|a| a.port()));
    log_tcp_reset(format!(
        "virtio-net NAT reset {}/{LOGGED_TCP_RESETS}: reason={name} kind={kind} errno={errno} \
         guest_port={} dst_port={} host_port={host_port}",
        earlier + 1,
        key.guest_port,
        key.dst_port,
    ));
}

fn log_tcp_reset(line: String) {
    eprintln!("{line}");
    #[cfg(test)]
    TEST_LOG.with_borrow_mut(|log| log.push(line));
}

#[cfg(test)]
thread_local! {
    static TEST_LOG: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}
#[cfg(test)]
use std::cell::RefCell;

/// The reset lines this thread logged since the last call.
#[cfg(test)]
pub(crate) fn take_tcp_reset_log() -> Vec<String> {
    TEST_LOG.take()
}
