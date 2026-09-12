use super::icmp_reply_rejection_reason::would_block;
use std::{io, net::TcpStream};

pub(crate) fn tcp_connect_error(stream: &TcpStream) -> io::Result<Option<i32>> {
    match stream.take_error()? {
        Some(err) => Ok(Some(err.raw_os_error().unwrap_or(1))),
        None => {
            // A zero-length peek can succeed before the connection completes.
            // An established peer, not receipt of application data, proves readiness.
            match stream.peer_addr() {
                Ok(_) => Ok(Some(0)),
                Err(err) if would_block(&err) => Ok(None),
                Err(err) if err.kind() == io::ErrorKind::NotConnected => Ok(None),
                Err(err) => Ok(Some(err.raw_os_error().unwrap_or(1))),
            }
        }
    }
}
