//! Aggregation and draining of device MSI-X messages and legacy SPI level changes.

use super::*;
use crate::msix::MsixMessage;

impl VirtPlatform {
    /// Drain MSI-X messages raised by PCIe devices since the last call. The live
    /// HVF run loop turns these into `hv_gic_send_msi` calls after configuring
    /// Apple `hv_gic`'s MSI frame.
    pub fn take_pending_msix(&mut self) -> Vec<MsixMessage> {
        if self.pending_msix.is_empty() {
            Vec::new()
        } else {
            self.pending_msix.drain(..).collect()
        }
    }

    /// Drain pending MSI-X messages into caller-owned storage.
    pub fn drain_pending_msix_into(&mut self, out: &mut Vec<MsixMessage>) {
        out.append(&mut self.pending_msix);
    }

    /// Drain level changes for legacy SPI-backed devices such as virtio-mmio.
    /// The live HVF loop turns these into `hv_gic_set_spi(intid, level)`.
    pub fn take_pending_spi_levels(&mut self) -> Vec<(u32, bool)> {
        if self.pending_spi_levels.is_empty() {
            Vec::new()
        } else {
            self.pending_spi_levels.drain(..).collect()
        }
    }

    /// Drain pending SPI level changes into caller-owned storage.
    pub fn drain_pending_spi_levels_into(&mut self, out: &mut Vec<(u32, bool)>) {
        out.append(&mut self.pending_spi_levels);
    }

    pub(crate) fn queue_nvme_completion_msix(&mut self) {
        let control = self.pcie.nvme_msix_control();
        for completion in &self.nvme_completion_scratch {
            if let Some(message) =
                self.nvme
                    .raise_msix(completion.vector, control.enabled, control.function_masked)
            {
                self.pending_msix.push(message);
            }
        }
        self.nvme_completion_scratch.clear();
    }

    pub(crate) fn flush_nvme_pending_msix(&mut self) {
        let control = self.pcie.nvme_msix_control();
        self.nvme.drain_pending_msix_into(
            control.enabled,
            control.function_masked,
            &mut self.pending_msix,
        );
    }

    pub(crate) fn queue_xhci_completion_msix(&mut self) {
        if !self.devices.xhci_present {
            return;
        }
        let control = self.pcie.xhci_msix_control();
        self.xhci.raise_pending_interrupter_msix_into(
            control.enabled,
            control.function_masked,
            &mut self.pending_msix,
        );
    }

    pub(crate) fn flush_xhci_pending_msix(&mut self) {
        if !self.devices.xhci_present {
            return;
        }
        let control = self.pcie.xhci_msix_control();
        self.xhci.drain_pending_msix_into(
            control.enabled,
            control.function_masked,
            &mut self.pending_msix,
        );
    }

    pub(crate) fn flush_virtio_net_pending_msix(&mut self) {
        if !self.devices.virtio_net_present {
            return;
        }
        let Some(dev) = self.virtio_net.as_mut() else {
            return;
        };
        let control = self.pcie.virtio_net_msix_control();
        dev.drain_pending_msix_into(
            control.enabled,
            control.function_masked,
            &mut self.pending_msix,
        );
    }

    pub(crate) fn flush_hda_pending_msi(&mut self) {
        if !self.devices.hda_present {
            return;
        }
        let config = self.pcie.hda_msi_config();
        let Some(hda) = self.hda.as_mut() else {
            return;
        };
        hda.drain_pending_msi_into(
            config.enabled,
            config.address,
            config.data,
            &mut self.pending_msix,
        );
    }

    pub(crate) fn flush_virtio_gpu_pending_msix(&mut self) {
        if !self.devices.virtio_gpu_present {
            return;
        }
        let Some(dev) = self.virtio_gpu.as_mut() else {
            return;
        };
        let control = self.pcie.virtio_gpu_msix_control();
        dev.drain_pending_msix_into(
            control.enabled,
            control.function_masked,
            &mut self.pending_msix,
        );
    }

    pub(crate) fn flush_virtio_console_pending_msix(&mut self) {
        if !self.devices.virtio_console_present {
            return;
        }
        let Some(dev) = self.virtio_console.as_mut() else {
            return;
        };
        let control = self.pcie.virtio_console_msix_control();
        dev.drain_pending_msix_into(
            control.enabled,
            control.function_masked,
            &mut self.pending_msix,
        );
    }
}
