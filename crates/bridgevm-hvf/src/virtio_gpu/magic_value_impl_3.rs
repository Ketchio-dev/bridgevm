//! Continuation of the `magic_value` impl block, split for the 1000-line rule.

use super::*;
use crate::fwcfg::GuestMemoryMut;
use crate::virtio_gpu_3d::CompletedFence;
use crate::virtio_gpu_3d::VIRTIO_GPU_CMD_SUBMIT_3D;
use crate::virtio_gpu_3d::VIRTIO_GPU_FLAG_FENCE;
use crate::virtio_gpu_trace::venus_start_trace_enabled;
use std::fmt::Write as _;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::time::Instant;

impl VirtioGpu {
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

    pub(crate) fn selected_queue(&self) -> Option<VirtioGpuQueue> {
        self.queues.get(self.queue_sel as usize).copied()
    }

    pub(crate) fn write_selected_queue(&mut self, write: impl FnOnce(&mut VirtioGpuQueue)) {
        if let Some(queue) = self.queues.get_mut(self.queue_sel as usize) {
            write(queue);
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

    /// Host-driven scanout resize. Updates the reported resolution and raises a
    /// virtio-gpu DISPLAY event + config-change interrupt so the guest WDDM
    /// driver re-queries GET_DISPLAY_INFO/GET_EDID and switches modes. No-op
    /// (returns false) when the size is unchanged or out of range; the caller
    /// delivers the config interrupt via the device wrapper's drain path.
    pub(crate) fn request_display_resolution(&mut self, width: u32, height: u32) -> bool {
        if width == 0
            || height == 0
            || width > MAX_SCANOUT_DIMENSION
            || height > MAX_SCANOUT_DIMENSION
        {
            return false;
        }
        if width == self.width && height == self.height {
            return false;
        }
        self.width = width;
        self.height = height;
        // Grow the 2D scanout backing to the new geometry; the guest re-creates
        // its scanout resource after the mode switch, so drop the stale binding.
        self.scanout.clear();
        self.scanout.resize(scanout_len(width, height), 0);
        self.scanout_resource = None;
        self.unbind_blob_scanout();
        self.events_read |= VIRTIO_GPU_EVENT_DISPLAY;
        self.pending_config_change = true;
        self.interrupt_status |= 2;
        true
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
}
