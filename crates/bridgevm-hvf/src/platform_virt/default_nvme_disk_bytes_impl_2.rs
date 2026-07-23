//! Continuation of the `default_nvme_disk_bytes` impl block, split for the 1000-line rule.

use super::*;

use crate::fwcfg::GuestMemoryMut;
use crate::ramfb::RamfbConfig;
use crate::virtio_blk::VirtioBlockRequestTrace;
use crate::virtio_blk::VirtioMmioBlock;
use crate::virtio_console::VirtioPciConsole;
use crate::virtio_gpu::VirtioPciGpu;
use crate::virtio_gpu_3d::GpuShmMapPort;
use crate::xhci::PointerInputAction;
use crate::xhci::SetupInputAction;
use crate::xhci::XhciEventLifecycleStats;
use crate::xhci::XhciHidSemanticStats;
use crate::xhci::XhciPointerInputQueueError;
use crate::xhci::XhciPointerInputReportStats;
use crate::xhci::XhciSetupInputQueueError;
use crate::xhci::XhciSetupInputReportStats;
use std::io;
use std::path::Path;
use std::time::Instant;

impl VirtPlatform {
    /// Current virtio-gpu scanout geometry, if the device is present.
    pub fn virtio_gpu_resolution(&self) -> Option<(u32, u32)> {
        self.virtio_gpu
            .as_ref()
            .map(VirtioPciGpu::display_resolution)
    }

    pub fn set_virtio_gpu_shm_map_port(&mut self, port: Box<dyn GpuShmMapPort>) -> bool {
        let Some(window_size) = self.pcie.virtio_gpu_host_visible_bar_size() else {
            return false;
        };
        let Some(gpu) = self.virtio_gpu.as_mut() else {
            return false;
        };
        gpu.set_shm_map_port(port, window_size);
        true
    }

    pub fn virtio_gpu_host_visible_bar_base(&self) -> Option<u64> {
        self.pcie.virtio_gpu_host_visible_bar_base()
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

    pub fn virtio_iso_request_trace(&self) -> Option<Vec<VirtioBlockRequestTrace>> {
        self.virtio_iso
            .as_ref()
            .map(VirtioMmioBlock::recent_request_trace)
    }

    pub fn queue_xhci_hid_boot_key_usage(
        &mut self,
        usage: u8,
    ) -> Result<(), XhciHidBootKeyQueueError> {
        if !self.devices.xhci_present {
            self.xhci_hid_boot_key_report_stats.busy_rejections = self
                .xhci_hid_boot_key_report_stats
                .busy_rejections
                .saturating_add(1);
            return Err(XhciHidBootKeyQueueError::Busy);
        }
        if usage != HID_BOOT_KEYBOARD_USAGE_SPACE {
            self.xhci_hid_boot_key_report_stats
                .unsupported_usage_rejections = self
                .xhci_hid_boot_key_report_stats
                .unsupported_usage_rejections
                .saturating_add(1);
            return Err(XhciHidBootKeyQueueError::UnsupportedUsage { usage });
        }
        self.queue_xhci_setup_input_actions(&[SetupInputAction::Space])
            .map_err(|error| match error {
                XhciSetupInputQueueError::Busy => XhciHidBootKeyQueueError::Busy,
                XhciSetupInputQueueError::EmptySequence
                | XhciSetupInputQueueError::TooManyActions { .. } => XhciHidBootKeyQueueError::Busy,
            })?;
        self.xhci_hid_boot_key_report_stats.queued_space_reports = self
            .xhci_hid_boot_key_report_stats
            .queued_space_reports
            .saturating_add(1);
        Ok(())
    }

    pub fn queue_xhci_setup_input_actions(
        &mut self,
        actions: &[SetupInputAction],
    ) -> Result<(), XhciSetupInputQueueError> {
        if !self.devices.xhci_present {
            self.xhci_hid_boot_key_report_stats.busy_rejections = self
                .xhci_hid_boot_key_report_stats
                .busy_rejections
                .saturating_add(1);
            return Err(XhciSetupInputQueueError::Busy);
        }
        match self.xhci.queue_setup_input_actions(actions) {
            Ok(()) => Ok(()),
            Err(XhciSetupInputQueueError::Busy) => {
                self.xhci_hid_boot_key_report_stats.busy_rejections = self
                    .xhci_hid_boot_key_report_stats
                    .busy_rejections
                    .saturating_add(1);
                Err(XhciSetupInputQueueError::Busy)
            }
            Err(error) => Err(error),
        }
    }

    pub fn queue_xhci_setup_input_actions_with_mem(
        &mut self,
        actions: &[SetupInputAction],
        mem: &mut dyn GuestMemoryMut,
    ) -> Result<(), XhciSetupInputQueueError> {
        self.queue_xhci_setup_input_actions(actions)?;
        self.drain_xhci_setup_input_reports(mem);
        Ok(())
    }

    pub fn drain_xhci_setup_input_reports(&mut self, mem: &mut dyn GuestMemoryMut) -> bool {
        if !self.devices.xhci_present {
            return false;
        }
        let mut posted_completion = false;
        let stats = self.xhci.setup_input_report_stats();
        let emitted_reports = stats
            .emitted_key_reports
            .saturating_add(stats.emitted_release_reports);
        let pending_reports = stats.queued_reports.saturating_sub(emitted_reports);
        for _ in 0..pending_reports.min(MAX_XHCI_SETUP_INPUT_DRAIN_ATTEMPTS as u64) {
            if !self.report_pacing_allows_emission(self.xhci_dci3_last_emission) {
                break;
            }
            if !self.xhci.process_queued_dci3_input(mem) {
                break;
            }
            posted_completion = true;
            self.queue_xhci_completion_msix();
            self.xhci_dci3_last_emission = self.host_now.or(self.xhci_dci3_last_emission);
        }
        if posted_completion {
            self.flush_xhci_pending_msix();
        }
        posted_completion
    }

    /// Report-pacing gate: while `host_now` is unset (unit tests) or the interval
    /// is zero, every emission is allowed (unpaced). Otherwise an emission is
    /// held off until the configured interval has elapsed since this endpoint's
    /// last emission. Because a single drain call sees one fixed `host_now`, this
    /// releases at most one report per run-loop iteration once pacing is active.
    pub(crate) fn report_pacing_allows_emission(&self, last_emission: Option<Instant>) -> bool {
        match self.host_now {
            None => true,
            Some(now) => {
                report_pacing_allows_emission(self.xhci_report_interval, last_emission, now)
            }
        }
    }

    pub fn xhci_hid_boot_key_report_stats(&self) -> XhciHidBootKeyReportStats {
        self.xhci_hid_boot_key_report_stats
    }

    pub fn xhci_setup_input_report_stats(&self) -> XhciSetupInputReportStats {
        self.xhci.setup_input_report_stats()
    }

    pub fn queue_xhci_pointer_input_actions(
        &mut self,
        actions: &[PointerInputAction],
    ) -> Result<(), XhciPointerInputQueueError> {
        if !self.devices.xhci_present {
            return Err(XhciPointerInputQueueError::Busy);
        }
        self.xhci.queue_pointer_input_actions(actions)
    }

    pub fn queue_xhci_pointer_input_actions_with_mem(
        &mut self,
        actions: &[PointerInputAction],
        mem: &mut dyn GuestMemoryMut,
    ) -> Result<(), XhciPointerInputQueueError> {
        self.queue_xhci_pointer_input_actions(actions)?;
        self.drain_xhci_pointer_input_reports(mem);
        Ok(())
    }

    pub fn drain_xhci_pointer_input_reports(&mut self, mem: &mut dyn GuestMemoryMut) -> bool {
        if !self.devices.xhci_present {
            return false;
        }
        let mut posted_completion = false;
        let stats = self.xhci.pointer_input_report_stats();
        let emitted_reports = stats
            .emitted_move_reports
            .saturating_add(stats.emitted_button_reports)
            .saturating_add(stats.emitted_release_reports)
            .saturating_add(stats.emitted_wheel_reports);
        let pending_reports = stats.queued_reports.saturating_sub(emitted_reports);
        for _ in 0..pending_reports.min(MAX_XHCI_SETUP_INPUT_DRAIN_ATTEMPTS as u64) {
            if !self.report_pacing_allows_emission(self.xhci_dci5_last_emission) {
                break;
            }
            if !self.xhci.process_queued_dci5_pointer_input(mem) {
                break;
            }
            posted_completion = true;
            self.queue_xhci_completion_msix();
            self.xhci_dci5_last_emission = self.host_now.or(self.xhci_dci5_last_emission);
        }
        if posted_completion {
            self.flush_xhci_pending_msix();
        }
        posted_completion
    }

    pub fn xhci_pointer_input_report_stats(&self) -> XhciPointerInputReportStats {
        self.xhci.pointer_input_report_stats()
    }

    pub fn xhci_event_lifecycle_stats(&self) -> XhciEventLifecycleStats {
        self.xhci.event_lifecycle_stats()
    }

    pub fn xhci_hid_semantic_stats(&self) -> XhciHidSemanticStats {
        self.xhci.hid_semantic_stats()
    }

    pub fn ramfb_config(&self) -> Option<RamfbConfig> {
        if !self.devices.ramfb_present {
            return None;
        }
        self.ramfb.config()
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
}
