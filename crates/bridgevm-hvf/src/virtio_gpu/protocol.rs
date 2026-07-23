//! virtio-gpu wire format: command/response opcodes, pixel formats, control header, rectangles.

use super::*;
use crate::virtio_gpu_3d;
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
use crate::virtio_gpu_3d::VIRTIO_GPU_FLAG_FENCE;

pub(crate) const VIRTIO_GPU_F_EDID: u32 = 1 << 1;

pub(crate) const VIRTIO_F_VERSION_1: u32 = 1 << 0;

pub(crate) const VIRTIO_GPU_CMD_GET_DISPLAY_INFO: u32 = 0x0100;

pub(crate) const VIRTIO_GPU_CMD_RESOURCE_CREATE_2D: u32 = 0x0101;

pub(crate) const VIRTIO_GPU_CMD_RESOURCE_UNREF: u32 = 0x0102;

pub(crate) const VIRTIO_GPU_CMD_SET_SCANOUT: u32 = 0x0103;

pub(crate) const VIRTIO_GPU_CMD_RESOURCE_FLUSH: u32 = 0x0104;

pub(crate) const VIRTIO_GPU_CMD_TRANSFER_TO_HOST_2D: u32 = 0x0105;

pub(crate) const VIRTIO_GPU_CMD_RESOURCE_ATTACH_BACKING: u32 = 0x0106;

pub(crate) const VIRTIO_GPU_CMD_RESOURCE_DETACH_BACKING: u32 = 0x0107;

pub(crate) const VIRTIO_GPU_CMD_GET_EDID: u32 = 0x010a;

pub(crate) const VIRTIO_GPU_CMD_SET_SCANOUT_BLOB: u32 = 0x010d;

pub(crate) const VIRTIO_GPU_CMD_UPDATE_CURSOR: u32 = 0x0300;

pub(crate) const VIRTIO_GPU_CMD_MOVE_CURSOR: u32 = 0x0301;

pub(crate) const VIRTIO_GPU_RESP_OK_NODATA: u32 = 0x1100;

pub(crate) const VIRTIO_GPU_RESP_OK_DISPLAY_INFO: u32 = 0x1101;

pub(crate) const VIRTIO_GPU_RESP_OK_EDID: u32 = 0x1104;

pub(crate) const VIRTIO_GPU_RESP_ERR_UNSPEC: u32 = 0x1200;

pub(crate) const FORMAT_B8G8R8A8_UNORM: u32 = 1;

pub(crate) const FORMAT_B8G8R8X8_UNORM: u32 = 2;

pub(crate) const FORMAT_X8R8G8B8_UNORM: u32 = 3;

pub(crate) const FORMAT_R8G8B8X8_UNORM: u32 = 4;

pub(crate) const SET_SCANOUT_BLOB_LEN: usize = 24 + 16 + 4 + 4 + 4 + 4 + 4 + 4 + 16 + 16;

#[derive(Debug, Clone, Copy)]
pub(crate) struct CtrlHdr {
    pub(crate) typ: u32,
    pub(crate) flags: u32,
    pub(crate) fence_id: u64,
    pub(crate) ctx_id: u32,
    pub(crate) padding: u32,
}

pub(crate) fn union_rect(a: Rect, b: Rect) -> Rect {
    if a.width == 0 || a.height == 0 {
        return b;
    }
    if b.width == 0 || b.height == 0 {
        return a;
    }
    let x = a.x.min(b.x);
    let y = a.y.min(b.y);
    let right = a.x.saturating_add(a.width).max(b.x.saturating_add(b.width));
    let bottom =
        a.y.saturating_add(a.height)
            .max(b.y.saturating_add(b.height));
    Rect {
        x,
        y,
        width: right - x,
        height: bottom - y,
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Rect {
    pub(crate) x: u32,
    pub(crate) y: u32,
    pub(crate) width: u32,
    pub(crate) height: u32,
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

pub(crate) fn push_rect(out: &mut Vec<u8>, rect: Rect) {
    out.extend_from_slice(&rect.x.to_le_bytes());
    out.extend_from_slice(&rect.y.to_le_bytes());
    out.extend_from_slice(&rect.width.to_le_bytes());
    out.extend_from_slice(&rect.height.to_le_bytes());
}

pub(crate) fn read_rect(bytes: &[u8], offset: usize) -> Option<Rect> {
    Some(Rect {
        x: read_le_u32(bytes, offset)?,
        y: read_le_u32(bytes, offset + 4)?,
        width: read_le_u32(bytes, offset + 8)?,
        height: read_le_u32(bytes, offset + 12)?,
    })
}

pub(crate) fn format_supported(format: u32) -> bool {
    matches!(
        format,
        FORMAT_B8G8R8A8_UNORM
            | FORMAT_B8G8R8X8_UNORM
            | FORMAT_X8R8G8B8_UNORM
            | FORMAT_R8G8B8X8_UNORM
    )
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
