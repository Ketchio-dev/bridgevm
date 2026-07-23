//! Continuation of the `virtiopcigpu` impl block, split for the 1000-line rule.

use super::*;
use crate::fwcfg::GuestMemoryMut;
use crate::msix::MsixMessage;
use crate::msix::MsixTable;
use crate::pcie::VIRTIO_GPU_MSIX_PBA_OFFSET;
use crate::pcie::VIRTIO_GPU_MSIX_TABLE_OFFSET;
use crate::pcie::VIRTIO_GPU_MSIX_VECTOR_COUNT;

impl VirtioPciGpu {
    pub fn reset_runtime_state(&mut self) {
        self.gpu.reset_runtime_state();
        self.msix = MsixTable::new(VIRTIO_GPU_MSIX_VECTOR_COUNT);
    }

    pub fn drain_host_vblank(&mut self, mem: &mut dyn GuestMemoryMut) {
        self.gpu.drain_host_vblank(mem);
    }

    pub fn drain_completed_fences(&mut self, mem: &mut dyn GuestMemoryMut) {
        self.gpu.drain_completed_fences(mem);
    }

    pub fn scanout(&self) -> Option<VirtioGpuScanout<'_>> {
        self.gpu.scanout()
    }

    pub fn access(
        &mut self,
        offset: u64,
        op: VirtioPciGpuOp,
        mem: &mut dyn GuestMemoryMut,
    ) -> VirtioGpuResult {
        let is_write = matches!(op, VirtioPciGpuOp::Write { .. });
        if let Some(common_offset) = common_cfg_offset(offset) {
            return match op {
                VirtioPciGpuOp::Read { size } => {
                    self.gpu.access_common(common_offset, false, size, 0, mem)
                }
                VirtioPciGpuOp::Write { size, value } => {
                    let result = self
                        .gpu
                        .access_common(common_offset, true, size, value, mem);
                    self.gpu.drain_completed_fences(mem);
                    result
                }
            };
        }
        if let Some(device_offset) = device_cfg_offset(offset) {
            return match op {
                VirtioPciGpuOp::Read { size } => {
                    VirtioGpuResult::ReadValue(self.gpu.config_read(device_offset, size))
                }
                VirtioPciGpuOp::Write { size, value } => {
                    self.gpu.config_write(device_offset, size, value);
                    VirtioGpuResult::WriteAck
                }
            };
        }
        if let Some(queue_index) = notify_queue_index(offset) {
            return match op {
                VirtioPciGpuOp::Read { .. } => VirtioGpuResult::ReadValue(0),
                VirtioPciGpuOp::Write { value, .. } => {
                    let queue = if offset == PCI_NOTIFY_CFG_OFFSET {
                        value as u16
                    } else {
                        queue_index
                    };
                    self.gpu.notify_queue(queue, mem);
                    VirtioGpuResult::WriteAck
                }
            };
        }
        if offset == PCI_ISR_CFG_OFFSET {
            return match op {
                VirtioPciGpuOp::Read { size } => VirtioGpuResult::ReadValue(mask_to_size(
                    u64::from(self.gpu.interrupt_status),
                    size,
                )),
                VirtioPciGpuOp::Write { value, .. } => {
                    self.gpu.interrupt_status &= !(value as u32);
                    VirtioGpuResult::WriteAck
                }
            };
        }
        match (op, is_write) {
            (VirtioPciGpuOp::Read { .. }, _) => VirtioGpuResult::ReadValue(0),
            (VirtioPciGpuOp::Write { .. }, _) => VirtioGpuResult::WriteAck,
        }
    }

    pub fn msix_bar_access(&mut self, offset: u64, op: VirtioPciGpuOp) -> VirtioGpuResult {
        if let Some(table_offset) = self.msix_table_offset(offset) {
            return match op {
                VirtioPciGpuOp::Read { size } => {
                    VirtioGpuResult::ReadValue(self.msix.table_read(table_offset, size))
                }
                VirtioPciGpuOp::Write { size, value } => {
                    self.msix.table_write(table_offset, size, value);
                    VirtioGpuResult::WriteAck
                }
            };
        }
        if let Some(pba_offset) = self.msix_pba_offset(offset) {
            return match op {
                VirtioPciGpuOp::Read { size } => {
                    VirtioGpuResult::ReadValue(self.msix.pba_read(pba_offset, size))
                }
                VirtioPciGpuOp::Write { size, value } => {
                    self.msix.pba_write(pba_offset, size, value);
                    VirtioGpuResult::WriteAck
                }
            };
        }
        match op {
            VirtioPciGpuOp::Read { .. } => VirtioGpuResult::ReadValue(0),
            VirtioPciGpuOp::Write { .. } => VirtioGpuResult::WriteAck,
        }
    }

    pub fn raise_pending_msix(
        &mut self,
        function_enabled: bool,
        function_masked: bool,
    ) -> Vec<MsixMessage> {
        let mut messages = Vec::new();
        self.raise_pending_msix_into(function_enabled, function_masked, &mut messages);
        messages
    }

    pub fn raise_pending_msix_into(
        &mut self,
        function_enabled: bool,
        function_masked: bool,
        out: &mut Vec<MsixMessage>,
    ) {
        if self.gpu.pending_config_change {
            let vector = self.gpu.config_msix_vector;
            if vector != VIRTIO_MSI_NO_VECTOR {
                if let Some(message) = self.msix.raise(vector, function_enabled, function_masked) {
                    self.gpu.pending_config_change = false;
                    venus_start_trace_msix(
                        "config raised",
                        vector,
                        function_enabled,
                        function_masked,
                    );
                    out.push(message);
                } else {
                    venus_start_trace_msix(
                        "config held",
                        vector,
                        function_enabled,
                        function_masked,
                    );
                }
            } else {
                // No config vector programmed (INTx path): the ISR config bit
                // is already set; nothing MSI-X to raise.
                self.gpu.pending_config_change = false;
            }
        }
        let mut pending = self.gpu.pending_msix_queue_bits;
        while pending != 0 {
            let queue_index = pending.trailing_zeros() as usize;
            let vector = self.gpu.queues[queue_index].msix_vector;
            if vector == VIRTIO_MSI_NO_VECTOR {
                venus_start_trace_msix_queue("no-vector (ISR path)", queue_index, vector);
                pending &= !(1u8 << queue_index);
                continue;
            }
            if let Some(message) = self.msix.raise(vector, function_enabled, function_masked) {
                self.gpu.queues[queue_index].pending_msix = false;
                self.gpu.pending_msix_queue_bits &= !(1u8 << queue_index);
                venus_start_trace_msix_queue("raised", queue_index, vector);
                out.push(message);
            } else {
                venus_start_trace_msix_queue("held (disabled/masked)", queue_index, vector);
            }
            pending &= !(1u8 << queue_index);
        }
    }

    pub fn drain_pending_msix(
        &mut self,
        function_enabled: bool,
        function_masked: bool,
    ) -> Vec<MsixMessage> {
        let mut messages = Vec::new();
        self.drain_pending_msix_into(function_enabled, function_masked, &mut messages);
        messages
    }

    pub fn drain_pending_msix_into(
        &mut self,
        function_enabled: bool,
        function_masked: bool,
        out: &mut Vec<MsixMessage>,
    ) {
        let start = out.len();
        self.msix
            .drain_pending_into(function_enabled, function_masked, out);
        for message in &out[start..] {
            self.clear_pending_queue_for_vector(message.vector);
        }
        self.raise_pending_msix_into(function_enabled, function_masked, out);
    }

    pub(crate) fn clear_pending_queue_for_vector(&mut self, vector: u16) {
        for (queue_index, queue) in self.gpu.queues.iter_mut().enumerate() {
            if queue.msix_vector == vector {
                queue.pending_msix = false;
                if let Some(bit) = queue_bit(queue_index) {
                    self.gpu.pending_msix_queue_bits &= !bit;
                }
            }
        }
    }

    pub(crate) fn msix_table_offset(&self, offset: u64) -> Option<u64> {
        let rel = offset.checked_sub(u64::from(VIRTIO_GPU_MSIX_TABLE_OFFSET))?;
        (rel < self.msix.table_byte_len()).then_some(rel)
    }

    pub(crate) fn msix_pba_offset(&self, offset: u64) -> Option<u64> {
        let rel = offset.checked_sub(u64::from(VIRTIO_GPU_MSIX_PBA_OFFSET))?;
        (rel < self.msix.pba_byte_len()).then_some(rel)
    }

    pub fn snapshot_state(&self) -> Vec<u8> {
        let gpu = &self.gpu;
        let mut out = crate::checkpoint::StateWriter::new();
        out.write_u32(1);
        out.write_u32(gpu.width);
        out.write_u32(gpu.height);
        out.write_u32(gpu.device_features_sel);
        out.write_u32(gpu.driver_features_sel);
        out.write_u32(gpu.driver_features[0]);
        out.write_u32(gpu.driver_features[1]);
        out.write_u16(gpu.config_msix_vector);
        out.write_u16(0);
        out.write_u32(gpu.queue_sel);
        out.write_u8(gpu.pending_msix_queue_bits);
        out.write_u8(0);
        out.write_u16(0);
        out.write_u32(gpu.status);
        out.write_u32(gpu.interrupt_status);
        out.write_u32(gpu.events_clear);

        for queue in &gpu.queues {
            out.write_u16(queue.size);
            out.write_bool(queue.ready);
            out.write_bool(queue.pending_msix);
            out.write_u64(queue.desc);
            out.write_u64(queue.driver);
            out.write_u64(queue.device);
            out.write_u16(queue.msix_vector);
            out.write_u16(queue.notify_off);
            out.write_u16(queue.last_avail_idx);
            out.write_u16(0);
        }

        out.write_u32(gpu.resources.len() as u32);
        for (&resource_id, resource) in &gpu.resources {
            out.write_u32(resource_id);
            out.write_u32(resource.format);
            out.write_u32(resource.width);
            out.write_u32(resource.height);
            out.write_blob(&resource.host_pixels);
            out.write_u32(resource.backing.len() as u32);
            for backing in &resource.backing {
                out.write_u64(backing.addr);
                out.write_u32(backing.len);
                out.write_u32(0);
            }
        }

        out.write_bool(gpu.scanout_resource.is_some());
        if let Some(resource_id) = gpu.scanout_resource {
            out.write_u32(resource_id);
        }
        out.write_blob(&gpu.scanout);
        out.write_blob(&self.msix.snapshot_state());
        out.into_inner()
    }

    pub fn restore_state(&mut self, data: &[u8]) {
        let mut input = crate::checkpoint::StateReader::new(data);
        assert_eq!(
            input.read_u32(),
            1,
            "unsupported virtio-gpu snapshot version"
        );

        let width = input.read_u32();
        let height = input.read_u32();
        assert_eq!(
            (width, height),
            (self.gpu.width, self.gpu.height),
            "virtio-gpu resolution mismatch on restore"
        );

        self.gpu.device_features_sel = input.read_u32();
        self.gpu.driver_features_sel = input.read_u32();
        self.gpu.driver_features = [input.read_u32(), input.read_u32()];
        self.gpu.config_msix_vector = input.read_u16();
        assert_eq!(input.read_u16(), 0, "invalid virtio-gpu snapshot");
        self.gpu.queue_sel = input.read_u32();
        self.gpu.pending_msix_queue_bits = input.read_u8();
        assert_eq!(input.read_u8(), 0, "invalid virtio-gpu snapshot");
        assert_eq!(input.read_u16(), 0, "invalid virtio-gpu snapshot");
        self.gpu.status = input.read_u32();
        self.gpu.interrupt_status = input.read_u32();
        self.gpu.events_clear = input.read_u32();

        for queue in &mut self.gpu.queues {
            queue.size = input.read_u16();
            queue.ready = input.read_bool();
            queue.pending_msix = input.read_bool();
            queue.desc = input.read_u64();
            queue.driver = input.read_u64();
            queue.device = input.read_u64();
            queue.msix_vector = input.read_u16();
            queue.notify_off = input.read_u16();
            queue.last_avail_idx = input.read_u16();
            assert_eq!(input.read_u16(), 0, "invalid virtio-gpu queue snapshot");
        }

        self.gpu.resources.clear();
        let resource_count = input.read_u32() as usize;
        for _ in 0..resource_count {
            let resource_id = input.read_u32();
            let format = input.read_u32();
            let width = input.read_u32();
            let height = input.read_u32();
            let host_pixels = input.read_blob();

            let backing_count = input.read_u32() as usize;
            let mut backing = Vec::with_capacity(backing_count);
            for _ in 0..backing_count {
                backing.push(BackingEntry {
                    addr: input.read_u64(),
                    len: input.read_u32(),
                });
                assert_eq!(input.read_u32(), 0, "invalid GPU backing snapshot");
            }

            self.gpu.resources.insert(
                resource_id,
                GpuResource {
                    format,
                    width,
                    height,
                    host_pixels,
                    backing,
                },
            );
        }

        self.gpu.scanout_resource = if input.read_bool() {
            Some(input.read_u32())
        } else {
            None
        };
        if let Some(resource_id) = self.gpu.scanout_resource {
            if !self.gpu.resources.contains_key(&resource_id) {
                // The active desktop scanout is normally backed by a 3D/blob resource
                // whose pixels live in the (non-serializable) virglrenderer host
                // context, so it is absent from the restored 2D resource map. Drop the
                // dangling reference rather than panicking; on resume the guest WDDM
                // driver detects the lost adapter, TDR-resets, and re-establishes the
                // scanout (the documented "3D contexts lost on restore" behavior).
                self.gpu.scanout_resource = None;
            }
        }
        self.gpu.scanout = input.read_blob();

        self.gpu.unbind_blob_scanout();
        self.gpu.three_d.reset();
        self.gpu.pending_fenced.clear();
        self.gpu.pending_vblank.clear();
        self.gpu.completed_fences_scratch.clear();
        self.gpu.descriptor_scratch.clear();
        self.gpu.parked_descriptor_scratch.clear();
        self.gpu.request_scratch.clear();
        self.gpu.response_scratch.clear();
        self.gpu.parked_response_scratch.clear();
        self.gpu.blob_row_scratch.clear();
        self.gpu.last_vblank = None;
        self.gpu.last_3d_scanout_readback = None;
        self.gpu.publish_vblank_wake();
        self.gpu.publish_scanout_fb_unconditionally();

        self.msix.restore_state(&input.read_blob());
        input.finish();
    }
}
