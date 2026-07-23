//! Continuation of the `magic_value` impl block, split for the 1000-line rule.

use super::*;
use crate::fwcfg::GuestMemoryMut;
use crate::virtio_gpu_3d;
use crate::virtio_gpu_3d::CtrlHdr3d;
use crate::virtio_gpu_3d::VIRTIO_GPU_CMD_CTX_ATTACH_RESOURCE;
use crate::virtio_gpu_3d::VIRTIO_GPU_CMD_CTX_CREATE;
use crate::virtio_gpu_3d::VIRTIO_GPU_CMD_CTX_DESTROY;
use crate::virtio_gpu_3d::VIRTIO_GPU_CMD_CTX_DETACH_RESOURCE;
use crate::virtio_gpu_3d::VIRTIO_GPU_CMD_GET_CAPSET;
use crate::virtio_gpu_3d::VIRTIO_GPU_CMD_GET_CAPSET_INFO;
use crate::virtio_gpu_3d::VIRTIO_GPU_CMD_RESOURCE_CREATE_3D;
use crate::virtio_gpu_3d::VIRTIO_GPU_CMD_RESOURCE_CREATE_BLOB;
use crate::virtio_gpu_3d::VIRTIO_GPU_CMD_RESOURCE_MAP_BLOB;
use crate::virtio_gpu_3d::VIRTIO_GPU_CMD_RESOURCE_UNMAP_BLOB;
use crate::virtio_gpu_3d::VIRTIO_GPU_CMD_SUBMIT_3D;
use crate::virtio_gpu_3d::VIRTIO_GPU_CMD_TRANSFER_FROM_HOST_3D;
use crate::virtio_gpu_3d::VIRTIO_GPU_CMD_TRANSFER_TO_HOST_3D;
use std::fmt::Write as _;
use std::time::Instant;

impl VirtioGpu {
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

    pub(crate) fn handle_cursor_request_into(&mut self, request: &[u8], out: &mut Vec<u8>) {
        let hdr = CtrlHdr::parse(request);
        match hdr.map(|h| h.typ) {
            Some(VIRTIO_GPU_CMD_UPDATE_CURSOR | VIRTIO_GPU_CMD_MOVE_CURSOR) => {
                response_hdr_into(out, VIRTIO_GPU_RESP_OK_NODATA, hdr);
            }
            _ => response_hdr_into(out, VIRTIO_GPU_RESP_ERR_UNSPEC, hdr),
        }
    }

    pub(crate) fn handle_control_request_into(
        &mut self,
        mem: &dyn GuestMemoryMut,
        request: &[u8],
        out: &mut Vec<u8>,
    ) {
        let Some(hdr) = CtrlHdr::parse(request) else {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_UNSPEC, None);
            return;
        };
        match hdr.typ {
            VIRTIO_GPU_CMD_GET_DISPLAY_INFO => self.response_display_info_into(Some(hdr), out),
            VIRTIO_GPU_CMD_GET_EDID => self.response_edid_into(Some(hdr), out),
            VIRTIO_GPU_CMD_RESOURCE_CREATE_2D => {
                self.resource_create_2d_into(request, Some(hdr), out)
            }
            VIRTIO_GPU_CMD_RESOURCE_UNREF => self.resource_unref_into(request, Some(hdr), out),
            VIRTIO_GPU_CMD_RESOURCE_ATTACH_BACKING => {
                self.attach_backing_into(mem, request, Some(hdr), out)
            }
            VIRTIO_GPU_CMD_RESOURCE_DETACH_BACKING => {
                self.detach_backing_into(request, Some(hdr), out)
            }
            VIRTIO_GPU_CMD_SET_SCANOUT => self.set_scanout_into(request, Some(hdr), out),
            VIRTIO_GPU_CMD_SET_SCANOUT_BLOB => self.set_scanout_blob_into(request, Some(hdr), out),
            VIRTIO_GPU_CMD_TRANSFER_TO_HOST_2D => {
                self.transfer_to_host_2d_into(mem, request, Some(hdr), out)
            }
            VIRTIO_GPU_CMD_RESOURCE_FLUSH => self.resource_flush_into(mem, request, Some(hdr), out),
            VIRTIO_GPU_CMD_GET_CAPSET_INFO
            | VIRTIO_GPU_CMD_GET_CAPSET
            | VIRTIO_GPU_CMD_RESOURCE_CREATE_BLOB
            | VIRTIO_GPU_CMD_CTX_CREATE
            | VIRTIO_GPU_CMD_CTX_DESTROY
            | VIRTIO_GPU_CMD_CTX_ATTACH_RESOURCE
            | VIRTIO_GPU_CMD_CTX_DETACH_RESOURCE
            | VIRTIO_GPU_CMD_RESOURCE_CREATE_3D
            | VIRTIO_GPU_CMD_TRANSFER_TO_HOST_3D
            | VIRTIO_GPU_CMD_TRANSFER_FROM_HOST_3D
            | VIRTIO_GPU_CMD_SUBMIT_3D
            | VIRTIO_GPU_CMD_RESOURCE_MAP_BLOB
            | VIRTIO_GPU_CMD_RESOURCE_UNMAP_BLOB => {
                let hdr3d = CtrlHdr3d::parse(request).unwrap();
                if hdr3d.typ == VIRTIO_GPU_CMD_CTX_DESTROY {
                    if let Some(resource_id) = self
                        .blob_scanout
                        .as_ref()
                        .map(|scanout| scanout.resource_id)
                    {
                        if self.three_d.ctx_has_resource(hdr3d.ctx_id, resource_id) {
                            self.unbind_blob_scanout();
                        }
                    }
                }
                if !self
                    .three_d
                    .handle_with_mem_into(Some(mem), request, hdr3d, out)
                {
                    virtio_gpu_3d::response_hdr_into(
                        out,
                        virtio_gpu_3d::VIRTIO_GPU_RESP_ERR_UNSPEC,
                        Some(hdr3d),
                    );
                }
            }
            _ => response_hdr_into(out, VIRTIO_GPU_RESP_ERR_UNSPEC, Some(hdr)),
        }
    }

    pub fn drain_host_vblank(&mut self, mem: &mut dyn GuestMemoryMut) {
        self.drain_host_vblank_at(mem, Instant::now());
    }

    pub(crate) fn drain_host_vblank_at(&mut self, mem: &mut dyn GuestMemoryMut, now: Instant) {
        if self.vblank_interval.is_zero() || self.pending_vblank.is_empty() {
            return;
        }
        if self
            .last_vblank
            .is_some_and(|last| now.saturating_duration_since(last) < self.vblank_interval)
        {
            return;
        }

        // Retire exactly one response. Even if the vCPU did not exit for several
        // intervals, do not catch up in a burst.
        let pending_response = self.pending_vblank.remove(0);
        let used_len =
            Self::scatter_write(mem, &pending_response.descs, &pending_response.response);
        Self::write_used(
            mem,
            &pending_response.queue,
            pending_response.head,
            used_len,
        );
        self.mark_queue_interrupt(pending_response.queue_index);
        // Anchor the next deadline on the absolute schedule, not the (late)
        // retire time, so wake/drain latency does not accumulate into a lower
        // long-run rate. Re-anchor at `now` only after a gap of more than one
        // interval (guest asleep) — never catch up in a burst.
        self.last_vblank = Some(match self.last_vblank {
            Some(last)
                if now.saturating_duration_since(last + self.vblank_interval)
                    <= self.vblank_interval =>
            {
                last + self.vblank_interval
            }
            _ => now,
        });
        self.vblank_paced_count = self.vblank_paced_count.saturating_add(1);
        self.publish_vblank_wake();

        let count = self.vblank_paced_count;
        let interval_ns = self.vblank_interval.as_nanos();
        let pending = self.pending_vblank.len();
        self.record_trace_fields("vblank_paced", |fields| {
            let _ = write!(
                fields,
                ",\"vblank_paced_count\":{count},\"interval_ns\":{interval_ns},\"used_len\":{used_len},\"pending\":{pending}"
            );
        });
        self.recycle_parked_response_buffers(pending_response.descs, pending_response.response);
    }

    pub fn drain_completed_fences(&mut self, mem: &mut dyn GuestMemoryMut) {
        self.drain_completed_fences_inner(mem, false);
    }

    pub(crate) fn drain_completed_fences_after_queue(&mut self, mem: &mut dyn GuestMemoryMut) {
        self.drain_completed_fences_inner(mem, true);
    }

    pub(crate) fn drain_completed_fences_inner(
        &mut self,
        mem: &mut dyn GuestMemoryMut,
        after_queue: bool,
    ) {
        let mut completed = std::mem::take(&mut self.completed_fences_scratch);
        completed.clear();
        if after_queue {
            self.three_d
                .drain_completed_fences_after_queue_into(&mut completed);
        } else {
            self.three_d.drain_completed_fences_into(&mut completed);
        }
        if completed.is_empty() || self.pending_fenced.is_empty() {
            for fence in &completed {
                self.trace_fence_complete(*fence);
            }
            completed.clear();
            self.completed_fences_scratch = completed;
            return;
        }
        for fence in &completed {
            self.trace_fence_complete(*fence);
        }
        let mut index = 0;
        while index < self.pending_fenced.len() {
            let ready = completed.iter().any(|completed| {
                let pending_response = &self.pending_fenced[index];
                completed.ctx_id == pending_response.fence.ctx_id
                    && completed.ring_idx == pending_response.fence.ring_idx
                    && completed.fence_id >= pending_response.fence.fence_id
            });
            if !ready {
                index += 1;
                continue;
            }

            let pending_response = self.pending_fenced.remove(index);
            let used_len =
                Self::scatter_write(mem, &pending_response.descs, &pending_response.response);
            self.trace_fence_delivery(pending_response.fence, used_len);
            Self::write_used(
                mem,
                &pending_response.queue,
                pending_response.head,
                used_len,
            );
            self.mark_queue_interrupt(pending_response.queue_index);
            self.recycle_parked_response_buffers(pending_response.descs, pending_response.response);
        }
        completed.clear();
        self.completed_fences_scratch = completed;
    }

    pub(crate) fn response_display_info_into(&self, hdr: Option<CtrlHdr>, out: &mut Vec<u8>) {
        response_hdr_into(out, VIRTIO_GPU_RESP_OK_DISPLAY_INFO, hdr);
        for scanout in 0..16 {
            if scanout == 0 {
                push_rect(
                    out,
                    Rect {
                        x: 0,
                        y: 0,
                        width: self.width,
                        height: self.height,
                    },
                );
                out.extend_from_slice(&1u32.to_le_bytes());
                out.extend_from_slice(&0u32.to_le_bytes());
            } else {
                out.extend_from_slice(&[0u8; 24]);
            }
        }
    }

    pub(crate) fn response_edid_into(&self, hdr: Option<CtrlHdr>, out: &mut Vec<u8>) {
        response_hdr_into(out, VIRTIO_GPU_RESP_OK_EDID, hdr);
        out.extend_from_slice(&128u32.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        let edid = build_edid(self.width, self.height);
        out.extend_from_slice(&edid);
        out.resize(out.len() + (1024 - 128), 0);
    }
}
