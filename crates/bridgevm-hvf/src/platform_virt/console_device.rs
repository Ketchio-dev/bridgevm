//! virtio-console host-agent plumbing and its BAR handlers.

use super::*;
use crate::fwcfg::GuestMemoryMut;
use crate::virtio_console::VirtioConsoleResult;
use crate::virtio_console::VirtioConsoleStats;
use crate::virtio_console::VirtioPciConsole;
use crate::virtio_console::VirtioPciConsoleOp;

impl VirtPlatform {
    pub fn virtio_console_stats(&self) -> Option<VirtioConsoleStats> {
        self.virtio_console.as_ref().map(VirtioPciConsole::stats)
    }

    pub fn virtio_console_agent_send(&mut self, data: &[u8], mem: &mut dyn GuestMemoryMut) {
        let Some(dev) = self.virtio_console.as_mut() else {
            return;
        };
        dev.agent_send(data);
        dev.poll(mem);
        self.flush_virtio_console_pending_msix();
    }

    pub fn virtio_console_agent_take_inbound(&mut self) -> Vec<u8> {
        self.virtio_console
            .as_mut()
            .map(VirtioPciConsole::take_inbound)
            .unwrap_or_default()
    }

    pub fn virtio_console_agent_drain_inbound_into(&mut self, out: &mut Vec<u8>) {
        let Some(dev) = self.virtio_console.as_mut() else {
            return;
        };
        dev.drain_inbound_into(out);
    }

    pub fn poll_virtio_console(&mut self, mem: &mut dyn GuestMemoryMut) -> bool {
        let Some(dev) = self.virtio_console.as_mut() else {
            return false;
        };
        let progressed = dev.poll(mem);
        if progressed {
            self.flush_virtio_console_pending_msix();
        }
        progressed
    }

    pub(crate) fn virtio_console_msix_access(&mut self, offset: u64, op: MmioOp) -> MmioOutcome {
        let Some(dev) = self.virtio_console.as_mut() else {
            return MmioOutcome::KnownUnimplemented("virtio-console-pci");
        };
        let is_write = matches!(op, MmioOp::Write { .. });
        let result = match op {
            MmioOp::Read { size } => dev.msix_bar_access(offset, VirtioPciConsoleOp::Read { size }),
            MmioOp::Write { size, value } => {
                dev.msix_bar_access(offset, VirtioPciConsoleOp::Write { size, value })
            }
        };
        if is_write {
            self.flush_virtio_console_pending_msix();
        }
        match result {
            VirtioConsoleResult::ReadValue(v) => MmioOutcome::ReadValue(v),
            VirtioConsoleResult::WriteAck => MmioOutcome::WriteAck,
        }
    }

    pub(crate) fn virtio_console_access(
        &mut self,
        offset: u64,
        op: MmioOp,
        mem: &mut dyn GuestMemoryMut,
    ) -> MmioOutcome {
        let Some(dev) = self.virtio_console.as_mut() else {
            return MmioOutcome::KnownUnimplemented("virtio-console-pci");
        };
        let result = match op {
            MmioOp::Read { size } => dev.access(offset, VirtioPciConsoleOp::Read { size }, mem),
            MmioOp::Write { size, value } => {
                dev.access(offset, VirtioPciConsoleOp::Write { size, value }, mem)
            }
        };
        dev.poll(mem);
        self.flush_virtio_console_pending_msix();
        match result {
            VirtioConsoleResult::ReadValue(v) => MmioOutcome::ReadValue(v),
            VirtioConsoleResult::WriteAck => MmioOutcome::WriteAck,
        }
    }
}
