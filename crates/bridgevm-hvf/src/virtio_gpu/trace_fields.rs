//! JSON field writers for request/response payload details, descriptor lengths, hex prefixes.

use super::*;
use crate::virtio_gpu_3d;
use crate::virtio_gpu_3d::VIRTIO_GPU_CMD_CTX_ATTACH_RESOURCE;
use crate::virtio_gpu_3d::VIRTIO_GPU_CMD_CTX_CREATE;
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
use crate::virtio_gpu_trace::write_json_string;
use std::fmt::Write as _;
use std::sync::OnceLock;

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

pub(crate) fn write_trace_command_response_details(
    out: &mut String,
    response_type: u32,
    response: &[u8],
) {
    match response_type {
        VIRTIO_GPU_RESP_OK_DISPLAY_INFO => {
            let _ = write!(
                out,
                ",\"response_scanout0_x\":{},\"response_scanout0_y\":{},\"response_scanout0_width\":{},\"response_scanout0_height\":{},\"response_scanout0_enabled\":{},\"response_scanout0_flags\":{}",
                read_le_u32(response, 24).unwrap_or(0),
                read_le_u32(response, 28).unwrap_or(0),
                read_le_u32(response, 32).unwrap_or(0),
                read_le_u32(response, 36).unwrap_or(0),
                read_le_u32(response, 40).unwrap_or(0),
                read_le_u32(response, 44).unwrap_or(0)
            );
        }
        VIRTIO_GPU_RESP_OK_EDID => {
            let edid_size = read_le_u32(response, 24).unwrap_or(0) as usize;
            let available = response.len().saturating_sub(32);
            let checksum_valid = edid_size > 0
                && edid_size <= available
                && response[32..32 + edid_size]
                    .iter()
                    .fold(0u8, |sum, byte| sum.wrapping_add(*byte))
                    == 0;
            let _ = write!(
                out,
                ",\"response_edid_size\":{},\"response_edid_checksum_valid\":{}",
                edid_size, checksum_valid
            );
        }
        virtio_gpu_3d::VIRTIO_GPU_RESP_OK_CAPSET_INFO => {
            let _ = write!(
                out,
                ",\"response_capset_id\":{},\"response_capset_max_version\":{},\"response_capset_max_size\":{}",
                read_le_u32(response, 24).unwrap_or(0),
                read_le_u32(response, 28).unwrap_or(0),
                read_le_u32(response, 32).unwrap_or(0)
            );
        }
        virtio_gpu_3d::VIRTIO_GPU_RESP_OK_CAPSET => {
            let _ = write!(
                out,
                ",\"response_capset_bytes\":{}",
                response.len().saturating_sub(24)
            );
        }
        _ => {}
    }
}

pub(crate) fn write_descriptor_lengths(out: &mut String, descs: &[Descriptor], writable: bool) {
    let mut first = true;
    for desc in descs {
        if (desc.flags & DESC_F_WRITE != 0) != writable {
            continue;
        }
        if !first {
            out.push(',');
        }
        first = false;
        let _ = write!(out, "{}", desc.len);
    }
}

/// Bytes of SUBMIT_3D payload preserved in the JSONL trace. The 32-byte
/// default identifies the leading command; raising it via
/// BRIDGEVM_VIRTIO_GPU_TRACE_SUBMIT_PREFIX captures whole command streams for
/// offline decoding when diagnosing renderer-level divergence.
pub(crate) fn submit_trace_prefix_len() -> usize {
    static LEN: OnceLock<usize> = OnceLock::new();
    *LEN.get_or_init(|| {
        std::env::var("BRIDGEVM_VIRTIO_GPU_TRACE_SUBMIT_PREFIX")
            .ok()
            .and_then(|value| value.trim().parse::<usize>().ok())
            .filter(|&value| value > 0)
            .unwrap_or(32)
    })
}

pub(crate) fn write_hex_prefix_json(out: &mut String, bytes: &[u8], max_len: usize) {
    out.push('"');
    write_hex_prefix(out, bytes, max_len);
    out.push('"');
}

pub(crate) fn write_hex_prefix(out: &mut String, bytes: &[u8], max_len: usize) {
    for (index, byte) in bytes.iter().take(max_len).enumerate() {
        if index > 0 {
            out.push(' ');
        }
        let _ = write!(out, "{byte:02x}");
    }
    if bytes.len() > max_len {
        out.push_str(" ...");
    }
}

#[cfg(test)]
pub(crate) fn hex_prefix(bytes: &[u8], max_len: usize) -> String {
    let prefix_len = bytes.len().min(max_len);
    let mut out = String::with_capacity(prefix_len.saturating_mul(3).saturating_add(4));
    write_hex_prefix(&mut out, bytes, max_len);
    out
}
