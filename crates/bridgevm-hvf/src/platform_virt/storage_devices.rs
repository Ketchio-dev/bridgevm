//! NVMe namespaces and virtio-blk boot media: attach, export, stats, and their BAR/PIO handlers.

use super::*;
use crate::fwcfg::GuestMemoryMut;
use crate::machine;
use crate::nvme::NvmeCommandTrace;
use crate::virtio_blk::VirtioBlockRequestTrace;
use crate::virtio_blk::VirtioMmioBlock;
use crate::virtio_blk::VirtioMmioBlockResult;
use crate::virtio_blk::VirtioMmioBlockStats;
use crate::virtio_blk::VirtioPciBlock;
use crate::virtio_blk::VirtioPciBlockOp;
use std::io;
use std::path::Path;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NvmePcieLiveness {
    pub nvme_advertised: bool,
    pub nvme_ecam_touched: bool,
    pub nvme_command_memory_enabled: bool,
    pub nvme_command_bus_master_enabled: bool,
    pub nvme_bar0_assigned: bool,
    pub nvme_mmio_reached: bool,
    pub nvme_cc_enabled: bool,
    pub nvme_admin_doorbell_rung: bool,
}

impl VirtPlatform {
    /// Replace the first NVMe disk image. The image is padded to full 512-byte
    /// LBAs by the controller.
    pub fn load_nvme_disk(&mut self, data: Vec<u8>) {
        self.nvme.load_disk_image(data);
    }

    /// Attach a host raw disk file as the first NVMe namespace without reading
    /// the whole image into memory. With `write_back = false`, guest writes stay
    /// in a sparse overlay and are only visible through explicit export.
    pub fn attach_nvme_raw_file(
        &mut self,
        path: impl AsRef<Path>,
        write_back: bool,
    ) -> io::Result<()> {
        self.nvme.load_raw_file(path, write_back)
    }

    /// Whether NVMe reads and writes may use stable guest-RAM host pointers.
    /// `BRIDGEVM_NVME_BUFFERED_IO=1` disables this for process-local A/B tests.
    pub fn nvme_direct_dma_enabled(&self) -> bool {
        self.nvme.direct_dma_enabled()
    }

    /// Attach a blank NSID-2 target namespace of `disk_bytes` in-memory storage,
    /// so Windows sees a second empty disk to install onto.
    pub fn attach_nvme_second_namespace(&mut self, disk_bytes: usize) {
        self.nvme.attach_second_namespace(disk_bytes);
    }

    /// Attach a host raw file as the NSID-2 target namespace (sparse overlay when
    /// `write_back = false`, direct writes when true).
    pub fn attach_nvme_second_namespace_raw_file(
        &mut self,
        path: impl AsRef<Path>,
        write_back: bool,
    ) -> io::Result<()> {
        self.nvme.attach_second_namespace_raw_file(path, write_back)
    }

    /// Attach a read-only Windows/Linux installer ISO to the last QEMU virt
    /// virtio-mmio transport slot. QEMU's own `virtio-blk-device` oracle uses
    /// slot 31 (`0x0a003e00`) for an explicitly added MMIO block device.
    pub fn attach_virtio_iso(&mut self, path: impl AsRef<Path>) -> io::Result<()> {
        if !self.devices.legacy_virtio_mmio_present {
            return Ok(());
        }
        self.virtio_iso = Some(VirtioMmioBlock::open_read_only(path)?);
        Ok(())
    }

    pub fn attach_pci_boot_media(&mut self, path: impl AsRef<Path>) -> io::Result<()> {
        if !self.devices.virtio_boot_media_present {
            return Ok(());
        }
        self.pci_boot_media = Some(VirtioPciBlock::open_read_only(path)?);
        Ok(())
    }

    /// Current read/queue counters for the installer ISO block device, if one is
    /// attached. Live probes print this when diagnosing Windows media boot.
    pub fn virtio_iso_stats(&self) -> Option<VirtioMmioBlockStats> {
        self.virtio_iso.as_ref().map(VirtioMmioBlock::stats)
    }

    pub fn pci_boot_media_stats(&self) -> Option<VirtioMmioBlockStats> {
        self.pci_boot_media.as_ref().map(VirtioPciBlock::stats)
    }

    pub fn pci_boot_media_request_trace(&self) -> Option<Vec<VirtioBlockRequestTrace>> {
        self.pci_boot_media
            .as_ref()
            .map(VirtioPciBlock::recent_request_trace)
    }

    pub fn virtio_iso_request_trace(&self) -> Option<Vec<VirtioBlockRequestTrace>> {
        self.virtio_iso
            .as_ref()
            .map(VirtioMmioBlock::recent_request_trace)
    }

    /// Snapshot the first NVMe disk image, including guest writes processed so
    /// far. Live probes use this to persist an explicitly writable image.
    pub fn nvme_disk(&self) -> &[u8] {
        self.nvme.disk_image()
    }

    /// In-memory snapshot of the first NVMe disk, if the media is memory-backed.
    pub fn nvme_disk_if_memory(&self) -> Option<&[u8]> {
        self.nvme.disk_image_if_memory()
    }

    pub fn nvme_second_namespace_disk_if_memory(&self) -> Option<&[u8]> {
        self.nvme.second_namespace_disk_image_if_memory()
    }

    /// Export current NVMe media to a raw file, applying sparse overlay writes.
    pub fn export_nvme_disk(&mut self, path: impl AsRef<Path>) -> io::Result<u64> {
        self.nvme.export_disk_image(path)
    }

    pub fn export_nvme_second_namespace_disk(&mut self, path: impl AsRef<Path>) -> io::Result<u64> {
        self.nvme.export_second_namespace_disk_image(path)
    }

    /// Flush write-through NVMe media.
    pub fn flush_nvme_disk(&mut self) -> io::Result<()> {
        self.nvme.flush_disk()
    }

    pub fn flush_nvme_second_namespace_disk(&mut self) -> io::Result<()> {
        self.nvme.flush_second_namespace_disk()
    }

    /// Current byte length of the NVMe namespace backing media.
    pub fn nvme_disk_len(&self) -> u64 {
        self.nvme.disk_len()
    }

    pub fn nvme_second_namespace_disk_len(&self) -> Option<u64> {
        self.nvme.second_namespace_disk_len()
    }

    /// Recent NVMe commands processed by the controller, oldest first. Live
    /// probes use this to diagnose Windows setup stalls without enabling a
    /// firehose of per-command logging.
    pub fn nvme_command_trace(&self) -> Vec<NvmeCommandTrace> {
        self.nvme.recent_command_trace()
    }

    pub fn nvme_pcie_liveness(&self) -> NvmePcieLiveness {
        let state = self.pcie.nvme_endpoint_state();
        NvmePcieLiveness {
            nvme_advertised: state.advertised,
            nvme_ecam_touched: self.nvme_ecam_touched,
            nvme_command_memory_enabled: state.command_memory_enabled,
            nvme_command_bus_master_enabled: state.command_bus_master_enabled,
            nvme_bar0_assigned: state.bar0_assigned,
            nvme_mmio_reached: self.nvme_mmio_reached,
            nvme_cc_enabled: self.nvme_cc_enabled,
            nvme_admin_doorbell_rung: self.nvme_admin_doorbell_rung,
        }
    }

    pub(crate) fn pci_boot_media_msix_access(&mut self, offset: u64, op: MmioOp) -> MmioOutcome {
        let Some(dev) = self.pci_boot_media.as_mut() else {
            return MmioOutcome::KnownUnimplemented("virtio-blk-pci");
        };
        let result = match op {
            MmioOp::Read { size } => dev.msix_bar_access(offset, VirtioPciBlockOp::Read { size }),
            MmioOp::Write { size, value } => {
                dev.msix_bar_access(offset, VirtioPciBlockOp::Write { size, value })
            }
        };
        match result {
            VirtioMmioBlockResult::ReadValue(v) => MmioOutcome::ReadValue(v),
            VirtioMmioBlockResult::WriteAck => MmioOutcome::WriteAck,
        }
    }

    pub(crate) fn pci_boot_media_access(
        &mut self,
        offset: u64,
        op: MmioOp,
        mem: &mut dyn GuestMemoryMut,
    ) -> MmioOutcome {
        let Some(dev) = self.pci_boot_media.as_mut() else {
            return MmioOutcome::KnownUnimplemented("virtio-blk-pci");
        };
        let old_irq = dev.interrupt_line_level();
        let result = match op {
            MmioOp::Read { size } => dev.access(offset, VirtioPciBlockOp::Read { size }, mem),
            MmioOp::Write { size, value } => {
                dev.access(offset, VirtioPciBlockOp::Write { size, value }, mem)
            }
        };
        let new_irq = dev.interrupt_line_level();
        if old_irq != new_irq {
            self.pending_spi_levels
                .push((machine::spi_to_intid(machine::SPI_PCIE_INTA), new_irq));
        }
        match result {
            VirtioMmioBlockResult::ReadValue(v) => MmioOutcome::ReadValue(v),
            VirtioMmioBlockResult::WriteAck => MmioOutcome::WriteAck,
        }
    }

    pub(crate) fn pci_boot_media_legacy_pio_access(
        &mut self,
        offset: u64,
        op: MmioOp,
        mem: &mut dyn GuestMemoryMut,
    ) -> MmioOutcome {
        let Some(dev) = self.pci_boot_media.as_mut() else {
            return MmioOutcome::KnownUnimplemented("virtio-blk-pci");
        };
        let old_irq = dev.interrupt_line_level();
        let result = match op {
            MmioOp::Read { size } => {
                dev.legacy_io_access(offset, VirtioPciBlockOp::Read { size }, mem)
            }
            MmioOp::Write { size, value } => {
                dev.legacy_io_access(offset, VirtioPciBlockOp::Write { size, value }, mem)
            }
        };
        let new_irq = dev.interrupt_line_level();
        if old_irq != new_irq {
            self.pending_spi_levels
                .push((machine::spi_to_intid(machine::SPI_PCIE_INTA), new_irq));
        }
        match result {
            VirtioMmioBlockResult::ReadValue(v) => MmioOutcome::ReadValue(v),
            VirtioMmioBlockResult::WriteAck => MmioOutcome::WriteAck,
        }
    }

    pub(crate) fn nvme_access(
        &mut self,
        offset: u64,
        op: MmioOp,
        mem: &mut dyn GuestMemoryMut,
    ) -> MmioOutcome {
        match op {
            MmioOp::Read { size } => MmioOutcome::ReadValue(self.nvme.mmio_read(offset, size)),
            MmioOp::Write { size, value } => {
                self.nvme.mmio_write(offset, size, value);
                self.nvme_completion_scratch.clear();
                self.nvme
                    .process_into(mem, &mut self.nvme_completion_scratch);
                self.queue_nvme_completion_msix();
                self.flush_nvme_pending_msix();
                MmioOutcome::WriteAck
            }
        }
    }
}
