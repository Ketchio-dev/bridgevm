//! Split out of virtio_gpu.rs to keep files under 850 lines.

use super::*;

use std::fmt::Write as _;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crate::{
    fwcfg::GuestMemoryMut,
    msix::{MsixMessage, MsixTable},
    pcie::{
        VIRTIO_GPU_MSIX_PBA_OFFSET, VIRTIO_GPU_MSIX_TABLE_OFFSET, VIRTIO_GPU_MSIX_VECTOR_COUNT,
    },
    virtio_gpu_3d::{
        self, GpuShmMapPort, VirtioGpu3dBackend, VIRTIO_GPU_CMD_CTX_ATTACH_RESOURCE,
        VIRTIO_GPU_CMD_CTX_CREATE, VIRTIO_GPU_CMD_CTX_DESTROY, VIRTIO_GPU_CMD_CTX_DETACH_RESOURCE,
        VIRTIO_GPU_CMD_GET_CAPSET, VIRTIO_GPU_CMD_GET_CAPSET_INFO,
        VIRTIO_GPU_CMD_RESOURCE_CREATE_3D, VIRTIO_GPU_CMD_RESOURCE_CREATE_BLOB,
        VIRTIO_GPU_CMD_RESOURCE_MAP_BLOB, VIRTIO_GPU_CMD_RESOURCE_UNMAP_BLOB,
        VIRTIO_GPU_CMD_SUBMIT_3D, VIRTIO_GPU_CMD_TRANSFER_FROM_HOST_3D,
        VIRTIO_GPU_CMD_TRANSFER_TO_HOST_3D, VIRTIO_GPU_FLAG_FENCE,
    },
    virtio_gpu_trace::{venus_start_trace_enabled, write_json_string},
};

#[derive(Debug)]
pub struct VirtioPciGpu {
    pub(crate) gpu: VirtioGpu,
    pub(crate) msix: MsixTable,
}

impl VirtioPciGpu {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            gpu: VirtioGpu::new(width, height),
            msix: MsixTable::new(VIRTIO_GPU_MSIX_VECTOR_COUNT),
        }
    }

    pub fn with_3d_backend(width: u32, height: u32, backend: Box<dyn VirtioGpu3dBackend>) -> Self {
        Self {
            gpu: VirtioGpu::with_3d_backend(width, height, backend),
            msix: MsixTable::new(VIRTIO_GPU_MSIX_VECTOR_COUNT),
        }
    }

    pub fn with_3d_backend_and_shm_map_port(
        width: u32,
        height: u32,
        backend: Box<dyn VirtioGpu3dBackend>,
        map_port: Box<dyn GpuShmMapPort>,
        shm_window_size: u64,
    ) -> Self {
        let mut gpu = VirtioGpu::with_3d_backend(width, height, backend);
        gpu.set_shm_map_port(map_port, shm_window_size);
        Self {
            gpu,
            msix: MsixTable::new(VIRTIO_GPU_MSIX_VECTOR_COUNT),
        }
    }

    pub fn set_shm_map_port(&mut self, port: Box<dyn GpuShmMapPort>, window_size: u64) {
        self.gpu.set_shm_map_port(port, window_size);
    }

    pub fn set_vblank_interval(&mut self, interval: Duration) {
        self.gpu.set_vblank_interval(interval);
    }

    /// Host-driven scanout resize. Returns true when the geometry changed and a
    /// DISPLAY event + config-change interrupt were armed; the caller flushes
    /// the resulting MSI-X via `drain_pending_msix_into`.
    pub fn request_display_resolution(&mut self, width: u32, height: u32) -> bool {
        self.gpu.request_display_resolution(width, height)
    }

    /// Current reported scanout geometry.
    pub fn display_resolution(&self) -> (u32, u32) {
        (self.gpu.width, self.gpu.height)
    }

    pub fn set_vblank_wake(&mut self, wake: Arc<VblankWakeState>) {
        self.gpu.set_vblank_wake(wake);
    }

    pub fn vblank_wake(&self) -> Option<Arc<VblankWakeState>> {
        self.gpu.vblank_wake()
    }

    pub fn set_3d_scanout_readback_interval(&mut self, interval: Duration) {
        self.gpu.set_3d_scanout_readback_interval(interval);
    }

    pub fn set_3d_scanout_deferred(&mut self, deferred: bool) {
        self.gpu.set_3d_scanout_deferred(deferred);
    }

    pub fn service_deferred_3d_scanout(&mut self) {
        self.gpu.service_deferred_3d_scanout();
    }

    pub fn set_3d_scanout_iosurface(&mut self, enabled: bool, verify: bool) {
        self.gpu.set_3d_scanout_iosurface(enabled, verify);
    }

    pub fn new_from_env() -> Self {
        let (width, height) = parse_resolution_env();
        Self::new(width, height)
    }

    pub fn stats(&self) -> VirtioGpuStats {
        self.gpu.stats()
    }

    pub fn interrupt_line_level(&self) -> bool {
        self.gpu.interrupt_line_level()
    }

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

pub(crate) fn common_cfg_offset(offset: u64) -> Option<u64> {
    (PCI_COMMON_CFG_OFFSET..PCI_COMMON_CFG_OFFSET + PCI_CFG_REGION_SIZE)
        .contains(&offset)
        .then_some(offset - PCI_COMMON_CFG_OFFSET)
}

pub(crate) fn device_cfg_offset(offset: u64) -> Option<u64> {
    (PCI_DEVICE_CFG_OFFSET..PCI_DEVICE_CFG_OFFSET + PCI_CFG_REGION_SIZE)
        .contains(&offset)
        .then_some(offset - PCI_DEVICE_CFG_OFFSET)
}

pub(crate) fn notify_queue_index(offset: u64) -> Option<u16> {
    let rel = offset.checked_sub(PCI_NOTIFY_CFG_OFFSET)?;
    (rel < PCI_CFG_REGION_SIZE).then_some((rel / 4) as u16)
}

pub(crate) fn queue_bit(index: usize) -> Option<u8> {
    (index < u8::BITS as usize).then(|| 1u8 << index)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Descriptor {
    pub(crate) addr: u64,
    pub(crate) len: u32,
    pub(crate) flags: u16,
    pub(crate) next: u16,
}

impl Descriptor {
    pub(crate) fn read(mem: &dyn GuestMemoryMut, gpa: u64) -> Option<Self> {
        let mut bytes = [0u8; DESC_SIZE as usize];
        if !mem.read_into(gpa, &mut bytes) {
            return None;
        }
        Some(Self {
            addr: u64::from_le_bytes(bytes[0..8].try_into().unwrap()),
            len: u32::from_le_bytes(bytes[8..12].try_into().unwrap()),
            flags: u16::from_le_bytes(bytes[12..14].try_into().unwrap()),
            next: u16::from_le_bytes(bytes[14..16].try_into().unwrap()),
        })
    }
}

impl CtrlHdr {
    pub(crate) fn parse(bytes: &[u8]) -> Option<Self> {
        Some(Self {
            typ: read_le_u32(bytes, 0)?,
            flags: read_le_u32(bytes, 4)?,
            fence_id: read_le_u64(bytes, 8)?,
            ctx_id: read_le_u32(bytes, 16)?,
            padding: read_le_u32(bytes, 20)?,
        })
    }

    pub(crate) fn response(self, typ: u32) -> Self {
        Self {
            typ,
            flags: self.flags & VIRTIO_GPU_FLAG_FENCE,
            fence_id: if self.flags & VIRTIO_GPU_FLAG_FENCE != 0 {
                self.fence_id
            } else {
                0
            },
            ctx_id: self.ctx_id,
            padding: self.padding,
        }
    }

    pub(crate) fn ring_idx(self) -> u8 {
        if self.flags & virtio_gpu_3d::VIRTIO_GPU_FLAG_INFO_RING_IDX != 0 {
            (self.padding & 0xff) as u8
        } else {
            0
        }
    }

    pub(crate) fn append_to(self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.typ.to_le_bytes());
        out.extend_from_slice(&self.flags.to_le_bytes());
        out.extend_from_slice(&self.fence_id.to_le_bytes());
        out.extend_from_slice(&self.ctx_id.to_le_bytes());
        out.extend_from_slice(&self.padding.to_le_bytes());
    }
}

pub(crate) fn response_hdr_into(out: &mut Vec<u8>, typ: u32, request: Option<CtrlHdr>) {
    let hdr = request.map_or(
        CtrlHdr {
            typ,
            flags: 0,
            fence_id: 0,
            ctx_id: 0,
            padding: 0,
        },
        |hdr| hdr.response(typ),
    );
    out.clear();
    out.reserve(24);
    hdr.append_to(out);
}

/// Commands whose backend call can leave rendering or transfer work in flight.
/// Every other command is synchronous and may complete its virtqueue fence as
/// soon as the call returns, even when it was routed through the 3D backend.
pub(crate) fn command_requires_backend_fence(typ: u32) -> bool {
    matches!(
        typ,
        VIRTIO_GPU_CMD_TRANSFER_TO_HOST_3D
            | VIRTIO_GPU_CMD_TRANSFER_FROM_HOST_3D
            | VIRTIO_GPU_CMD_SUBMIT_3D
    )
}

pub(crate) fn command_name(typ: u32) -> &'static str {
    match typ {
        VIRTIO_GPU_CMD_GET_DISPLAY_INFO => "GET_DISPLAY_INFO",
        VIRTIO_GPU_CMD_RESOURCE_CREATE_2D => "RESOURCE_CREATE_2D",
        VIRTIO_GPU_CMD_RESOURCE_UNREF => "RESOURCE_UNREF",
        VIRTIO_GPU_CMD_SET_SCANOUT => "SET_SCANOUT",
        VIRTIO_GPU_CMD_RESOURCE_FLUSH => "RESOURCE_FLUSH",
        VIRTIO_GPU_CMD_TRANSFER_TO_HOST_2D => "TRANSFER_TO_HOST_2D",
        VIRTIO_GPU_CMD_RESOURCE_ATTACH_BACKING => "RESOURCE_ATTACH_BACKING",
        VIRTIO_GPU_CMD_RESOURCE_DETACH_BACKING => "RESOURCE_DETACH_BACKING",
        VIRTIO_GPU_CMD_GET_CAPSET_INFO => "GET_CAPSET_INFO",
        VIRTIO_GPU_CMD_GET_CAPSET => "GET_CAPSET",
        VIRTIO_GPU_CMD_GET_EDID => "GET_EDID",
        VIRTIO_GPU_CMD_RESOURCE_CREATE_BLOB => "RESOURCE_CREATE_BLOB",
        VIRTIO_GPU_CMD_SET_SCANOUT_BLOB => "SET_SCANOUT_BLOB",
        VIRTIO_GPU_CMD_CTX_CREATE => "CTX_CREATE",
        VIRTIO_GPU_CMD_CTX_DESTROY => "CTX_DESTROY",
        VIRTIO_GPU_CMD_CTX_ATTACH_RESOURCE => "CTX_ATTACH_RESOURCE",
        VIRTIO_GPU_CMD_CTX_DETACH_RESOURCE => "CTX_DETACH_RESOURCE",
        VIRTIO_GPU_CMD_RESOURCE_CREATE_3D => "RESOURCE_CREATE_3D",
        VIRTIO_GPU_CMD_TRANSFER_TO_HOST_3D => "TRANSFER_TO_HOST_3D",
        VIRTIO_GPU_CMD_TRANSFER_FROM_HOST_3D => "TRANSFER_FROM_HOST_3D",
        VIRTIO_GPU_CMD_SUBMIT_3D => "SUBMIT_3D",
        VIRTIO_GPU_CMD_RESOURCE_MAP_BLOB => "RESOURCE_MAP_BLOB",
        VIRTIO_GPU_CMD_RESOURCE_UNMAP_BLOB => "RESOURCE_UNMAP_BLOB",
        VIRTIO_GPU_CMD_UPDATE_CURSOR => "UPDATE_CURSOR",
        VIRTIO_GPU_CMD_MOVE_CURSOR => "MOVE_CURSOR",
        _ => "UNKNOWN",
    }
}

pub(crate) fn trace_sample(count: u64) -> bool {
    count <= 64 || count % 1024 == 0
}

pub(crate) fn venus_start_trace_msix(what: &str, vector: u16, enabled: bool, masked: bool) {
    if !venus_start_trace_enabled() {
        return;
    }
    static COUNT: AtomicU64 = AtomicU64::new(0);
    let n = COUNT.fetch_add(1, Ordering::Relaxed) + 1;
    if trace_sample(n) {
        println!(
            "venus-start: msix {what} vector={vector} fn_enabled={enabled} fn_masked={masked} n={n}"
        );
    }
}

pub(crate) fn venus_start_trace_msix_queue(what: &str, queue_index: usize, vector: u16) {
    if !venus_start_trace_enabled() {
        return;
    }
    static COUNT: AtomicU64 = AtomicU64::new(0);
    let n = COUNT.fetch_add(1, Ordering::Relaxed) + 1;
    if trace_sample(n) {
        println!("venus-start: msix queue={queue_index} {what} vector={vector} n={n}");
    }
}

/// Stdout mirror of the command trace for the venus KMD start path
/// (`BRIDGEVM_TRACE_VENUS_START=1`). Capset/blob/context lifecycle commands
/// and every error response print unconditionally — those are exactly the
/// accesses DxgkDdiStartDevice makes before the crash — while the high-rate
/// steady-state commands (SUBMIT_3D NOPs, transfers, flushes) are sampled.
pub(crate) fn venus_start_trace_command(request: &[u8], hdr: CtrlHdr, response: &[u8]) {
    if !venus_start_trace_enabled() {
        return;
    }
    let response_type = read_le_u32(response, 0).unwrap_or(0);
    let is_error = response_type >= VIRTIO_GPU_RESP_ERR_UNSPEC;
    let always = is_error
        || matches!(
            hdr.typ,
            VIRTIO_GPU_CMD_GET_CAPSET_INFO
                | VIRTIO_GPU_CMD_GET_CAPSET
                | VIRTIO_GPU_CMD_RESOURCE_CREATE_BLOB
                | VIRTIO_GPU_CMD_RESOURCE_MAP_BLOB
                | VIRTIO_GPU_CMD_RESOURCE_UNMAP_BLOB
                | VIRTIO_GPU_CMD_CTX_CREATE
                | VIRTIO_GPU_CMD_CTX_DESTROY
        );
    if !always {
        static COUNT: AtomicU64 = AtomicU64::new(0);
        let n = COUNT.fetch_add(1, Ordering::Relaxed) + 1;
        if !trace_sample(n) {
            return;
        }
    }
    let mut line = format!(
        "venus-start: cmd {} typ={:#x} ctx={} flags={:#x} -> {} typ={:#x}",
        command_name(hdr.typ),
        hdr.typ,
        hdr.ctx_id,
        hdr.flags,
        response_name(response_type),
        response_type
    );
    match hdr.typ {
        VIRTIO_GPU_CMD_GET_CAPSET_INFO => {
            let _ = write!(
                line,
                " capset_index={}",
                read_le_u32(request, 24).unwrap_or(u32::MAX)
            );
            if response_type == virtio_gpu_3d::VIRTIO_GPU_RESP_OK_CAPSET_INFO {
                let _ = write!(
                    line,
                    " capset_id={} max_version={} max_size={}",
                    read_le_u32(response, 24).unwrap_or(0),
                    read_le_u32(response, 28).unwrap_or(0),
                    read_le_u32(response, 32).unwrap_or(0)
                );
            }
        }
        VIRTIO_GPU_CMD_GET_CAPSET => {
            let _ = write!(
                line,
                " capset_id={} version={} response_bytes={}",
                read_le_u32(request, 24).unwrap_or(0),
                read_le_u32(request, 28).unwrap_or(0),
                response.len().saturating_sub(24)
            );
        }
        VIRTIO_GPU_CMD_RESOURCE_CREATE_BLOB => {
            let _ = write!(
                line,
                " resource_id={} blob_mem={} blob_flags={:#x} blob_id={} size={} nr_entries={}",
                read_le_u32(request, 24).unwrap_or(0),
                read_le_u32(request, 28).unwrap_or(0),
                read_le_u32(request, 32).unwrap_or(0),
                read_le_u64(request, 40).unwrap_or(0),
                read_le_u64(request, 48).unwrap_or(0),
                read_le_u32(request, 36).unwrap_or(0)
            );
        }
        VIRTIO_GPU_CMD_RESOURCE_MAP_BLOB => {
            let _ = write!(
                line,
                " resource_id={} shm_offset={:#x}",
                read_le_u32(request, 24).unwrap_or(0),
                read_le_u64(request, 32).unwrap_or(0)
            );
            if response_type == virtio_gpu_3d::VIRTIO_GPU_RESP_OK_MAP_INFO {
                let _ = write!(
                    line,
                    " map_info={:#x}",
                    read_le_u32(response, 24).unwrap_or(0)
                );
            }
        }
        VIRTIO_GPU_CMD_RESOURCE_UNMAP_BLOB => {
            let _ = write!(
                line,
                " resource_id={}",
                read_le_u32(request, 24).unwrap_or(0)
            );
        }
        _ => {}
    }
    println!("{line}");
}

pub(crate) fn response_name(typ: u32) -> &'static str {
    match typ {
        VIRTIO_GPU_RESP_OK_NODATA => "OK_NODATA",
        VIRTIO_GPU_RESP_OK_DISPLAY_INFO => "OK_DISPLAY_INFO",
        virtio_gpu_3d::VIRTIO_GPU_RESP_OK_CAPSET_INFO => "OK_CAPSET_INFO",
        virtio_gpu_3d::VIRTIO_GPU_RESP_OK_CAPSET => "OK_CAPSET",
        VIRTIO_GPU_RESP_OK_EDID => "OK_EDID",
        virtio_gpu_3d::VIRTIO_GPU_RESP_OK_MAP_INFO => "OK_MAP_INFO",
        VIRTIO_GPU_RESP_ERR_UNSPEC => "ERR_UNSPEC",
        virtio_gpu_3d::VIRTIO_GPU_RESP_ERR_OUT_OF_MEMORY => "ERR_OUT_OF_MEMORY",
        virtio_gpu_3d::VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER => "ERR_INVALID_PARAMETER",
        _ => "UNKNOWN",
    }
}

pub(crate) fn write_trace_command_details(out: &mut String, request: &[u8], hdr: CtrlHdr) {
    match hdr.typ {
        VIRTIO_GPU_CMD_RESOURCE_CREATE_2D => {
            let _ = write!(
                out,
                ",\"resource_id\":{},\"format\":{},\"width\":{},\"height\":{}",
                read_le_u32(request, 24).unwrap_or(0),
                read_le_u32(request, 28).unwrap_or(0),
                read_le_u32(request, 32).unwrap_or(0),
                read_le_u32(request, 36).unwrap_or(0)
            );
        }
        VIRTIO_GPU_CMD_RESOURCE_UNREF
        | VIRTIO_GPU_CMD_RESOURCE_DETACH_BACKING
        | VIRTIO_GPU_CMD_RESOURCE_UNMAP_BLOB => {
            let _ = write!(
                out,
                ",\"resource_id\":{}",
                read_le_u32(request, 24).unwrap_or(0)
            );
        }
        VIRTIO_GPU_CMD_RESOURCE_ATTACH_BACKING => {
            let _ = write!(
                out,
                ",\"resource_id\":{},\"nr_entries\":{}",
                read_le_u32(request, 24).unwrap_or(0),
                read_le_u32(request, 28).unwrap_or(0)
            );
        }
        VIRTIO_GPU_CMD_SET_SCANOUT => {
            let rect = read_rect(request, 24).unwrap_or(Rect {
                x: 0,
                y: 0,
                width: 0,
                height: 0,
            });
            let _ = write!(
                out,
                ",\"scanout_id\":{},\"resource_id\":{},\"rect_x\":{},\"rect_y\":{},\"rect_w\":{},\"rect_h\":{}",
                read_le_u32(request, 40).unwrap_or(u32::MAX),
                read_le_u32(request, 44).unwrap_or(0),
                rect.x,
                rect.y,
                rect.width,
                rect.height
            );
        }
        VIRTIO_GPU_CMD_RESOURCE_FLUSH => {
            let rect = read_rect(request, 24).unwrap_or(Rect {
                x: 0,
                y: 0,
                width: 0,
                height: 0,
            });
            let _ = write!(
                out,
                ",\"resource_id\":{},\"rect_x\":{},\"rect_y\":{},\"rect_w\":{},\"rect_h\":{}",
                read_le_u32(request, 40).unwrap_or(0),
                rect.x,
                rect.y,
                rect.width,
                rect.height
            );
        }
        VIRTIO_GPU_CMD_TRANSFER_TO_HOST_2D => {
            let rect = read_rect(request, 24).unwrap_or(Rect {
                x: 0,
                y: 0,
                width: 0,
                height: 0,
            });
            let _ = write!(
                out,
                ",\"resource_id\":{},\"offset\":{},\"rect_x\":{},\"rect_y\":{},\"rect_w\":{},\"rect_h\":{}",
                read_le_u32(request, 48).unwrap_or(0),
                read_le_u64(request, 40).unwrap_or(0),
                rect.x,
                rect.y,
                rect.width,
                rect.height
            );
        }
        VIRTIO_GPU_CMD_GET_CAPSET_INFO => {
            let _ = write!(
                out,
                ",\"capset_index\":{}",
                read_le_u32(request, 24).unwrap_or(u32::MAX)
            );
        }
        VIRTIO_GPU_CMD_GET_CAPSET => {
            let _ = write!(
                out,
                ",\"capset_id\":{},\"capset_version\":{}",
                read_le_u32(request, 24).unwrap_or(0),
                read_le_u32(request, 28).unwrap_or(0)
            );
        }
        VIRTIO_GPU_CMD_RESOURCE_CREATE_BLOB => {
            let _ = write!(
                out,
                ",\"resource_id\":{},\"blob_mem\":{},\"blob_flags\":{},\"nr_entries\":{},\"blob_id\":{},\"blob_size\":{}",
                read_le_u32(request, 24).unwrap_or(0),
                read_le_u32(request, 28).unwrap_or(0),
                read_le_u32(request, 32).unwrap_or(0),
                read_le_u32(request, 36).unwrap_or(0),
                read_le_u64(request, 40).unwrap_or(0),
                read_le_u64(request, 48).unwrap_or(0)
            );
        }
        VIRTIO_GPU_CMD_SET_SCANOUT_BLOB => {
            let rect = read_rect(request, 24).unwrap_or(Rect {
                x: 0,
                y: 0,
                width: 0,
                height: 0,
            });
            let _ = write!(
                out,
                ",\"scanout_id\":{},\"resource_id\":{},\"width\":{},\"height\":{},\"format\":{},\"stride0\":{},\"offset0\":{},\"rect_x\":{},\"rect_y\":{},\"rect_w\":{},\"rect_h\":{}",
                read_le_u32(request, 40).unwrap_or(u32::MAX),
                read_le_u32(request, 44).unwrap_or(0),
                read_le_u32(request, 48).unwrap_or(0),
                read_le_u32(request, 52).unwrap_or(0),
                read_le_u32(request, 56).unwrap_or(0),
                read_le_u32(request, 64).unwrap_or(0),
                read_le_u32(request, 80).unwrap_or(0),
                rect.x,
                rect.y,
                rect.width,
                rect.height
            );
        }
        VIRTIO_GPU_CMD_CTX_CREATE => {
            let nlen = read_le_u32(request, 24).unwrap_or(0).min(64) as usize;
            let name_end = 32usize.saturating_add(nlen).min(request.len());
            let _ = write!(
                out,
                ",\"context_init\":{},\"name_len\":{},\"debug_name\":",
                read_le_u32(request, 28).unwrap_or(0),
                nlen
            );
            let name = request
                .get(32..name_end)
                .map(String::from_utf8_lossy)
                .unwrap_or_default();
            write_json_string(out, name.as_ref());
        }
        VIRTIO_GPU_CMD_CTX_ATTACH_RESOURCE | VIRTIO_GPU_CMD_CTX_DETACH_RESOURCE => {
            let _ = write!(
                out,
                ",\"resource_id\":{}",
                read_le_u32(request, 24).unwrap_or(0)
            );
        }
        VIRTIO_GPU_CMD_RESOURCE_CREATE_3D => {
            let _ = write!(
                out,
                ",\"resource_id\":{},\"target\":{},\"format\":{},\"bind\":{},\"width\":{},\"height\":{},\"depth\":{},\"array_size\":{},\"last_level\":{},\"nr_samples\":{},\"resource_flags\":{}",
                read_le_u32(request, 24).unwrap_or(0),
                read_le_u32(request, 28).unwrap_or(0),
                read_le_u32(request, 32).unwrap_or(0),
                read_le_u32(request, 36).unwrap_or(0),
                read_le_u32(request, 40).unwrap_or(0),
                read_le_u32(request, 44).unwrap_or(0),
                read_le_u32(request, 48).unwrap_or(0),
                read_le_u32(request, 52).unwrap_or(0),
                read_le_u32(request, 56).unwrap_or(0),
                read_le_u32(request, 60).unwrap_or(0),
                read_le_u32(request, 64).unwrap_or(0)
            );
        }
        VIRTIO_GPU_CMD_TRANSFER_TO_HOST_3D | VIRTIO_GPU_CMD_TRANSFER_FROM_HOST_3D => {
            let _ = write!(
                out,
                ",\"resource_id\":{},\"level\":{},\"stride\":{},\"layer_stride\":{},\"transfer_offset\":{},\"box_x\":{},\"box_y\":{},\"box_z\":{},\"box_w\":{},\"box_h\":{},\"box_d\":{}",
                read_le_u32(request, 56).unwrap_or(0),
                read_le_u32(request, 60).unwrap_or(0),
                read_le_u32(request, 64).unwrap_or(0),
                read_le_u32(request, 68).unwrap_or(0),
                read_le_u64(request, 48).unwrap_or(0),
                read_le_u32(request, 24).unwrap_or(0),
                read_le_u32(request, 28).unwrap_or(0),
                read_le_u32(request, 32).unwrap_or(0),
                read_le_u32(request, 36).unwrap_or(0),
                read_le_u32(request, 40).unwrap_or(0),
                read_le_u32(request, 44).unwrap_or(0)
            );
        }
        VIRTIO_GPU_CMD_SUBMIT_3D => {
            let size = read_le_u32(request, 24).unwrap_or(0) as usize;
            let payload_start = 32usize.min(request.len());
            let payload_end = payload_start.saturating_add(size).min(request.len());
            let payload = request.get(payload_start..payload_end).unwrap_or(&[]);
            let _ = write!(
                out,
                ",\"submit_size\":{},\"submit_dwords\":{},\"submit_prefix_hex\":",
                size,
                size.div_ceil(4)
            );
            write_hex_prefix_json(out, payload, submit_trace_prefix_len());
        }
        VIRTIO_GPU_CMD_RESOURCE_MAP_BLOB => {
            let _ = write!(
                out,
                ",\"resource_id\":{},\"shm_offset\":{}",
                read_le_u32(request, 24).unwrap_or(0),
                read_le_u64(request, 32).unwrap_or(0)
            );
        }
        _ => {}
    }
}
