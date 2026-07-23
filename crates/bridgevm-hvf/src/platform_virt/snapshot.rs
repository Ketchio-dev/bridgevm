//! VirtPlatform checkpoint serialize and restore.

use super::*;
use crate::msix::MsixMessage;

impl VirtPlatform {
    pub fn snapshot_state(&self) -> Vec<u8> {
        let mut out = crate::checkpoint::StateWriter::new();
        out.write_u32(1);

        out.write_blob(&self.pcie.snapshot_state());
        out.write_blob(&self.nvme.snapshot_state());
        out.write_blob(&self.flash_vars.snapshot_state());

        out.write_bool(self.virtio_iso.is_some());
        if let Some(device) = &self.virtio_iso {
            out.write_blob(&device.snapshot_state());
        }

        out.write_bool(self.pci_boot_media.is_some());
        if let Some(device) = &self.pci_boot_media {
            out.write_blob(&device.snapshot_state());
        }

        out.write_bool(self.virtio_net.is_some());
        if let Some(device) = &self.virtio_net {
            out.write_blob(&device.snapshot_state());
        }

        out.write_bool(self.virtio_gpu.is_some());
        if let Some(device) = &self.virtio_gpu {
            out.write_blob(&device.snapshot_state());
        }

        out.write_bool(self.virtio_console.is_some());
        if let Some(device) = &self.virtio_console {
            out.write_blob(&device.snapshot_state());
        }

        out.write_u32(self.pending_msix.len() as u32);
        for message in &self.pending_msix {
            out.write_u16(message.vector);
            out.write_u16(0);
            out.write_u32(message.data);
            out.write_u64(message.address);
        }

        out.write_u32(self.pending_spi_levels.len() as u32);
        for &(intid, level) in &self.pending_spi_levels {
            out.write_u32(intid);
            out.write_bool(level);
            out.write_u8(0);
            out.write_u16(0);
        }

        out.write_bool(self.nvme_ecam_touched);
        out.write_bool(self.nvme_mmio_reached);
        out.write_bool(self.nvme_cc_enabled);
        out.write_bool(self.nvme_admin_doorbell_rung);
        out.into_inner()
    }

    pub fn restore_state(&mut self, data: &[u8]) {
        let mut input = crate::checkpoint::StateReader::new(data);
        assert_eq!(
            input.read_u32(),
            1,
            "unsupported VirtPlatform snapshot version"
        );

        self.pcie.restore_state(&input.read_blob());
        self.nvme.restore_state(&input.read_blob());
        self.flash_vars.restore_state(&input.read_blob());

        let has_virtio_iso = input.read_bool();
        assert_eq!(
            has_virtio_iso,
            self.virtio_iso.is_some(),
            "legacy virtio ISO attachment mismatch on restore"
        );
        if let Some(device) = self.virtio_iso.as_mut() {
            device.restore_state(&input.read_blob());
        }

        let has_pci_boot_media = input.read_bool();
        assert_eq!(
            has_pci_boot_media,
            self.pci_boot_media.is_some(),
            "PCI boot-media attachment mismatch on restore"
        );
        if let Some(device) = self.pci_boot_media.as_mut() {
            device.restore_state(&input.read_blob());
        }

        let has_net = input.read_bool();
        assert_eq!(
            has_net,
            self.virtio_net.is_some(),
            "virtio-net presence mismatch on restore"
        );
        if let Some(device) = self.virtio_net.as_mut() {
            device.restore_state(&input.read_blob());
        }

        let has_gpu = input.read_bool();
        assert_eq!(
            has_gpu,
            self.virtio_gpu.is_some(),
            "virtio-gpu presence mismatch on restore"
        );
        if let Some(device) = self.virtio_gpu.as_mut() {
            device.restore_state(&input.read_blob());
        }

        let has_console = input.read_bool();
        assert_eq!(
            has_console,
            self.virtio_console.is_some(),
            "virtio-console presence mismatch on restore"
        );
        if let Some(device) = self.virtio_console.as_mut() {
            device.restore_state(&input.read_blob());
        }

        self.pending_msix.clear();
        let pending_msix = input.read_u32() as usize;
        self.pending_msix.reserve(pending_msix);
        for _ in 0..pending_msix {
            let vector = input.read_u16();
            assert_eq!(input.read_u16(), 0, "invalid pending MSI-X snapshot");
            let data = input.read_u32();
            let address = input.read_u64();
            self.pending_msix.push(MsixMessage {
                vector,
                address,
                data,
            });
        }

        self.pending_spi_levels.clear();
        let pending_spis = input.read_u32() as usize;
        self.pending_spi_levels.reserve(pending_spis);
        for _ in 0..pending_spis {
            let intid = input.read_u32();
            let level = input.read_bool();
            assert_eq!(input.read_u8(), 0, "invalid pending SPI snapshot");
            assert_eq!(input.read_u16(), 0, "invalid pending SPI snapshot");
            self.pending_spi_levels.push((intid, level));
        }

        self.nvme_ecam_touched = input.read_bool();
        self.nvme_mmio_reached = input.read_bool();
        self.nvme_cc_enabled = input.read_bool();
        self.nvme_admin_doorbell_rung = input.read_bool();

        self.nvme_completion_scratch.clear();
        self.host_now = None;
        self.xhci_dci3_last_emission = None;
        self.xhci_dci5_last_emission = None;
        input.finish();
    }
}
