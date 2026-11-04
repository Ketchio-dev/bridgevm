//! Where guest queries for 10.0.2.3 go: the host's nameserver from resolv.conf.
//!
//! macOS rewrites /etc/resolv.conf when the network changes (Wi-Fi switch, VPN
//! toggle, wake on another network), so the file is checked again at most once
//! a second while queries arrive, and parsed again only when its inode, size or
//! mtime changed. A UDP flow is keyed by the server it was opened to, so new
//! queries go to the new server and old flows simply idle out.

use std::{
    ffi::CString,
    io::Read,
    net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV6},
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    time::SystemTime,
};

pub(crate) const MAX_RESOLV_CONF_BYTES: u64 = 64 * 1024;
/// Only when resolv.conf names no usable nameserver at all (or is missing).
pub(crate) const FALLBACK_DNS_SERVER: SocketAddr =
    SocketAddr::new(IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1)), 53);
pub(crate) const RESOLV_CONF_RECHECK_MS: u64 = 1_000;

type FileStamp = (u64, u64, Option<SystemTime>);

#[derive(Debug)]
pub(crate) struct DnsUpstream {
    /// None keeps `server` fixed.
    resolv_conf: Option<PathBuf>,
    stamp: Option<FileStamp>,
    pub(crate) next_check_ms: u64,
    pub(crate) server: SocketAddr,
    /// Server switches since the first read.
    pub(crate) changes: u64,
}

impl DnsUpstream {
    pub(crate) fn fixed(server: IpAddr) -> Self {
        Self {
            resolv_conf: None,
            stamp: None,
            next_check_ms: u64::MAX,
            server: SocketAddr::new(server, 53),
            changes: 0,
        }
    }

    pub(crate) fn from_resolv_conf(path: PathBuf) -> Self {
        Self {
            stamp: file_stamp(&path),
            server: nameserver_from_path(&path).unwrap_or(FALLBACK_DNS_SERVER),
            resolv_conf: Some(path),
            next_check_ms: RESOLV_CONF_RECHECK_MS,
            changes: 0,
        }
    }

    /// The server for a query sent at `now_ms` on the handler's clock.
    pub(crate) fn server_at(&mut self, now_ms: u64) -> SocketAddr {
        if now_ms < self.next_check_ms {
            return self.server;
        }
        self.next_check_ms = now_ms.saturating_add(RESOLV_CONF_RECHECK_MS);
        if let Some(path) = &self.resolv_conf {
            let stamp = file_stamp(path);
            if stamp != self.stamp {
                self.stamp = stamp;
                let server = nameserver_from_path(path).unwrap_or(FALLBACK_DNS_SERVER);
                if server != self.server {
                    self.server = server;
                    self.changes = self.changes.saturating_add(1);
                }
            }
        }
        self.server
    }
}

fn file_stamp(path: &Path) -> Option<FileStamp> {
    let meta = std::fs::metadata(path).ok()?;
    Some((meta.ino(), meta.len(), meta.modified().ok()))
}

/// The first IPv4 nameserver, else the first IPv6 one. The file is host
/// input, so it is read with a size cap.
pub(crate) fn nameserver_from_path(path: &Path) -> Option<SocketAddr> {
    let file = std::fs::File::open(path).ok()?;
    if file.metadata().ok()?.len() > MAX_RESOLV_CONF_BYTES {
        return None;
    }
    let mut bytes = Vec::new();
    file.take(MAX_RESOLV_CONF_BYTES + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() as u64 > MAX_RESOLV_CONF_BYTES {
        return None;
    }
    let contents = String::from_utf8(bytes).ok()?;
    let mut first_ipv6 = None;
    for line in contents.lines() {
        let mut words = line.split_whitespace();
        if words.next() != Some("nameserver") {
            continue;
        }
        match words.next().and_then(parse_nameserver) {
            Some(server @ SocketAddr::V4(_)) => return Some(server),
            Some(server) => {
                first_ipv6.get_or_insert(server);
            }
            None => {}
        }
    }
    first_ipv6
}

/// `addr` or, for IPv6, `addr%zone`: macOS lists a link-local nameserver
/// with its interface (`fe80::1%en0`), which the socket needs as scope id.
pub(crate) fn parse_nameserver(word: &str) -> Option<SocketAddr> {
    let (addr, zone) = match word.split_once('%') {
        Some((addr, zone)) => (addr, Some(zone)),
        None => (word, None),
    };
    match addr.parse::<IpAddr>().ok()? {
        IpAddr::V4(ip) => Some(SocketAddr::new(ip.into(), 53)),
        IpAddr::V6(ip) => {
            let scope_id = match zone {
                None => 0,
                Some(zone) => zone.parse().ok().or_else(|| interface_index(zone))?,
            };
            Some(SocketAddrV6::new(ip, 53, 0, scope_id).into())
        }
    }
}

fn interface_index(name: &str) -> Option<u32> {
    let name = CString::new(name).ok()?;
    // SAFETY: `name` is a valid NUL-terminated string for the whole call.
    let index = unsafe { libc::if_nametoindex(name.as_ptr()) };
    (index != 0).then_some(index)
}
