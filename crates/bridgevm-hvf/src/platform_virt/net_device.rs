//! virtio-net runtime: RX pumping, stats, and its MSI-X and modern BAR handlers.

use super::*;
use crate::fwcfg::GuestMemoryMut;
use crate::net_nat::NatStats;
use crate::virtio_net::VirtioNetResult;
use crate::virtio_net::VirtioNetStats;
use crate::virtio_net::VirtioPciNet;
use crate::virtio_net::VirtioPciNetOp;

impl VirtPlatform {
    pub fn virtio_net_stats(&self) -> Option<VirtioNetStats> {
        self.virtio_net.as_ref().map(VirtioPciNet::stats)
    }

    pub fn virtio_net_nat_stats(&self) -> Option<NatStats> {
        self.virtio_net
            .as_ref()
            .and_then(|dev| dev.backend().nat_stats())
    }

    pub fn pump_virtio_net_receive(&mut self, mem: &mut dyn GuestMemoryMut) -> bool {
        self.poll_virtio_net(mem)
    }

    pub fn poll_virtio_net(&mut self, mem: &mut dyn GuestMemoryMut) -> bool {
        let Some(dev) = self.virtio_net.as_mut() else {
            return false;
        };
        dev.poll_host_sockets();
        // Every poll may enqueue an unbounded batch of frames from drained
        // host sockets, so delivering a single frame per poll lets the shared
        // reply queue grow without bound under bulk host->guest traffic and
        // starves newer connections behind it (live guest fetches collapsed
        // below curl's 1000 B/s abort threshold). Drain a bounded burst; the
        // loop also stops as soon as the guest has no free RX descriptor.
        const RX_BURST_FRAMES: usize = 256;
        let mut delivered = false;
        for _ in 0..RX_BURST_FRAMES {
            if !dev.pump_receive(mem) {
                break;
            }
            delivered = true;
        }
        if delivered {
            self.flush_virtio_net_pending_msix();
        }
        delivered
    }

    pub(crate) fn virtio_net_msix_access(&mut self, offset: u64, op: MmioOp) -> MmioOutcome {
        let Some(dev) = self.virtio_net.as_mut() else {
            return MmioOutcome::KnownUnimplemented("virtio-net-pci");
        };
        let is_write = matches!(op, MmioOp::Write { .. });
        let result = match op {
            MmioOp::Read { size } => dev.msix_bar_access(offset, VirtioPciNetOp::Read { size }),
            MmioOp::Write { size, value } => {
                dev.msix_bar_access(offset, VirtioPciNetOp::Write { size, value })
            }
        };
        if is_write {
            self.flush_virtio_net_pending_msix();
        }
        match result {
            VirtioNetResult::ReadValue(v) => MmioOutcome::ReadValue(v),
            VirtioNetResult::WriteAck => MmioOutcome::WriteAck,
        }
    }

    pub(crate) fn virtio_net_access(
        &mut self,
        offset: u64,
        op: MmioOp,
        mem: &mut dyn GuestMemoryMut,
    ) -> MmioOutcome {
        let Some(dev) = self.virtio_net.as_mut() else {
            return MmioOutcome::KnownUnimplemented("virtio-net-pci");
        };
        let result = match op {
            MmioOp::Read { size } => dev.access(offset, VirtioPciNetOp::Read { size }, mem),
            MmioOp::Write { size, value } => {
                dev.access(offset, VirtioPciNetOp::Write { size, value }, mem)
            }
        };
        self.flush_virtio_net_pending_msix();
        match result {
            VirtioNetResult::ReadValue(v) => MmioOutcome::ReadValue(v),
            VirtioNetResult::WriteAck => MmioOutcome::WriteAck,
        }
    }
}
