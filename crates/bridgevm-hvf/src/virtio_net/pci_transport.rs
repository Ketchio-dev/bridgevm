//! VirtioPciNet BAR region decode and forwarding to the core device.

use super::host_poll_pacing::HostPollPacer;
use super::register_decode::{common_cfg_offset, device_cfg_offset, notify_queue_index};
use super::*;
use crate::fwcfg::GuestMemoryMut;
use crate::msix::MsixTable;
use crate::pcie::VIRTIO_NET_MSIX_VECTOR_COUNT;
use std::time::Instant;

#[derive(Debug)]
pub struct VirtioPciNet<B: NetBackend = LoopbackTestBackend> {
    pub(crate) net: VirtioNet<B>,
    pub(crate) msix: MsixTable,
    pub(crate) host_poll_pacer: HostPollPacer,
}

impl VirtioPciNet<LoopbackTestBackend> {
    pub fn new_loopback() -> Self {
        Self::new(LoopbackTestBackend::default())
    }
}

impl<B: NetBackend> VirtioPciNet<B> {
    pub fn new(backend: B) -> Self {
        Self {
            net: VirtioNet::new(backend),
            msix: MsixTable::new(VIRTIO_NET_MSIX_VECTOR_COUNT),
            host_poll_pacer: HostPollPacer::default(),
        }
    }

    pub fn backend(&self) -> &B {
        self.net.backend()
    }

    pub fn backend_mut(&mut self) -> &mut B {
        self.net.backend_mut()
    }

    pub fn stats(&self) -> VirtioNetStats {
        self.net.stats()
    }

    pub fn interrupt_line_level(&self) -> bool {
        self.net.interrupt_line_level()
    }

    pub fn reset_runtime_state(&mut self) {
        self.net.reset_runtime_state();
        self.msix = MsixTable::new(VIRTIO_NET_MSIX_VECTOR_COUNT);
        self.host_poll_pacer.reset();
    }

    pub fn pump_receive(&mut self, mem: &mut dyn GuestMemoryMut) -> bool {
        self.net.pump_receive(mem)
    }

    pub fn poll_host_sockets(&mut self, now: Option<Instant>) -> bool {
        let poll = self.host_poll_pacer.should_poll(now);
        if poll {
            self.net.backend_mut().poll_host_sockets();
        }
        poll
    }

    pub fn access(
        &mut self,
        offset: u64,
        op: VirtioPciNetOp,
        mem: &mut dyn GuestMemoryMut,
    ) -> VirtioNetResult {
        let is_write = matches!(op, VirtioPciNetOp::Write { .. });
        if let Some(common_offset) = common_cfg_offset(offset) {
            return match op {
                VirtioPciNetOp::Read { size } => {
                    self.net.access_common(common_offset, false, size, 0, mem)
                }
                VirtioPciNetOp::Write { size, value } => {
                    self.net
                        .access_common(common_offset, true, size, value, mem)
                }
            };
        }
        if let Some(device_offset) = device_cfg_offset(offset) {
            return match op {
                VirtioPciNetOp::Read { size } => {
                    VirtioNetResult::ReadValue(self.net.config_read(device_offset, size))
                }
                VirtioPciNetOp::Write { .. } => VirtioNetResult::WriteAck,
            };
        }
        if let Some(queue_index) = notify_queue_index(offset) {
            return match op {
                VirtioPciNetOp::Read { .. } => VirtioNetResult::ReadValue(0),
                VirtioPciNetOp::Write { value, .. } => {
                    let queue = if offset == PCI_NOTIFY_CFG_OFFSET {
                        value as u16
                    } else {
                        queue_index
                    };
                    self.net.notify_queue(queue, mem);
                    VirtioNetResult::WriteAck
                }
            };
        }
        if offset == PCI_ISR_CFG_OFFSET {
            return match op {
                VirtioPciNetOp::Read { size } => VirtioNetResult::ReadValue(mask_to_size(
                    u64::from(self.net.interrupt_status),
                    size,
                )),
                VirtioPciNetOp::Write { value, .. } => {
                    self.net.interrupt_status &= !(value as u32);
                    VirtioNetResult::WriteAck
                }
            };
        }
        match (op, is_write) {
            (VirtioPciNetOp::Read { .. }, _) => VirtioNetResult::ReadValue(0),
            (VirtioPciNetOp::Write { .. }, _) => VirtioNetResult::WriteAck,
        }
    }
}
