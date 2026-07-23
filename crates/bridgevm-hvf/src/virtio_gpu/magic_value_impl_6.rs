//! Continuation of the `magic_value` impl block, split for the 1000-line rule.

use super::*;
use crate::fwcfg::GuestMemoryMut;
use crate::virtio_gpu_3d::CompletedFence;
use crate::virtio_gpu_3d::VIRTIO_GPU_CMD_SUBMIT_3D;
use crate::virtio_gpu_3d::VIRTIO_GPU_FLAG_FENCE;
use crate::virtio_gpu_trace::write_json_string;
use std::fmt::Write as _;

impl VirtioGpu {
    pub(crate) fn trace_queue_notify(&mut self, queue_index: u16) {
        if !self.trace.enabled() {
            return;
        }
        self.trace_queue_notify_count = self.trace_queue_notify_count.saturating_add(1);
        if !trace_sample(self.trace_queue_notify_count) {
            return;
        }
        let Some(queue) = self.queues.get(queue_index as usize).copied() else {
            self.record_trace_fields("queue_notify", |fields| {
                let _ = write!(fields, ",\"queue\":{},\"valid\":false", queue_index);
            });
            return;
        };
        self.record_trace_fields("queue_notify", |fields| {
            let _ = write!(
                fields,
                ",\"queue\":{},\"valid\":true,\"size\":{},\"ready\":{},\"desc\":{},\"driver\":{},\"device\":{},\"msix_vector\":{},\"last_avail_idx\":{}",
                queue_index,
                queue.size,
                queue.ready,
                queue.desc,
                queue.driver,
                queue.device,
                queue.msix_vector,
                queue.last_avail_idx
            );
        });
    }

    pub(crate) fn record_trace_fields(
        &mut self,
        event: &str,
        write_fields: impl FnOnce(&mut String),
    ) {
        if !self.trace.enabled() {
            return;
        }
        let mut fields = std::mem::take(&mut self.trace_fields_scratch);
        fields.clear();
        write_fields(&mut fields);
        self.trace.record(event, &fields);
        fields.clear();
        self.trace_fields_scratch = fields;
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn trace_command(
        &mut self,
        queue_index: usize,
        head: u16,
        control: bool,
        descs: &[Descriptor],
        request: &[u8],
        hdr: CtrlHdr,
        response: &[u8],
        handle_ns: u64,
    ) {
        venus_start_trace_command(request, hdr, response);
        if !self.trace.enabled() {
            return;
        }
        let response_type = read_le_u32(response, 0).unwrap_or(0);
        if hdr.typ == VIRTIO_GPU_CMD_SUBMIT_3D && response_type == VIRTIO_GPU_RESP_OK_NODATA {
            // Sample only EMPTY submissions: the Windows KMD's 60 Hz vsync
            // heartbeat floods this counter with size-0 no-ops, and sampling
            // everything let those consume the un-sampled budget so real
            // application command buffers vanished from the trace minutes
            // into a boot. A nonempty SUBMIT_3D is exactly what the P3 gate
            // exists to witness; record every one of them.
            let submit_size = read_le_u32(request, 24).unwrap_or(0);
            if submit_size == 0 {
                self.trace_submit_success_count = self.trace_submit_success_count.saturating_add(1);
                if !trace_sample(self.trace_submit_success_count) {
                    return;
                }
            }
        }
        let readable_descriptor_count = descs
            .iter()
            .filter(|desc| desc.flags & DESC_F_WRITE == 0)
            .count();
        let writable_descriptor_count = descs.len().saturating_sub(readable_descriptor_count);
        let readable_descriptor_bytes = descs
            .iter()
            .filter(|desc| desc.flags & DESC_F_WRITE == 0)
            .fold(0u64, |total, desc| {
                total.saturating_add(u64::from(desc.len))
            });
        let writable_descriptor_bytes = descs
            .iter()
            .filter(|desc| desc.flags & DESC_F_WRITE != 0)
            .fold(0u64, |total, desc| {
                total.saturating_add(u64::from(desc.len))
            });
        let response_planned_write_len = writable_descriptor_bytes.min(response.len() as u64);
        let response_header = CtrlHdr::parse(response);
        self.record_trace_fields("command", |fields| {
            let _ = write!(
                fields,
                ",\"queue\":{},\"head\":{},\"control\":{},\"typ\":{},\"duration_ns\":{handle_ns},\"name\":",
                queue_index, head, control, hdr.typ
            );
            write_json_string(fields, command_name(hdr.typ));
            let _ = write!(
                fields,
                ",\"flags\":{},\"fenced\":{},\"fence_id\":{},\"ctx_id\":{},\"ring_idx\":{},\"request_len\":{},\"response_type\":{},\"response_name\":",
                hdr.flags,
                hdr.flags & VIRTIO_GPU_FLAG_FENCE != 0,
                hdr.fence_id,
                hdr.ctx_id,
                hdr.ring_idx(),
                request.len(),
                response_type
            );
            write_json_string(fields, response_name(response_type));
            let _ = write!(
                fields,
                ",\"response_len\":{},\"descriptor_count\":{},\"readable_descriptor_count\":{},\"readable_descriptor_bytes\":{},\"writable_descriptor_count\":{},\"writable_descriptor_bytes\":{},\"response_planned_write_len\":{},\"response_truncated\":{}",
                response.len(),
                descs.len(),
                readable_descriptor_count,
                readable_descriptor_bytes,
                writable_descriptor_count,
                writable_descriptor_bytes,
                response_planned_write_len,
                response.len() as u64 > writable_descriptor_bytes
            );
            fields.push_str(",\"readable_descriptor_lengths\":[");
            write_descriptor_lengths(fields, descs, false);
            fields.push_str("],\"writable_descriptor_lengths\":[");
            write_descriptor_lengths(fields, descs, true);
            fields.push(']');
            if let Some(response_header) = response_header {
                let _ = write!(
                    fields,
                    ",\"response_header_valid\":true,\"response_flags\":{},\"response_fenced\":{},\"response_fence_id\":{},\"response_ctx_id\":{},\"response_ring_idx\":{}",
                    response_header.flags,
                    response_header.flags & VIRTIO_GPU_FLAG_FENCE != 0,
                    response_header.fence_id,
                    response_header.ctx_id,
                    response_header.ring_idx()
                );
            } else {
                fields.push_str(",\"response_header_valid\":false");
            }
            write_trace_command_details(fields, request, hdr);
            write_trace_command_response_details(fields, response_type, response);
        });
    }

    pub(crate) fn trace_fence_create(
        &mut self,
        fence: CompletedFence,
        backend_accepted: bool,
        outcome: &str,
    ) {
        self.trace_fence_create_count = self.trace_fence_create_count.saturating_add(1);
        if !trace_sample(self.trace_fence_create_count) {
            return;
        }
        self.record_trace_fields("fence_create", |fields| {
            let _ = write!(
                fields,
                ",\"ctx_id\":{},\"ring_idx\":{},\"fence_id\":{},\"backend_accepted\":{}",
                fence.ctx_id, fence.ring_idx, fence.fence_id, backend_accepted
            );
            fields.push_str(",\"outcome\":");
            write_json_string(fields, outcome);
        });
    }

    pub(crate) fn trace_fence_complete(&mut self, fence: CompletedFence) {
        self.trace_fence_complete_count = self.trace_fence_complete_count.saturating_add(1);
        if !trace_sample(self.trace_fence_complete_count) {
            return;
        }
        self.record_trace_fields("fence_complete", |fields| {
            let _ = write!(
                fields,
                ",\"ctx_id\":{},\"ring_idx\":{},\"fence_id\":{}",
                fence.ctx_id, fence.ring_idx, fence.fence_id
            );
        });
    }

    pub(crate) fn trace_fence_delivery(&mut self, fence: CompletedFence, used_len: u32) {
        self.trace_fence_deliver_count = self.trace_fence_deliver_count.saturating_add(1);
        if !trace_sample(self.trace_fence_deliver_count) {
            return;
        }
        self.record_trace_fields("fence_deliver", |fields| {
            let _ = write!(
                fields,
                ",\"ctx_id\":{},\"ring_idx\":{},\"fence_id\":{},\"used_len\":{}",
                fence.ctx_id, fence.ring_idx, fence.fence_id, used_len
            );
        });
    }

    pub(crate) fn mark_queue_interrupt(&mut self, queue_index: usize) {
        if let Some(queue) = self.queues.get_mut(queue_index) {
            queue.pending_msix = true;
            if let Some(bit) = queue_bit(queue_index) {
                self.pending_msix_queue_bits |= bit;
            }
        }
        self.interrupt_status |= 1;
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
