//! The NAT/loopback multiplexer that backs virtio-net.

use super::*;
use crate::net_nat::HostSocketOutboundIpv4Handler;
use crate::net_nat::NatBackend;
use crate::net_nat::NatStats;
use crate::virtio_net::LoopbackTestBackend;
use crate::virtio_net::NetBackend;

#[derive(Debug)]
pub(crate) enum PlatformNetBackend {
    Nat(Box<NatBackend<HostSocketOutboundIpv4Handler>>),
    Loopback(LoopbackTestBackend),
}

pub(crate) fn make_virtio_net_backend(kind: VirtioNetBackendKind) -> PlatformNetBackend {
    PlatformNetBackend::new(kind)
}

impl PlatformNetBackend {
    pub(crate) fn new(kind: VirtioNetBackendKind) -> Self {
        match kind {
            VirtioNetBackendKind::Nat => Self::Nat(Box::new(NatBackend::new_host_socket())),
            VirtioNetBackendKind::Loopback => Self::Loopback(LoopbackTestBackend::default()),
        }
    }

    pub(crate) fn nat_stats(&self) -> Option<NatStats> {
        match self {
            Self::Nat(backend) => Some(backend.stats()),
            Self::Loopback(_) => None,
        }
    }
}

impl NetBackend for PlatformNetBackend {
    fn transmit(&mut self, frame: &[u8]) {
        match self {
            Self::Nat(backend) => backend.transmit(frame),
            Self::Loopback(backend) => backend.transmit(frame),
        }
    }

    fn poll_receive(&mut self) -> Option<Vec<u8>> {
        match self {
            Self::Nat(backend) => backend.poll_receive(),
            Self::Loopback(backend) => backend.poll_receive(),
        }
    }

    fn poll_receive_into(&mut self, out: &mut Vec<u8>) -> bool {
        match self {
            Self::Nat(backend) => backend.poll_receive_into(out),
            Self::Loopback(backend) => backend.poll_receive_into(out),
        }
    }

    fn poll_host_sockets(&mut self) {
        if let Self::Nat(backend) = self {
            backend.poll_host_sockets();
        }
    }

    #[cfg(test)]
    fn test_transmitted_frames(&self) -> Option<&[Vec<u8>]> {
        match self {
            Self::Nat(_) => None,
            Self::Loopback(backend) => backend.test_transmitted_frames(),
        }
    }
}
