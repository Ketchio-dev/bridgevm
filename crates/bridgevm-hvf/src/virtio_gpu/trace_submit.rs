// SUBMIT_3D request and bounded renderer diagnostic JSONL fields.

use crate::virtio_gpu_3d::Submit3dDiagnostic;

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
