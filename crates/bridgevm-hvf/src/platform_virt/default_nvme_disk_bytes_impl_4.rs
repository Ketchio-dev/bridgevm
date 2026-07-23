//! Continuation of the `default_nvme_disk_bytes` impl block, split for the 1000-line rule.

use super::*;

use crate::fwcfg::GuestMemoryMut;
use crate::hda::HdaPcmSink;
use crate::machine;
use crate::msix::MsixMessage;
use crate::ramfb::RAMFB_FW_CFG_FILE;
use crate::virtio_blk::VirtioMmioBlockResult;
use crate::virtio_blk::VirtioPciBlockOp;
use crate::virtio_console::VirtioConsoleResult;
use crate::virtio_console::VirtioPciConsoleOp;

impl VirtPlatform {
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

    pub(crate) fn hda_access(
        &mut self,
        offset: u64,
        op: MmioOp,
        mem: &mut dyn GuestMemoryMut,
    ) -> MmioOutcome {
        let outcome = {
            let Some(hda) = self.hda.as_mut() else {
                return MmioOutcome::KnownUnimplemented("intel-hda");
            };
            match op {
                MmioOp::Read { size } => MmioOutcome::ReadValue(hda.mmio_read(offset, size)),
                MmioOp::Write { size, value } => {
                    hda.mmio_write(offset, size, value, mem);
                    MmioOutcome::WriteAck
                }
            }
        };
        self.flush_hda_pending_msi();
        outcome
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

    /// Retire host-paced vblank NOPs and venus fences, then flush their
    /// interrupts without guest MMIO. Completion must not depend on the guest
    /// touching the device: a guest blocked in vkWaitForFences sits in WFI
    /// generating no virtio-gpu accesses, so the per-exit drain path calls this
    /// while timer exits keep it running.
    pub fn poll_virtio_gpu_fences(&mut self, mem: &mut dyn GuestMemoryMut) {
        if let Some(dev) = self.virtio_gpu.as_mut() {
            dev.service_deferred_3d_scanout();
            dev.drain_host_vblank(mem);
            dev.drain_completed_fences(mem);
            self.flush_virtio_gpu_pending_msix();
        }
    }

    /// Advance the host-clock-paced HDA playback stream and flush standard MSI raised
    /// by IOC or DMA errors into the platform's pending-message aggregation.
    pub fn poll_hda(&mut self, mem: &mut dyn GuestMemoryMut) {
        if let Some(hda) = self.hda.as_mut() {
            hda.poll(mem, self.host_now);
        }
        self.flush_hda_pending_msi();
    }

    /// Install or clear the host PCM sink for the optional HDA controller.
    pub fn set_hda_pcm_sink(&mut self, sink: Option<Box<dyn HdaPcmSink>>) {
        if let Some(hda) = self.hda.as_mut() {
            hda.set_pcm_sink(sink);
        }
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

    pub(crate) fn uart_access(&mut self, offset: u64, op: MmioOp) -> MmioOutcome {
        match op {
            MmioOp::Read { size } => MmioOutcome::ReadValue(self.uart.mmio_read(offset, size)),
            MmioOp::Write { size, value } => {
                self.uart.mmio_write(offset, size, value);
                MmioOutcome::WriteAck
            }
        }
    }

    pub(crate) fn rtc_access(&mut self, offset: u64, op: MmioOp) -> MmioOutcome {
        match op {
            MmioOp::Read { size } => MmioOutcome::ReadValue(self.rtc.mmio_read(offset, size)),
            MmioOp::Write { size, value } => {
                self.rtc.mmio_write(offset, size, value);
                MmioOutcome::WriteAck
            }
        }
    }

    /// Bytes the guest/firmware has written to the UART so far.
    pub fn uart_output(&self) -> &[u8] {
        self.uart.output()
    }

    /// Drain and return everything the guest has written to the UART since the
    /// last drain. Used by the KD serial bridge to forward the guest's
    /// KDCOM/serial-debug transmit stream to a host socket; unlike
    /// `uart_output()` (a non-draining borrow the boot scanner reads) this
    /// consumes the buffer so bytes are forwarded exactly once.
    pub fn take_uart_output(&mut self) -> Vec<u8> {
        self.uart.take_output()
    }

    /// Queue bytes that the guest can read from the PL011 UART data register.
    /// Live probes use this to test firmware/loader input paths while the default
    /// platform remains an unattached, receive-empty serial backend.
    pub fn push_uart_input(&mut self, bytes: &[u8]) {
        self.uart.push_input(bytes);
    }

    /// Number of preloaded PL011 input bytes still waiting to be read.
    pub fn uart_input_len(&self) -> usize {
        self.uart.input_len()
    }

    pub(crate) fn fw_cfg_access(
        &mut self,
        offset: u64,
        op: MmioOp,
        mem: &mut dyn GuestMemoryMut,
    ) -> MmioOutcome {
        match op {
            MmioOp::Read { size } => MmioOutcome::ReadValue(self.fw_cfg.mmio_read(offset, size)),
            MmioOp::Write { size, value } => {
                self.fw_cfg.mmio_write(offset, size, value, mem);
                self.refresh_ramfb();
                MmioOutcome::WriteAck
            }
        }
    }

    pub(crate) fn refresh_ramfb(&mut self) {
        if !self.devices.ramfb_present {
            return;
        }
        if let Some(bytes) = self.fw_cfg.file_bytes(RAMFB_FW_CFG_FILE) {
            self.ramfb.update_from_fw_cfg(bytes);
        }
    }

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
