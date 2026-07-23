//! Split out of net_nat.rs to keep files under 850 lines.

use super::*;

use std::{
    collections::{hash_map::Entry, HashMap, VecDeque},
    io::{self, Read},
    net::{IpAddr, Ipv4Addr as StdIpv4Addr, TcpStream},
    os::fd::{FromRawFd, RawFd},
    path::Path,
    sync::OnceLock,
};

pub(crate) fn icmp_reply_rejection_reason(reply: &[u8]) -> &'static str {
    let Some(offset) = icmp_reply_payload_offset(reply) else {
        return "malformed";
    };
    let icmp = &reply[offset..];
    if icmp[0] != 0 {
        return "not_echo_reply";
    }
    if icmp[1] != 0 {
        return "nonzero_code";
    }
    "rejected"
}

pub(crate) fn trace_icmp_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("BRIDGEVM_TRACE_ICMP").is_some())
}

pub(crate) fn trace_icmp_recv(len: usize, first_byte: u8, outcome: &str) {
    eprintln!("bridgevm icmp recv bytes={len} first=0x{first_byte:02x} {outcome}");
}

pub(crate) fn first_resolv_conf_nameserver() -> Option<StdIpv4Addr> {
    first_nameserver_from_path(Path::new("/etc/resolv.conf"))
}

pub(crate) fn first_nameserver_from_path(path: &Path) -> Option<StdIpv4Addr> {
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
    for line in contents.lines() {
        let line = line.trim();
        if line.starts_with('#') || !line.starts_with("nameserver") {
            continue;
        }
        let addr = line.split_whitespace().nth(1)?;
        if let Ok(IpAddr::V4(ip)) = addr.parse::<IpAddr>() {
            return Some(ip);
        }
    }
    None
}

pub(crate) fn queue_udp_reply(
    reply_queue: &mut VecDeque<Vec<u8>>,
    guest_mac: MacAddr,
    src_ip: Ipv4Addr,
    dst_ip: Ipv4Addr,
    src_port: u16,
    dst_port: u16,
    payload: &[u8],
) {
    let Some(frame) = build_udp_reply_frame(
        guest_mac,
        GATEWAY_MAC,
        src_ip,
        dst_ip,
        src_port,
        dst_port,
        payload,
    ) else {
        return;
    };
    reply_queue.push_back(frame);
}

pub(crate) fn queue_tcp_reply(
    reply_queue: &mut VecDeque<Vec<u8>>,
    guest_mac: MacAddr,
    key: &TcpFlowKey,
    seq: u32,
    ack: u32,
    flags: u8,
    payload: &[u8],
) {
    let Some(frame) = build_tcp_reply_frame(
        EthernetIpv4Endpoint {
            mac: GATEWAY_MAC,
            network: Ipv4Endpoint::new(key.dst_ip, key.dst_port),
        },
        EthernetIpv4Endpoint {
            mac: guest_mac,
            network: Ipv4Endpoint::new(key.guest_ip, key.guest_port),
        },
        seq,
        ack,
        flags,
        payload,
    ) else {
        return;
    };
    reply_queue.push_back(frame);
}

pub(crate) fn evict_lru<K: Copy + Eq + std::hash::Hash, V: HasLastActivity>(
    map: &mut HashMap<K, V>,
    cap: usize,
) {
    while map.len() > cap {
        let Some((&key, _)) = map.iter().min_by_key(|(_, flow)| flow.last_activity()) else {
            return;
        };
        map.remove(&key);
    }
}

pub(crate) fn get_or_insert_lru<K, V, E, F>(
    map: &mut HashMap<K, V>,
    key: K,
    cap: usize,
    create: F,
) -> Result<Option<&mut V>, E>
where
    K: Copy + Eq + std::hash::Hash,
    V: HasLastActivity,
    F: FnOnce() -> Result<V, E>,
{
    if let Entry::Vacant(entry) = map.entry(key) {
        entry.insert(create()?);
        evict_lru(map, cap);
    }
    Ok(map.get_mut(&key))
}

pub(crate) trait HasLastActivity {
    fn last_activity(&self) -> u64;
}

impl HasLastActivity for UdpFlow {
    fn last_activity(&self) -> u64 {
        self.last_activity
    }
}

impl HasLastActivity for TcpFlow {
    fn last_activity(&self) -> u64 {
        self.last_activity
    }
}

impl HasLastActivity for IcmpFlow {
    fn last_activity(&self) -> u64 {
        self.last_activity
    }
}

pub(crate) fn would_block(err: &io::Error) -> bool {
    matches!(
        err.kind(),
        io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
    )
}

pub(crate) fn tcp_connect_error(stream: &TcpStream) -> io::Result<Option<i32>> {
    match stream.take_error()? {
        Some(err) => Ok(Some(err.raw_os_error().unwrap_or(1))),
        None => {
            let mut byte = [0u8; 0];
            match stream.peek(&mut byte) {
                Ok(_) => Ok(Some(0)),
                Err(err) if would_block(&err) => Ok(None),
                Err(err) if err.kind() == io::ErrorKind::NotConnected => Ok(None),
                Err(err) => Ok(Some(err.raw_os_error().unwrap_or(1))),
            }
        }
    }
}

#[cfg(target_os = "macos")]
#[derive(Debug)]
pub(crate) struct RawIcmpSocket {
    pub(crate) fd: RawFd,
}

#[cfg(target_os = "macos")]
impl RawIcmpSocket {
    pub(crate) fn new_nonblocking() -> io::Result<Self> {
        // SAFETY: socket is called with valid constants; errors are returned via errno.
        let fd = unsafe { socket(AF_INET, SOCK_DGRAM, IPPROTO_ICMP) };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        if let Err(err) = set_raw_icmp_socket_nonblocking(fd) {
            let _ = raw_close(fd);
            return Err(err);
        }
        Ok(Self { fd })
    }

    pub(crate) fn send_to(&self, message: &[u8], dst: StdIpv4Addr) -> io::Result<usize> {
        let addr = RawSockAddrIn::new(dst, 0);
        // SAFETY: message and addr point to valid memory for the duration of the call.
        let rc = unsafe {
            sendto(
                self.fd,
                message.as_ptr().cast(),
                message.len(),
                0,
                (&addr as *const RawSockAddrIn).cast::<RawSockAddr>(),
                std::mem::size_of::<RawSockAddrIn>() as RawSockLen,
            )
        };
        if rc >= 0 {
            Ok(rc as usize)
        } else {
            Err(io::Error::last_os_error())
        }
    }

    pub(crate) fn recv_from(&self, buf: &mut [u8]) -> io::Result<(usize, StdIpv4Addr)> {
        // SAFETY: zeroed sockaddr_in is a valid writable buffer for recvfrom.
        let mut addr = unsafe { std::mem::zeroed::<RawSockAddrIn>() };
        let mut len = std::mem::size_of::<RawSockAddrIn>() as RawSockLen;
        // SAFETY: buf and addr point to valid writable memory for the duration of the call.
        let rc = unsafe {
            recvfrom(
                self.fd,
                buf.as_mut_ptr().cast(),
                buf.len(),
                0,
                (&mut addr as *mut RawSockAddrIn).cast::<RawSockAddr>(),
                &mut len,
            )
        };
        if rc >= 0 {
            Ok((rc as usize, StdIpv4Addr::from(addr.sin_addr.to_ne_bytes())))
        } else {
            Err(io::Error::last_os_error())
        }
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn raw_icmp_nonblocking_flags(flags: i32) -> i32 {
    flags | O_NONBLOCK
}

#[cfg(target_os = "macos")]
pub(crate) fn set_raw_icmp_socket_nonblocking(fd: RawFd) -> io::Result<()> {
    // SAFETY: this ICMP-only fcntl binding is declared with C's variadic ABI,
    // which is required for F_SETFL on arm64 macOS.
    let flags = unsafe { fcntl_icmp(fd, F_GETFL) };
    if flags < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: fcntl operates on the socket fd created above.
    if unsafe { fcntl_icmp(fd, F_SETFL, raw_icmp_nonblocking_flags(flags)) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(target_os = "macos")]
impl Drop for RawIcmpSocket {
    fn drop(&mut self) {
        let _ = raw_close(self.fd);
    }
}

#[cfg(not(target_os = "macos"))]
#[derive(Debug)]
pub(crate) struct RawIcmpSocket;

#[cfg(not(target_os = "macos"))]
impl RawIcmpSocket {
    pub(crate) fn new_nonblocking() -> io::Result<Self> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "unprivileged ICMP datagram sockets are only enabled on macOS",
        ))
    }

    pub(crate) fn send_to(&self, _message: &[u8], _dst: StdIpv4Addr) -> io::Result<usize> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "icmp unsupported",
        ))
    }

    pub(crate) fn recv_from(&self, _buf: &mut [u8]) -> io::Result<(usize, StdIpv4Addr)> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "icmp unsupported",
        ))
    }
}

#[cfg(unix)]
pub(crate) fn nonblocking_tcp_connect(dst: StdIpv4Addr, port: u16) -> io::Result<TcpStream> {
    let fd = raw_nonblocking_tcp_socket()?;
    let connect_result = raw_connect_ipv4(fd, dst, port);
    if let Err(err) = connect_result {
        if !raw_connect_in_progress(&err) {
            let _ = raw_close(fd);
            return Err(err);
        }
    }
    // SAFETY: fd is a valid TCP socket created above and ownership is moved into TcpStream.
    let stream = unsafe { TcpStream::from_raw_fd(fd) };
    stream.set_nonblocking(true)?;
    Ok(stream)
}

#[cfg(not(unix))]
pub(crate) fn nonblocking_tcp_connect(dst: StdIpv4Addr, port: u16) -> io::Result<TcpStream> {
    let stream = TcpStream::connect(SocketAddrV4::new(dst, port))?;
    stream.set_nonblocking(true)?;
    Ok(stream)
}

#[cfg(unix)]
pub(crate) fn raw_nonblocking_tcp_socket() -> io::Result<RawFd> {
    // SAFETY: socket is called with valid constants; errors are returned via errno.
    let fd = unsafe { socket(AF_INET, SOCK_STREAM, 0) };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: fcntl operates on the socket fd created above.
    let flags = unsafe { fcntl(fd, F_GETFL, 0) };
    if flags < 0 || unsafe { fcntl(fd, F_SETFL, flags | O_NONBLOCK) } < 0 {
        let err = io::Error::last_os_error();
        let _ = raw_close(fd);
        return Err(err);
    }
    Ok(fd)
}

#[cfg(unix)]
pub(crate) fn raw_connect_ipv4(fd: RawFd, dst: StdIpv4Addr, port: u16) -> io::Result<()> {
    let addr = RawSockAddrIn::new(dst, port);
    // SAFETY: addr points to a properly initialized IPv4 sockaddr for this platform.
    let rc = unsafe {
        connect(
            fd,
            (&addr as *const RawSockAddrIn).cast::<RawSockAddr>(),
            std::mem::size_of::<RawSockAddrIn>() as RawSockLen,
        )
    };
    if rc == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(unix)]
pub(crate) fn raw_connect_in_progress(err: &io::Error) -> bool {
    matches!(
        err.raw_os_error(),
        Some(ERRNO_EINPROGRESS) | Some(ERRNO_EALREADY) | Some(ERRNO_EWOULDBLOCK)
    )
}

#[cfg(unix)]
pub(crate) fn raw_close(fd: RawFd) -> io::Result<()> {
    // SAFETY: close is called with a raw fd; errors are surfaced through errno.
    let rc = unsafe { close(fd) };
    if rc == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(target_os = "macos")]
pub(crate) type RawSockLen = u32;
#[cfg(target_os = "macos")]
#[repr(C)]
pub(crate) struct RawSockAddr {
    pub(crate) sa_len: u8,
    pub(crate) sa_family: u8,
    pub(crate) sa_data: [u8; 14],
}
#[cfg(target_os = "macos")]
#[repr(C)]
pub(crate) struct RawSockAddrIn {
    pub(crate) sin_len: u8,
    pub(crate) sin_family: u8,
    pub(crate) sin_port: u16,
    pub(crate) sin_addr: u32,
    pub(crate) sin_zero: [u8; 8],
}
#[cfg(target_os = "macos")]
impl RawSockAddrIn {
    pub(crate) fn new(dst: StdIpv4Addr, port: u16) -> Self {
        Self {
            sin_len: std::mem::size_of::<Self>() as u8,
            sin_family: AF_INET as u8,
            sin_port: port.to_be(),
            sin_addr: u32::from_ne_bytes(dst.octets()),
            sin_zero: [0; 8],
        }
    }
}

#[cfg(target_os = "linux")]
pub(crate) type RawSockLen = u32;
#[cfg(target_os = "linux")]
#[repr(C)]
pub(crate) struct RawSockAddr {
    pub(crate) sa_family: u16,
    pub(crate) sa_data: [u8; 14],
}
#[cfg(target_os = "linux")]
#[repr(C)]
pub(crate) struct RawSockAddrIn {
    pub(crate) sin_family: u16,
    pub(crate) sin_port: u16,
    pub(crate) sin_addr: u32,
    pub(crate) sin_zero: [u8; 8],
}
#[cfg(target_os = "linux")]
impl RawSockAddrIn {
    pub(crate) fn new(dst: StdIpv4Addr, port: u16) -> Self {
        Self {
            sin_family: AF_INET as u16,
            sin_port: port.to_be(),
            sin_addr: u32::from_ne_bytes(dst.octets()),
            sin_zero: [0; 8],
        }
    }
}

#[cfg(target_os = "macos")]
pub(crate) const AF_INET: i32 = 2;
#[cfg(target_os = "linux")]
pub(crate) const AF_INET: i32 = 2;
#[cfg(unix)]
pub(crate) const SOCK_STREAM: i32 = 1;
#[cfg(target_os = "macos")]
pub(crate) const SOCK_DGRAM: i32 = 2;
#[cfg(target_os = "macos")]
pub(crate) const IPPROTO_ICMP: i32 = 1;
#[cfg(unix)]
pub(crate) const F_GETFL: i32 = 3;
#[cfg(unix)]
pub(crate) const F_SETFL: i32 = 4;
#[cfg(target_os = "macos")]
pub(crate) const O_NONBLOCK: i32 = 0x0004;
#[cfg(target_os = "linux")]
pub(crate) const O_NONBLOCK: i32 = 0o4000;
#[cfg(target_os = "macos")]
pub(crate) const ERRNO_EINPROGRESS: i32 = 36;
#[cfg(target_os = "linux")]
pub(crate) const ERRNO_EINPROGRESS: i32 = 115;
#[cfg(target_os = "macos")]
pub(crate) const ERRNO_EALREADY: i32 = 37;
#[cfg(target_os = "linux")]
pub(crate) const ERRNO_EALREADY: i32 = 114;
#[cfg(target_os = "macos")]
pub(crate) const ERRNO_EWOULDBLOCK: i32 = 35;
#[cfg(target_os = "linux")]
pub(crate) const ERRNO_EWOULDBLOCK: i32 = 11;

#[cfg(unix)]
unsafe extern "C" {
    fn socket(domain: i32, ty: i32, protocol: i32) -> RawFd;
    fn connect(fd: RawFd, addr: *const RawSockAddr, len: RawSockLen) -> i32;
    fn fcntl(fd: RawFd, cmd: i32, arg: i32) -> i32;
    #[cfg(target_os = "macos")]
    #[allow(clashing_extern_declarations)]
    #[link_name = "fcntl"]
    fn fcntl_icmp(fd: RawFd, cmd: i32, ...) -> i32;
    fn close(fd: RawFd) -> i32;
    #[cfg(target_os = "macos")]
    fn sendto(
        fd: RawFd,
        buf: *const std::ffi::c_void,
        len: usize,
        flags: i32,
        addr: *const RawSockAddr,
        addr_len: RawSockLen,
    ) -> isize;
    #[cfg(target_os = "macos")]
    fn recvfrom(
        fd: RawFd,
        buf: *mut std::ffi::c_void,
        len: usize,
        flags: i32,
        addr: *mut RawSockAddr,
        addr_len: *mut RawSockLen,
    ) -> isize;
}

pub(crate) fn read_u16_be(bytes: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_be_bytes(read_array(bytes, offset)?))
}

pub(crate) fn read_array<const N: usize>(bytes: &[u8], offset: usize) -> Option<[u8; N]> {
    let end = offset.checked_add(N)?;
    bytes.get(offset..end)?.try_into().ok()
}
