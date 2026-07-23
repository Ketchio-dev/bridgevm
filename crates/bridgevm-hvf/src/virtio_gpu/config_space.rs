//! Common-config and device-config register plane: feature negotiation, status, queue registers, MMIO alias.

use super::*;
use crate::fwcfg::GuestMemoryMut;
use crate::virtio_gpu_3d::VIRTIO_GPU_F_CONTEXT_INIT;
use crate::virtio_gpu_3d::VIRTIO_GPU_F_RESOURCE_BLOB;
use crate::virtio_gpu_3d::VIRTIO_GPU_F_VIRGL;
use crate::virtio_gpu_trace::venus_start_trace_enabled;
use std::fmt::Write as _;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;

impl VirtioGpu {
    pub(crate) fn access_common(
        &mut self,
        offset: u64,
        is_write: bool,
        size: u8,
        value: u64,
        mem: &mut dyn GuestMemoryMut,
    ) -> VirtioGpuResult {
        if !is_write {
            let value = self.read_common(offset, size);
            self.trace_common_read(offset, size, value);
            return VirtioGpuResult::ReadValue(value);
        }
        self.write_common(offset, size, value, mem);
        VirtioGpuResult::WriteAck
    }

    pub(crate) fn read_common(&self, offset: u64, size: u8) -> u64 {
        if let Some(value) = self.read_common_field(offset, size) {
            return value;
        }
        self.read_mmio_alias(offset, size)
    }

    pub(crate) fn read_mmio_alias(&self, offset: u64, size: u8) -> u64 {
        let value = match offset {
            REG_MAGIC => u64::from(MAGIC_VALUE),
            REG_VERSION => u64::from(VERSION_MODERN),
            REG_DEVICE_ID => u64::from(DEVICE_ID_GPU),
            REG_VENDOR_ID => u64::from(VENDOR_ID_QEMU),
            REG_DEVICE_FEATURES => u64::from(self.offered_features_word(self.device_features_sel)),
            REG_DRIVER_FEATURES => {
                u64::from(self.driver_features[self.driver_features_sel.min(1) as usize])
            }
            REG_QUEUE_NUM_MAX => {
                if self.selected_queue().is_some() {
                    u64::from(QUEUE_MAX)
                } else {
                    0
                }
            }
            REG_QUEUE_NUM => self.selected_queue().map_or(0, |q| u64::from(q.size)),
            REG_QUEUE_READY => self
                .selected_queue()
                .map_or(0, |q| u64::from(q.ready as u8)),
            REG_INTERRUPT_STATUS => u64::from(self.interrupt_status),
            REG_STATUS => u64::from(self.status),
            REG_QUEUE_DESC_LOW => self.selected_queue().map_or(0, |q| q.desc & 0xffff_ffff),
            REG_QUEUE_DESC_HIGH => self.selected_queue().map_or(0, |q| q.desc >> 32),
            REG_QUEUE_DRIVER_LOW => self.selected_queue().map_or(0, |q| q.driver & 0xffff_ffff),
            REG_QUEUE_DRIVER_HIGH => self.selected_queue().map_or(0, |q| q.driver >> 32),
            REG_QUEUE_DEVICE_LOW => self.selected_queue().map_or(0, |q| q.device & 0xffff_ffff),
            REG_QUEUE_DEVICE_HIGH => self.selected_queue().map_or(0, |q| q.device >> 32),
            REG_CONFIG_GENERATION => 0,
            _ => 0,
        };
        mask_to_size(value, size)
    }

    pub(crate) fn write_common(
        &mut self,
        offset: u64,
        size: u8,
        value: u64,
        mem: &mut dyn GuestMemoryMut,
    ) {
        if self.write_common_field(offset, size, value) {
            return;
        }
        self.write_mmio_alias(offset, value, mem);
    }

    pub(crate) fn write_mmio_alias(
        &mut self,
        offset: u64,
        value: u64,
        mem: &mut dyn GuestMemoryMut,
    ) {
        match offset {
            REG_DEVICE_FEATURES_SEL => self.device_features_sel = value as u32,
            REG_DRIVER_FEATURES_SEL => self.driver_features_sel = value as u32,
            REG_DRIVER_FEATURES => self.write_driver_features(value),
            REG_QUEUE_SEL => self.queue_sel = value as u32,
            REG_QUEUE_NUM => self.write_selected_queue(|q| q.size = (value as u16).min(QUEUE_MAX)),
            REG_QUEUE_READY => self.write_selected_queue(|q| {
                q.ready = value != 0;
                if !q.ready {
                    q.last_avail_idx = 0;
                }
            }),
            REG_QUEUE_NOTIFY => self.notify_queue(value as u16, mem),
            REG_INTERRUPT_ACK => self.interrupt_status &= !(value as u32),
            REG_STATUS => self.write_status(value),
            REG_QUEUE_DESC_LOW => self.write_selected_queue(|q| q.desc = set_low(q.desc, value)),
            REG_QUEUE_DESC_HIGH => self.write_selected_queue(|q| q.desc = set_high(q.desc, value)),
            REG_QUEUE_DRIVER_LOW => {
                self.write_selected_queue(|q| q.driver = set_low(q.driver, value))
            }
            REG_QUEUE_DRIVER_HIGH => {
                self.write_selected_queue(|q| q.driver = set_high(q.driver, value))
            }
            REG_QUEUE_DEVICE_LOW => {
                self.write_selected_queue(|q| q.device = set_low(q.device, value))
            }
            REG_QUEUE_DEVICE_HIGH => {
                self.write_selected_queue(|q| q.device = set_high(q.device, value))
            }
            _ => {}
        }
    }

    pub(crate) fn write_driver_features(&mut self, value: u64) {
        if self.driver_features_sel < 2 {
            let index = self.driver_features_sel as usize;
            self.driver_features[index] = (value as u32) & self.offered_features_word(index as u32);
            let select = self.driver_features_sel;
            let raw = value as u32;
            let accepted = self.driver_features[index];
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
    }

    pub(crate) fn offered_features_word(&self, select: u32) -> u32 {
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

    pub(crate) fn read_common_field(&self, offset: u64, size: u8) -> Option<u64> {
        if !is_supported_common_access_size(size) {
            return None;
        }
        let selected_queue = self.selected_queue();
        let fields = [
            (
                COMMON_DEVICE_FEATURE_SELECT,
                4,
                u64::from(self.device_features_sel),
            ),
            (
                COMMON_DEVICE_FEATURE,
                4,
                u64::from(self.offered_features_word(self.device_features_sel)),
            ),
            (
                COMMON_DRIVER_FEATURE_SELECT,
                4,
                u64::from(self.driver_features_sel),
            ),
            (
                COMMON_DRIVER_FEATURE,
                4,
                u64::from(self.driver_features[self.driver_features_sel.min(1) as usize]),
            ),
            (
                COMMON_CONFIG_MSIX_VECTOR,
                2,
                u64::from(self.config_msix_vector),
            ),
            (COMMON_NUM_QUEUES, 2, QUEUE_COUNT as u64),
            (COMMON_DEVICE_STATUS, 1, u64::from(self.status & 0xff)),
            (COMMON_CONFIG_GENERATION, 1, 0),
            (COMMON_QUEUE_SELECT, 2, u64::from(self.queue_sel as u16)),
            (
                COMMON_QUEUE_SIZE,
                2,
                selected_queue.map_or(0, |q| {
                    u64::from(if q.size == 0 { QUEUE_MAX } else { q.size })
                }),
            ),
            (
                COMMON_QUEUE_MSIX_VECTOR,
                2,
                selected_queue.map_or(u64::from(VIRTIO_MSI_NO_VECTOR), |q| {
                    u64::from(q.msix_vector)
                }),
            ),
            (
                COMMON_QUEUE_ENABLE,
                2,
                selected_queue.map_or(0, |q| u64::from(q.ready as u8)),
            ),
            (
                COMMON_QUEUE_NOTIFY_OFF,
                2,
                selected_queue.map_or(0, |q| u64::from(q.notify_off)),
            ),
            (COMMON_QUEUE_DESC, 8, selected_queue.map_or(0, |q| q.desc)),
            (
                COMMON_QUEUE_DRIVER,
                8,
                selected_queue.map_or(0, |q| q.driver),
            ),
            (
                COMMON_QUEUE_DEVICE,
                8,
                selected_queue.map_or(0, |q| q.device),
            ),
        ];
        fields.iter().find_map(|(base, width, value)| {
            read_common_register(*base, *width, *value, offset, size)
        })
    }

    pub(crate) fn write_common_field(&mut self, offset: u64, size: u8, value: u64) -> bool {
        if !is_supported_common_access_size(size) {
            return false;
        }
        if common_access_touches(COMMON_DEVICE_FEATURE_SELECT, 4, offset, size) {
            self.device_features_sel = write_common_register(
                self.device_features_sel.into(),
                COMMON_DEVICE_FEATURE_SELECT,
                4,
                offset,
                size,
                value,
            ) as u32;
            return true;
        }
        if common_access_touches(COMMON_DRIVER_FEATURE_SELECT, 4, offset, size) {
            self.driver_features_sel = write_common_register(
                self.driver_features_sel.into(),
                COMMON_DRIVER_FEATURE_SELECT,
                4,
                offset,
                size,
                value,
            ) as u32;
            return true;
        }
        if common_access_touches(COMMON_DRIVER_FEATURE, 4, offset, size) {
            let current = self.driver_features[self.driver_features_sel.min(1) as usize];
            let merged = write_common_register(
                current.into(),
                COMMON_DRIVER_FEATURE,
                4,
                offset,
                size,
                value,
            );
            self.write_driver_features(merged);
            return true;
        }
        if common_access_touches(COMMON_CONFIG_MSIX_VECTOR, 2, offset, size) {
            let vector = write_common_register(
                self.config_msix_vector.into(),
                COMMON_CONFIG_MSIX_VECTOR,
                2,
                offset,
                size,
                value,
            ) as u16;
            self.config_msix_vector = valid_msix_vector(vector);
            if venus_start_trace_enabled() {
                println!(
                    "venus-start: config_msix_vector write raw={vector} accepted={}",
                    self.config_msix_vector
                );
            }
            return true;
        }
        if common_access_touches(COMMON_DEVICE_STATUS, 1, offset, size) {
            let status = write_common_register(
                u64::from(self.status & 0xff),
                COMMON_DEVICE_STATUS,
                1,
                offset,
                size,
                value,
            );
            self.write_status(status);
            return true;
        }
        if common_access_touches(COMMON_QUEUE_SELECT, 2, offset, size) {
            self.queue_sel = write_common_register(
                u64::from(self.queue_sel as u16),
                COMMON_QUEUE_SELECT,
                2,
                offset,
                size,
                value,
            ) as u32;
            return true;
        }
        let Some(queue) = self.queues.get_mut(self.queue_sel as usize) else {
            return common_access_touches_queue_field(offset, size);
        };
        if common_access_touches(COMMON_QUEUE_SIZE, 2, offset, size) {
            queue.size = (write_common_register(
                u64::from(queue.size),
                COMMON_QUEUE_SIZE,
                2,
                offset,
                size,
                value,
            ) as u16)
                .min(QUEUE_MAX);
            return true;
        }
        if common_access_touches(COMMON_QUEUE_MSIX_VECTOR, 2, offset, size) {
            let vector = write_common_register(
                u64::from(queue.msix_vector),
                COMMON_QUEUE_MSIX_VECTOR,
                2,
                offset,
                size,
                value,
            ) as u16;
            queue.msix_vector = valid_msix_vector(vector);
            if venus_start_trace_enabled() {
                println!(
                    "venus-start: queue={} msix_vector write raw={vector} accepted={}",
                    self.queue_sel, queue.msix_vector
                );
            }
            return true;
        }
        if common_access_touches(COMMON_QUEUE_ENABLE, 2, offset, size) {
            let enable = write_common_register(
                u64::from(queue.ready as u8),
                COMMON_QUEUE_ENABLE,
                2,
                offset,
                size,
                value,
            );
            queue.ready = enable == 1;
            if !queue.ready {
                queue.last_avail_idx = 0;
            }
            if venus_start_trace_enabled() {
                println!(
                    "venus-start: queue={} enable write {} size={} desc={:#x} driver={:#x} device={:#x} msix_vector={}",
                    self.queue_sel, enable, queue.size, queue.desc, queue.driver, queue.device, queue.msix_vector
                );
            }
            return true;
        }
        if common_access_touches(COMMON_QUEUE_DESC, 8, offset, size) {
            queue.desc =
                write_common_register(queue.desc, COMMON_QUEUE_DESC, 8, offset, size, value);
            return true;
        }
        if common_access_touches(COMMON_QUEUE_DRIVER, 8, offset, size) {
            queue.driver =
                write_common_register(queue.driver, COMMON_QUEUE_DRIVER, 8, offset, size, value);
            return true;
        }
        if common_access_touches(COMMON_QUEUE_DEVICE, 8, offset, size) {
            queue.device =
                write_common_register(queue.device, COMMON_QUEUE_DEVICE, 8, offset, size, value);
            return true;
        }
        false
    }

    pub(crate) fn write_status(&mut self, value: u64) {
        let raw = value as u32;
        let previous = self.status;
        let driver_features_word0 = self.driver_features[0];
        let driver_features_word1 = self.driver_features[1];
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
        self.status = value as u32;
        if value == 0 {
            self.reset_runtime_state();
        }
    }

    pub(crate) fn config_read(&self, offset: u64, size: u8) -> u64 {
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

    pub(crate) fn config_write(&mut self, offset: u64, size: u8, value: u64) {
        if common_access_touches(4, 4, offset, size) {
            self.events_clear =
                write_common_register(self.events_clear.into(), 4, 4, offset, size, value) as u32;
            // The driver acks a display event by writing its bit to
            // events_clear; clear the matching events_read bits so the next
            // GET_DISPLAY_INFO does not re-report a stale change.
            self.events_read &= !self.events_clear;
        }
    }
}
