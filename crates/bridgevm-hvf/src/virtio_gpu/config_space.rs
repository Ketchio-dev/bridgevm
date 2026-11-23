//! virtio-gpu on the shared virtio-PCI transport: features, device config, MSI-X vector
//! limits, fence draining after common writes, and register trace output.

use super::interrupt::valid_msix_vector;
use super::*;
use crate::fwcfg::GuestMemoryMut;
use crate::msix::{MsixMessage, MsixTable};
use crate::virtio_gpu_3d::VIRTIO_GPU_F_CONTEXT_INIT;
use crate::virtio_gpu_3d::VIRTIO_GPU_F_RESOURCE_BLOB;
use crate::virtio_gpu_3d::VIRTIO_GPU_F_VIRGL;
use crate::virtio_gpu_trace::venus_start_trace_enabled;
use std::fmt::Write as _;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;

impl VirtioPciDevice<(), QUEUE_COUNT> for VirtioGpu {
    const DEVICE_ID: u32 = DEVICE_ID_GPU;
    const QUEUE_MAX: u16 = QUEUE_MAX;

    fn pci(&self) -> &VirtioPciState<(), QUEUE_COUNT> {
        &self.pci
    }

    fn pci_mut(&mut self) -> &mut VirtioPciState<(), QUEUE_COUNT> {
        &mut self.pci
    }

    fn offered_features_word(&self, select: u32) -> u32 {
        match select {
            0 => {
                let mut features = VIRTIO_GPU_F_EDID;
                if self.three_d.has_backend() {
                    features |=
                        VIRTIO_GPU_F_VIRGL | VIRTIO_GPU_F_RESOURCE_BLOB | VIRTIO_GPU_F_CONTEXT_INIT;
                }
                features
            }
            1 => VIRTIO_F_VERSION_1,
            _ => 0,
        }
    }

    fn reset_device(&mut self) {
        self.reset_runtime_state();
    }

    fn notify(&mut self, queue_index: u16, mem: &mut dyn GuestMemoryMut) {
        self.notify_queue(queue_index, mem);
    }

    fn config_read(&self, offset: u64, size: u8) -> u64 {
        // struct virtio_gpu_config: le32 events_read @0, le32 events_clear @4,
        // le32 num_scanouts @8, le32 num_capsets @12. num_capsets was being
        // written into the num_scanouts slot, so Linux saw "number of cap
        // sets: 0" and never queried the venus capset (and a 2D-only device
        // reported zero scanouts).
        let mut config = [0u8; 16];
        config[0..4].copy_from_slice(&self.events_read.to_le_bytes());
        config[4..8].copy_from_slice(&self.events_clear.to_le_bytes());
        config[8..12].copy_from_slice(&1u32.to_le_bytes());
        let num_capsets = self.three_d.capset_count();
        config[12..16].copy_from_slice(&num_capsets.to_le_bytes());
        let value = read_le_from_bytes(&config, offset, size).unwrap_or(0);
        if venus_start_trace_enabled() {
            static COUNT: AtomicU64 = AtomicU64::new(0);
            let n = COUNT.fetch_add(1, Ordering::Relaxed) + 1;
            if trace_sample(n) {
                println!(
                    "venus-start: config_read n={n} off={offset:#x} size={size} value={value:#x} num_capsets={num_capsets}"
                );
            }
        }
        value
    }

    fn config_write(&mut self, offset: u64, size: u8, value: u64) {
        if common_access_touches(4, 4, offset, size) {
            self.events_clear =
                write_common_register(self.events_clear.into(), 4, 4, offset, size, value) as u32;
            // The driver acks a display event by writing its bit to
            // events_clear; clear the matching events_read bits so the next
            // GET_DISPLAY_INFO does not re-report a stale change.
            self.events_read &= !self.events_clear;
        }
    }

    /// The device has VIRTIO_GPU_MSIX_VECTOR_COUNT vectors; anything else is NO_VECTOR.
    fn accept_msix_vector(&self, vector: u16) -> u16 {
        valid_msix_vector(vector)
    }

    fn after_common_read(&mut self, offset: u64, size: u8, value: u64) {
        self.trace_common_read(offset, size, value);
    }

    fn after_common_write(&mut self, mem: &mut dyn GuestMemoryMut) {
        self.drain_completed_fences(mem);
    }

    fn before_status_write(&mut self, value: u64) {
        let raw = value as u32;
        let previous = self.pci.status;
        let driver_features_word0 = self.pci.driver_features[0];
        let driver_features_word1 = self.pci.driver_features[1];
        let resources = self.resources.len();
        let scanout_active = self.scanout_resource.is_some() || self.blob_scanout.is_some();
        if venus_start_trace_enabled() {
            println!("venus-start: device_status write {raw:#x}");
        }
        self.record_trace_fields("device_status", |fields| {
            let _ = write!(
                fields,
                ",\"raw\":{},\"raw_hex\":\"{:#x}\",\"previous\":{},\"previous_hex\":\"{:#x}\",\"reset\":{},\"driver_features_word0\":{},\"driver_features_word0_hex\":\"{:#x}\",\"driver_features_word1\":{},\"driver_features_word1_hex\":\"{:#x}\",\"resources\":{},\"scanout_active\":{}",
                raw,
                raw,
                previous,
                previous,
                raw == 0,
                driver_features_word0,
                driver_features_word0,
                driver_features_word1,
                driver_features_word1,
                resources,
                scanout_active
            );
        });
    }

    fn after_driver_features_write(&mut self, value: u64) {
        let select = self.pci.driver_features_sel;
        let raw = value as u32;
        let accepted = self.pci.driver_features[select as usize];
        if venus_start_trace_enabled() {
            println!(
                "venus-start: driver_features select={select} raw={raw:#x} accepted={accepted:#x} offered={:#x}",
                self.offered_features_word(select)
            );
        }
        self.record_trace_fields("driver_features", |fields| {
            let _ = write!(
                fields,
                ",\"select\":{},\"raw\":{},\"accepted\":{},\"accepted_hex\":\"{:#x}\"",
                select, raw, accepted, accepted
            );
        });
    }

    fn after_msix_vector_write(&self, queue: Option<u32>, vector: u16) {
        if !venus_start_trace_enabled() {
            return;
        }
        match queue {
            None => println!(
                "venus-start: config_msix_vector write raw={vector} accepted={}",
                self.pci.config_msix_vector
            ),
            Some(queue_sel) => println!(
                "venus-start: queue={} msix_vector write raw={vector} accepted={}",
                queue_sel, self.pci.queues[queue_sel as usize].msix_vector
            ),
        }
    }

    fn after_queue_enable_write(&self, enable: u64) {
        if !venus_start_trace_enabled() {
            return;
        }
        let Some(queue) = self.pci.selected_queue() else {
            return;
        };
        println!(
            "venus-start: queue={} enable write {} size={} desc={:#x} driver={:#x} device={:#x} msix_vector={}",
            self.pci.queue_sel, enable, queue.size, queue.desc, queue.driver, queue.device, queue.msix_vector
        );
    }

    fn raise_config_msix(
        &mut self,
        msix: &mut MsixTable,
        function_enabled: bool,
        function_masked: bool,
        out: &mut Vec<MsixMessage>,
    ) {
        self.raise_config_change_msix(msix, function_enabled, function_masked, out);
    }

    fn msix_delivered(&mut self, vector: u16) {
        if vector == self.pci.config_msix_vector {
            self.pending_config_change = false;
        }
    }

    fn trace_queue_msix(&self, what: &str, queue_index: usize, vector: u16) {
        venus_start_trace_msix_queue(what, queue_index, vector);
    }
}
