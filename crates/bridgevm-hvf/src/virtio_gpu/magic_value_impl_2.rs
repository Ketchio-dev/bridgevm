//! Continuation of the `magic_value` impl block, split for the 1000-line rule.

use super::*;
use crate::fwcfg::GuestMemoryMut;
use crate::ramfb::DRM_FORMAT_XRGB8888;
use crate::virtio_gpu_3d::VIRTIO_GPU_F_CONTEXT_INIT;
use crate::virtio_gpu_3d::VIRTIO_GPU_F_RESOURCE_BLOB;
use crate::virtio_gpu_3d::VIRTIO_GPU_F_VIRGL;
use crate::virtio_gpu_trace::venus_start_trace_enabled;
use std::fmt::Write as _;

impl VirtioGpu {
    pub fn new_from_env() -> Self {
        let (width, height) = parse_resolution_env();
        Self::new(width, height)
    }

    pub fn stats(&self) -> VirtioGpuStats {
        let mut stats = VirtioGpuStats {
            status: self.status,
            interrupt_status: self.interrupt_status,
            driver_features: u64::from(self.driver_features[0])
                | (u64::from(self.driver_features[1]) << 32),
            resources: self.resources.len(),
            scanout_active: self.scanout_resource.is_some() || self.blob_scanout.is_some(),
            scanout_3d_flushes: self.scanout_3d_flush_count,
            vblank_paced_count: self.vblank_paced_count,
            scanout_readback_attempts: self.scanout_readback_attempt_count,
            scanout_readbacks: self.scanout_readback_count,
            scanout_readback_throttled: self.scanout_readback_throttled_count,
            scanout_readback_bytes: self.scanout_readback_bytes,
            scanout_readback_nanoseconds: self.scanout_readback_nanoseconds,
            deferred_scanout_flushes: self.deferred_scanout_flush_count,
            deferred_scanout_serviced: self.deferred_scanout_serviced_count,
            scanout_blits: self.scanout_blit_count,
            three_d: self.three_d.stats(self.pending_fenced.len()),
            queues: [VirtioGpuQueueStats::default(); QUEUE_COUNT],
        };
        for (out, queue) in stats.queues.iter_mut().zip(self.queues) {
            *out = VirtioGpuQueueStats {
                size: queue.size,
                ready: queue.ready,
                desc: queue.desc,
                driver: queue.driver,
                device: queue.device,
                msix_vector: queue.msix_vector,
                notify_off: queue.notify_off,
                last_avail_idx: queue.last_avail_idx,
                pending_msix: queue.pending_msix,
            };
        }
        stats
    }

    pub fn interrupt_line_level(&self) -> bool {
        self.interrupt_status != 0
    }

    pub fn reset_runtime_state(&mut self) {
        let width = self.width;
        let height = self.height;
        self.device_features_sel = 0;
        self.driver_features_sel = 0;
        self.driver_features = [0; 2];
        self.config_msix_vector = VIRTIO_MSI_NO_VECTOR;
        self.queue_sel = 0;
        for queue in &mut self.queues {
            queue.reset();
        }
        self.pending_msix_queue_bits = 0;
        self.status = 0;
        self.interrupt_status = 0;
        self.events_read = 0;
        self.events_clear = 0;
        self.pending_config_change = false;
        self.resources.clear();
        self.scanout_resource = None;
        self.unbind_blob_scanout();
        self.scanout.clear();
        self.scanout.resize(scanout_len(width, height), 0);
        self.three_d.reset();
        self.pending_fenced.clear();
        self.pending_vblank.clear();
        self.completed_fences_scratch.clear();
        self.descriptor_scratch.clear();
        self.parked_descriptor_scratch.clear();
        self.request_scratch.clear();
        self.response_scratch.clear();
        self.parked_response_scratch.clear();
        self.blob_row_scratch.clear();
        self.scanout_readback_scratch.clear();
        self.trace_fields_scratch.clear();
        self.last_vblank = None;
        self.vblank_paced_count = 0;
        self.publish_vblank_wake();
        self.last_3d_scanout_readback = None;
        self.scanout_3d_flush_count = 0;
        self.scanout_readback_attempt_count = 0;
        self.scanout_readback_count = 0;
        self.scanout_readback_throttled_count = 0;
        self.scanout_readback_bytes = 0;
        self.scanout_readback_nanoseconds = 0;
    }

    pub fn scanout(&self) -> Option<VirtioGpuScanout<'_>> {
        (self.scanout_resource.is_some() || self.blob_scanout.is_some()).then_some(
            VirtioGpuScanout {
                bytes: &self.scanout,
                width: self.width,
                height: self.height,
                stride: self.width * 4,
                fourcc: DRM_FORMAT_XRGB8888,
            },
        )
    }

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
}
