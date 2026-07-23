//! Virtqueue transport: queue state, descriptor walking, gather/scatter, used ring, scratch pooling.

use super::*;
use crate::fwcfg::GuestMemoryMut;
use crate::virtio_gpu_3d::CompletedFence;
use crate::virtio_gpu_3d::VIRTIO_GPU_CMD_SUBMIT_3D;
use crate::virtio_gpu_3d::VIRTIO_GPU_FLAG_FENCE;
use std::fmt::Write as _;
use std::time::Instant;

pub(crate) const QUEUE_CONTROL: usize = 0;

pub(crate) const QUEUE_CURSOR: usize = 1;

pub(crate) const QUEUE_COUNT: usize = 2;

pub(crate) const PARKED_RESPONSE_BUFFER_POOL_LIMIT: usize = 4;

pub(crate) const QUEUE_MAX: u16 = 64;

pub(crate) const DESC_SIZE: u64 = 16;

pub(crate) const DESC_F_NEXT: u16 = 1;

pub(crate) const DESC_F_WRITE: u16 = 2;

pub(crate) const MAX_GPU_REQUEST_LEN: usize = 64 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct VirtioGpuQueue {
    pub(crate) size: u16,
    pub(crate) ready: bool,
    pub(crate) desc: u64,
    pub(crate) driver: u64,
    pub(crate) device: u64,
    pub(crate) msix_vector: u16,
    pub(crate) notify_off: u16,
    pub(crate) last_avail_idx: u16,
    pub(crate) pending_msix: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ChainCompletion {
    Immediate(u32),
    Parked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Descriptor {
    pub(crate) addr: u64,
    pub(crate) len: u32,
    pub(crate) flags: u16,
    pub(crate) next: u16,
}

impl VirtioGpuQueue {
    pub(crate) const fn new(notify_off: u16) -> Self {
        Self {
            size: 0,
            ready: false,
            desc: 0,
            driver: 0,
            device: 0,
            msix_vector: VIRTIO_MSI_NO_VECTOR,
            notify_off,
            last_avail_idx: 0,
            pending_msix: false,
        }
    }

    pub(crate) fn reset(&mut self) {
        let notify_off = self.notify_off;
        *self = Self::new(notify_off);
    }

    /// Queue size the device must actually run at. The virtio driver may enable
    /// a queue without ever writing COMMON_QUEUE_SIZE, in which case the queue
    /// operates at the advertised maximum (`QUEUE_MAX`) rather than the reset
    /// value of 0. Reads of COMMON_QUEUE_SIZE already report this effective
    /// value, so descriptor processing must agree with it.
    pub(crate) fn effective_size(&self) -> u16 {
        if self.size == 0 {
            QUEUE_MAX
        } else {
            self.size
        }
    }
}

impl VirtioGpu {
    pub(crate) fn selected_queue(&self) -> Option<VirtioGpuQueue> {
        self.queues.get(self.queue_sel as usize).copied()
    }

    pub(crate) fn write_selected_queue(&mut self, write: impl FnOnce(&mut VirtioGpuQueue)) {
        if let Some(queue) = self.queues.get_mut(self.queue_sel as usize) {
            write(queue);
        }
    }

    pub(crate) fn notify_queue(&mut self, queue_index: u16, mem: &mut dyn GuestMemoryMut) {
        self.trace_queue_notify(queue_index);
        match usize::from(queue_index) {
            QUEUE_CONTROL => self.process_control_queue(mem),
            QUEUE_CURSOR => self.process_cursor_queue(mem),
            _ => {}
        }
    }

    pub(crate) fn process_control_queue(&mut self, mem: &mut dyn GuestMemoryMut) {
        self.process_queue(QUEUE_CONTROL, mem, true);
    }

    pub(crate) fn process_cursor_queue(&mut self, mem: &mut dyn GuestMemoryMut) {
        self.process_queue(QUEUE_CURSOR, mem, false);
    }

    pub(crate) fn process_queue(
        &mut self,
        queue_index: usize,
        mem: &mut dyn GuestMemoryMut,
        control: bool,
    ) {
        let queue = self.queues[queue_index];
        if !queue.ready || queue.desc == 0 || queue.driver == 0 {
            return;
        }
        // A driver may enable the queue without writing COMMON_QUEUE_SIZE (EDK2's
        // VirtioGpuDxe reads the advertised size but never writes it back). Gating
        // on the raw stored size left `queue.size == 0`, so the control queue was
        // never drained: firmware submitted GET_DISPLAY_INFO, polled the used ring
        // forever, and the guest hung before reaching the boot manager.
        let queue_size = queue.effective_size();
        let Some(avail_idx) = read_u16(mem, queue.driver + 2) else {
            return;
        };
        while self.queues[queue_index].last_avail_idx != avail_idx {
            let last_avail_idx = self.queues[queue_index].last_avail_idx;
            let ring_off = 4 + u64::from(last_avail_idx % queue_size) * 2;
            let Some(head) = read_u16(mem, queue.driver + ring_off) else {
                return;
            };
            let completion = self.process_chain(mem, &queue, queue_index, head, control);
            self.queues[queue_index].last_avail_idx = last_avail_idx.wrapping_add(1);
            if let ChainCompletion::Immediate(used_len) = completion {
                Self::write_used(mem, &queue, head, used_len);
                self.mark_queue_interrupt(queue_index);
            }
        }
        self.drain_completed_fences_after_queue(mem);
    }

    pub(crate) fn process_chain(
        &mut self,
        mem: &mut dyn GuestMemoryMut,
        queue: &VirtioGpuQueue,
        queue_index: usize,
        head: u16,
        control: bool,
    ) -> ChainCompletion {
        let mut descs = self.take_descriptor_scratch();
        if !Self::descriptor_chain_into(mem, queue, head, &mut descs) {
            self.descriptor_scratch = descs;
            return ChainCompletion::Immediate(0);
        }
        let mut request = std::mem::take(&mut self.request_scratch);
        Self::gather_readable_into(mem, &descs, &mut request);
        let mut response = self.take_response_scratch();
        response.clear();
        let handle_started = Instant::now();
        if control {
            self.handle_control_request_into(mem, &request, &mut response);
        } else {
            self.handle_cursor_request_into(&request, &mut response);
        }
        let handle_ns = handle_started
            .elapsed()
            .as_nanos()
            .min(u128::from(u64::MAX)) as u64;
        let Some(hdr) = CtrlHdr::parse(&request) else {
            let request_len = request.len();
            let response_len = response.len();
            self.record_trace_fields("command_parse_error", |fields| {
                let _ = write!(
                    fields,
                    ",\"queue\":{},\"head\":{},\"request_len\":{},\"response_len\":{}",
                    queue_index, head, request_len, response_len
                );
            });
            let used_len = Self::scatter_write(mem, &descs, &response);
            self.recycle_queue_scratch(descs, request, response);
            return ChainCompletion::Immediate(used_len);
        };
        self.trace_command(
            queue_index,
            head,
            control,
            &descs,
            &request,
            hdr,
            &response,
            handle_ns,
        );
        // viogpu3d uses an empty context-0 SUBMIT_3D as its control-queue
        // synchronization NOP. Its used-ring completion drives the guest's
        // DXGK CRTC_VSYNC notification, so park that completion when host
        // vblank pacing is enabled.
        if control
            && !self.vblank_interval.is_zero()
            && hdr.typ == VIRTIO_GPU_CMD_SUBMIT_3D
            && hdr.ctx_id == 0
            && read_le_u32(&request, 24) == Some(0)
            && read_le_u32(&response, 0) == Some(VIRTIO_GPU_RESP_OK_NODATA)
        {
            self.pending_vblank.push(PendingVblankResponse {
                queue_index,
                queue: *queue,
                head,
                descs,
                response,
            });
            self.publish_vblank_wake();
            request.clear();
            self.request_scratch = request;
            self.response_scratch = Vec::new();
            return ChainCompletion::Parked;
        }
        // Defer only commands that can leave GPU work in flight. Resource/context
        // lifecycle, capset, and map operations are complete when their backend
        // call returns, so their fence is already satisfied. In particular,
        // RESOURCE_CREATE_3D normally carries ctx_id=0; trying to create a context
        // fence for it is invalid in virglrenderer and floods the host log.
        if control
            && hdr.flags & VIRTIO_GPU_FLAG_FENCE != 0
            && hdr.ctx_id != 0
            && self.three_d.has_backend()
            // The WDDM KMD uses numeric context ids for its display-copy path
            // before any UMD VIOGPU_CTX_INIT/CTX_CREATE. Those commands are
            // handled synchronously by the local scanout path and have no
            // renderer context on which virglrenderer could create a fence.
            && self.three_d.has_live_context(hdr.ctx_id)
            && command_requires_backend_fence(hdr.typ)
        {
            let fence = CompletedFence {
                ctx_id: hdr.ctx_id,
                ring_idx: hdr.ring_idx(),
                fence_id: hdr.fence_id,
            };
            if self.three_d.create_fence(fence) {
                self.trace_fence_create(fence, true, "parked");
                self.pending_fenced.push(PendingFencedResponse {
                    queue_index,
                    queue: *queue,
                    head,
                    descs,
                    response,
                    fence,
                });
                request.clear();
                self.request_scratch = request;
                self.response_scratch = Vec::new();
                return ChainCompletion::Parked;
            }
            // If virgl rejects the requested timeline, the command response is
            // still delivered; there is no backend fence that can retire it.
            self.trace_fence_create(fence, false, "immediate");
        }
        let used_len = Self::scatter_write(mem, &descs, &response);
        self.recycle_queue_scratch(descs, request, response);
        ChainCompletion::Immediate(used_len)
    }

    pub(crate) fn recycle_queue_scratch(
        &mut self,
        mut descs: Vec<Descriptor>,
        mut request: Vec<u8>,
        mut response: Vec<u8>,
    ) {
        descs.clear();
        request.clear();
        response.clear();
        self.descriptor_scratch = descs;
        self.request_scratch = request;
        self.response_scratch = response;
    }

    pub(crate) fn take_descriptor_scratch(&mut self) -> Vec<Descriptor> {
        let scratch = std::mem::take(&mut self.descriptor_scratch);
        if scratch.capacity() == 0 {
            self.parked_descriptor_scratch.pop().unwrap_or(scratch)
        } else {
            scratch
        }
    }

    pub(crate) fn take_response_scratch(&mut self) -> Vec<u8> {
        let scratch = std::mem::take(&mut self.response_scratch);
        if scratch.capacity() == 0 {
            self.parked_response_scratch.pop().unwrap_or(scratch)
        } else {
            scratch
        }
    }

    pub(crate) fn recycle_parked_response_buffers(
        &mut self,
        mut descs: Vec<Descriptor>,
        mut response: Vec<u8>,
    ) {
        descs.clear();
        response.clear();
        self.recycle_descriptor_scratch(descs);
        self.recycle_response_scratch(response);
    }

    pub(crate) fn recycle_descriptor_scratch(&mut self, mut descs: Vec<Descriptor>) {
        if descs.capacity() > self.descriptor_scratch.capacity() {
            std::mem::swap(&mut self.descriptor_scratch, &mut descs);
        }
        self.recycle_extra_descriptor_scratch(descs);
    }

    pub(crate) fn recycle_response_scratch(&mut self, mut response: Vec<u8>) {
        if response.capacity() > self.response_scratch.capacity() {
            std::mem::swap(&mut self.response_scratch, &mut response);
        }
        self.recycle_extra_response_scratch(response);
    }

    pub(crate) fn recycle_extra_descriptor_scratch(&mut self, descs: Vec<Descriptor>) {
        if descs.capacity() != 0
            && self.parked_descriptor_scratch.len() < PARKED_RESPONSE_BUFFER_POOL_LIMIT
        {
            self.parked_descriptor_scratch.push(descs);
        }
    }

    pub(crate) fn recycle_extra_response_scratch(&mut self, response: Vec<u8>) {
        if response.capacity() != 0
            && self.parked_response_scratch.len() < PARKED_RESPONSE_BUFFER_POOL_LIMIT
        {
            self.parked_response_scratch.push(response);
        }
    }

    pub(crate) fn descriptor_chain_into(
        mem: &dyn GuestMemoryMut,
        queue: &VirtioGpuQueue,
        head: u16,
        out: &mut Vec<Descriptor>,
    ) -> bool {
        out.clear();
        let queue_size = queue.effective_size();
        if head >= queue_size {
            return false;
        }
        let mut index = head;
        for _ in 0..queue_size {
            let Some(desc) = Descriptor::read(mem, queue.desc + u64::from(index) * DESC_SIZE)
            else {
                return false;
            };
            let has_next = desc.flags & DESC_F_NEXT != 0;
            out.push(desc);
            if !has_next {
                return true;
            }
            index = desc.next;
            if index >= queue_size {
                return false;
            }
        }
        false
    }

    pub(crate) fn gather_readable_into(
        mem: &dyn GuestMemoryMut,
        descs: &[Descriptor],
        out: &mut Vec<u8>,
    ) {
        out.clear();
        for desc in descs {
            if desc.flags & DESC_F_WRITE != 0 {
                continue;
            }
            let start = out.len();
            let Some(end) = start.checked_add(desc.len as usize) else {
                out.clear();
                return;
            };
            if end > MAX_GPU_REQUEST_LEN {
                out.clear();
                return;
            }
            if let Some(bytes) = mem.read_bytes(desc.addr, desc.len as usize) {
                out.extend_from_slice(&bytes);
            }
        }
    }

    pub(crate) fn scatter_write(
        mem: &mut dyn GuestMemoryMut,
        descs: &[Descriptor],
        bytes: &[u8],
    ) -> u32 {
        let mut offset = 0usize;
        for desc in descs {
            if desc.flags & DESC_F_WRITE == 0 {
                continue;
            }
            let writable = (desc.len as usize).min(bytes.len().saturating_sub(offset));
            if writable == 0 {
                continue;
            }
            if !mem.write_bytes(desc.addr, &bytes[offset..offset + writable]) {
                break;
            }
            offset += writable;
            if offset == bytes.len() {
                break;
            }
        }
        u32::try_from(offset).unwrap_or(u32::MAX)
    }

    pub(crate) fn write_used(
        mem: &mut dyn GuestMemoryMut,
        queue: &VirtioGpuQueue,
        id: u16,
        len: u32,
    ) {
        if queue.device == 0 {
            return;
        }
        let queue_size = queue.effective_size();
        let Some(used_idx) = read_u16(mem, queue.device + 2) else {
            return;
        };
        let elem = queue.device + 4 + u64::from(used_idx % queue_size) * 8;
        let _ = mem.write_bytes(elem, &u32::from(id).to_le_bytes());
        let _ = mem.write_bytes(elem + 4, &len.to_le_bytes());
        let _ = mem.write_bytes(queue.device + 2, &used_idx.wrapping_add(1).to_le_bytes());
    }
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
