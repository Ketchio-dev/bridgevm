//! Device-side JSONL trace emission and its sampling policy.

use super::*;
use crate::pcie::VIRTIO_GPU_MSIX_VECTOR_COUNT;
use crate::virtio_gpu_3d::CompletedFence;
use crate::virtio_gpu_3d::Submit3dDiagnostic;
use crate::virtio_gpu_3d::VIRTIO_GPU_CMD_SUBMIT_3D;
use crate::virtio_gpu_3d::VIRTIO_GPU_FLAG_FENCE;
use crate::virtio_gpu_trace::write_json_string;
use std::fmt::Write as _;

// Sampled renderer-fence JSONL events.

impl VirtioGpu {
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
}
// SUBMIT_3D request and bounded renderer diagnostic JSONL fields.

pub(crate) fn write_submit_request_details(out: &mut String, request: &[u8]) {
    let size = read_le_u32(request, 24).unwrap_or(0) as usize;
    let payload_start = 32usize.min(request.len());
    let payload_end = payload_start.saturating_add(size).min(request.len());
    let payload = request.get(payload_start..payload_end).unwrap_or(&[]);
    let first_dword = read_le_u32(payload, 0).unwrap_or(0);
    let _ = write!(
        out,
        ",\"submit_size\":{},\"submit_dwords\":{},\"submit_first_dword\":{},\"submit_first_command_id\":{},\"submit_prefix_hex\":",
        size, size.div_ceil(4), first_dword, first_dword & 0xff
    );
    write_hex_prefix_json(out, payload, submit_trace_prefix_len());
}

pub(crate) fn write_submit_diagnostic(out: &mut String, diagnostic: Submit3dDiagnostic) {
    let _ = write!(out, ",\"renderer_status\":{}", diagnostic.renderer_status);
    if let Some(offset) = diagnostic.command_offset_dwords {
        let _ = write!(out, ",\"renderer_command_offset_dwords\":{offset}");
    }
    if let Some(command_id) = diagnostic.command_id {
        let _ = write!(out, ",\"renderer_command_id\":{command_id}");
    }
    if let Some(header) = diagnostic.command_header {
        let _ = write!(out, ",\"renderer_command_header\":{header}");
    }
    if let Some(resource_id) = diagnostic.resource_id {
        let _ = write!(out, ",\"renderer_resource_id\":{resource_id}");
    }
    if let Some(found) = diagnostic.resource_found {
        let _ = write!(out, ",\"renderer_resource_found\":{found}");
    }
    if let Some(backed) = diagnostic.resource_backed {
        let _ = write!(out, ",\"renderer_resource_backed\":{backed}");
    }
}

pub(crate) fn trace_sample(count: u64) -> bool {
    count <= 64 || count % 1024 == 0
}

impl VirtioGpu {
    pub(crate) fn trace_device_init(&mut self, backend_3d: bool) {
        let width = self.width;
        let height = self.height;
        self.record_trace_fields("device_init", |fields| {
            let _ = write!(
                fields,
                ",\"width\":{},\"height\":{},\"device_id\":{},\"vendor_id\":{},\"queue_count\":{},\"queue_max\":{},\"msix_vectors\":{},\"backend_3d\":{},\"common_cfg_offset\":{},\"device_cfg_offset\":{},\"notify_cfg_offset\":{}",
                width,
                height,
                DEVICE_ID_GPU,
                VENDOR_ID_COMPAT,
                QUEUE_COUNT,
                QUEUE_MAX,
                VIRTIO_GPU_MSIX_VECTOR_COUNT,
                backend_3d,
                PCI_COMMON_CFG_OFFSET,
                PCI_DEVICE_CFG_OFFSET,
                PCI_NOTIFY_CFG_OFFSET
            );
        });
    }

    pub(crate) fn trace_common_read(&mut self, offset: u64, size: u8, value: u64) {
        if !self.trace.enabled() {
            return;
        }
        let field = match offset {
            COMMON_DEVICE_FEATURE | REG_DEVICE_FEATURES => "device_features",
            COMMON_DRIVER_FEATURE | REG_DRIVER_FEATURES => "driver_features",
            COMMON_DEVICE_STATUS | REG_STATUS => "device_status",
            COMMON_QUEUE_SIZE | REG_QUEUE_NUM => "queue_size",
            COMMON_QUEUE_ENABLE | REG_QUEUE_READY => "queue_enable",
            _ => return,
        };
        let device_features_sel = self.pci.device_features_sel;
        let driver_features_sel = self.pci.driver_features_sel;
        let queue_sel = self.pci.queue_sel;
        self.record_trace_fields("common_read", |fields| {
            fields.push_str(",\"field\":");
            write_json_string(fields, field);
            let _ = write!(
                fields,
                ",\"offset\":{},\"size\":{},\"value\":{},\"value_hex\":\"{:#x}\",\"device_features_sel\":{},\"driver_features_sel\":{},\"queue_sel\":{}",
                offset,
                size,
                value,
                value,
                device_features_sel,
                driver_features_sel,
                queue_sel
            );
        });
    }

    pub(crate) fn trace_queue_notify(&mut self, queue_index: u16) {
        if !self.trace.enabled() {
            return;
        }
        self.trace_queue_notify_count = self.trace_queue_notify_count.saturating_add(1);
        if !trace_sample(self.trace_queue_notify_count) {
            return;
        }
        let Some(queue) = self.pci.queues.get(queue_index as usize).copied() else {
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
        let submit_diagnostic = (hdr.typ == VIRTIO_GPU_CMD_SUBMIT_3D)
            .then(|| self.three_d.take_submit_diagnostic())
            .flatten();
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
            if let Some(diagnostic) = submit_diagnostic {
                write_submit_diagnostic(fields, diagnostic);
            }
            write_trace_command_response_details(fields, response_type, response);
        });
    }
}
