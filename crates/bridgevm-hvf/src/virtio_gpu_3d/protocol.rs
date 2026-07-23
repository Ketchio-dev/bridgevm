//! virtio-gpu 3D wire format: feature/command/response codes, request lengths, header decode and encode.

pub fn response_hdr(typ: u32, request: Option<CtrlHdr3d>) -> Vec<u8> {
    let mut out = Vec::with_capacity(CTRL_HDR_LEN);
    response_hdr_into(&mut out, typ, request);
    out
}

pub fn response_hdr_into(out: &mut Vec<u8>, typ: u32, request: Option<CtrlHdr3d>) {
    let (flags, fence_id, ctx_id, padding) = request.map_or((0, 0, 0, 0), |hdr| {
        (
            hdr.flags & (VIRTIO_GPU_FLAG_FENCE | VIRTIO_GPU_FLAG_INFO_RING_IDX),
            if hdr.fenced() { hdr.fence_id } else { 0 },
            hdr.ctx_id,
            hdr.padding,
        )
    });
    out.clear();
    out.reserve(CTRL_HDR_LEN);
    out.extend_from_slice(&typ.to_le_bytes());
    out.extend_from_slice(&flags.to_le_bytes());
    out.extend_from_slice(&fence_id.to_le_bytes());
    out.extend_from_slice(&ctx_id.to_le_bytes());
    out.extend_from_slice(&padding.to_le_bytes());
}

pub(crate) fn read_le_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

pub(crate) fn read_le_u64(bytes: &[u8], offset: usize) -> Option<u64> {
    Some(u64::from_le_bytes(
        bytes.get(offset..offset + 8)?.try_into().ok()?,
    ))
}

pub const VIRTIO_GPU_F_VIRGL: u32 = 1 << 0;

pub const VIRTIO_GPU_F_RESOURCE_BLOB: u32 = 1 << 3;

pub const VIRTIO_GPU_F_CONTEXT_INIT: u32 = 1 << 4;

pub const VIRTIO_GPU_FLAG_FENCE: u32 = 1;

pub const VIRTIO_GPU_FLAG_INFO_RING_IDX: u32 = 1 << 1;

pub const VIRTIO_GPU_CMD_GET_CAPSET_INFO: u32 = 0x0108;

pub const VIRTIO_GPU_CMD_GET_CAPSET: u32 = 0x0109;

pub const VIRTIO_GPU_CMD_RESOURCE_CREATE_BLOB: u32 = 0x010c;

pub const VIRTIO_GPU_CMD_CTX_CREATE: u32 = 0x0200;

pub const VIRTIO_GPU_CMD_CTX_DESTROY: u32 = 0x0201;

pub const VIRTIO_GPU_CMD_CTX_ATTACH_RESOURCE: u32 = 0x0202;

pub const VIRTIO_GPU_CMD_CTX_DETACH_RESOURCE: u32 = 0x0203;

pub const VIRTIO_GPU_CMD_RESOURCE_CREATE_3D: u32 = 0x0204;

pub const VIRTIO_GPU_CMD_TRANSFER_TO_HOST_3D: u32 = 0x0205;

pub const VIRTIO_GPU_CMD_TRANSFER_FROM_HOST_3D: u32 = 0x0206;

pub const VIRTIO_GPU_CMD_SUBMIT_3D: u32 = 0x0207;

pub const VIRTIO_GPU_CMD_RESOURCE_MAP_BLOB: u32 = 0x0208;

pub const VIRTIO_GPU_CMD_RESOURCE_UNMAP_BLOB: u32 = 0x0209;

pub const VIRTIO_GPU_RESP_OK_NODATA: u32 = 0x1100;

pub const VIRTIO_GPU_RESP_OK_CAPSET_INFO: u32 = 0x1102;

pub const VIRTIO_GPU_RESP_OK_CAPSET: u32 = 0x1103;

pub const VIRTIO_GPU_RESP_OK_MAP_INFO: u32 = 0x1106;

pub const VIRTIO_GPU_RESP_ERR_UNSPEC: u32 = 0x1200;

pub const VIRTIO_GPU_RESP_ERR_OUT_OF_MEMORY: u32 = 0x1201;

pub const VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER: u32 = 0x1203;

pub const VIRTIO_GPU_BLOB_MEM_GUEST: u32 = 1;

pub const VIRTIO_GPU_BLOB_MEM_HOST3D: u32 = 2;

pub const VIRTIO_GPU_BLOB_MEM_HOST3D_GUEST: u32 = 3;

pub const VIRTIO_GPU_MAP_CACHE_MASK: u32 = 0x0f;

pub(crate) const CTRL_HDR_LEN: usize = 24;

pub(crate) const CTX_CREATE_LEN: usize = 24 + 4 + 4 + 64;

pub(crate) const CTX_RESOURCE_LEN: usize = 24 + 4 + 4;

pub(crate) const RESOURCE_CREATE_3D_LEN: usize = 24 + 12 * 4;

pub(crate) const TRANSFER_3D_LEN: usize = 24 + 6 * 4 + 8 + 4 * 4;

pub(crate) const SUBMIT_3D_LEN: usize = 24 + 4 + 4;

pub(crate) const RESOURCE_CREATE_BLOB_LEN: usize = 24 + 4 + 4 + 4 + 4 + 8 + 8;

pub(crate) const RESOURCE_MAP_BLOB_LEN: usize = 24 + 4 + 4 + 8;

pub(crate) const RESOURCE_UNMAP_BLOB_LEN: usize = 24 + 4 + 4;

pub(crate) const MEM_ENTRY_LEN: usize = 16;

pub(crate) const MAX_SUBMIT_3D_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, Copy)]
pub struct CtrlHdr3d {
    pub typ: u32,
    pub flags: u32,
    pub fence_id: u64,
    pub ctx_id: u32,
    pub ring_idx: u8,
    pub padding: u32,
}

impl CtrlHdr3d {
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        let padding = read_le_u32(bytes, 20)?;
        Some(Self {
            typ: read_le_u32(bytes, 0)?,
            flags: read_le_u32(bytes, 4)?,
            fence_id: read_le_u64(bytes, 8)?,
            ctx_id: read_le_u32(bytes, 16)?,
            ring_idx: if read_le_u32(bytes, 4)? & VIRTIO_GPU_FLAG_INFO_RING_IDX != 0 {
                (padding & 0xff) as u8
            } else {
                0
            },
            padding,
        })
    }

    pub fn fenced(self) -> bool {
        self.flags & VIRTIO_GPU_FLAG_FENCE != 0
    }
}
