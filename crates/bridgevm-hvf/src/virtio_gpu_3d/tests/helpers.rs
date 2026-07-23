//! Split test module.

use super::super::*;
use crate::fwcfg::GuestMemoryMut;

#[derive(Debug)]
pub(super) struct TestMem {
    pub(super) base: u64,
    pub(super) bytes: Vec<u8>,
}

impl TestMem {
    pub(super) fn new(base: u64, len: usize) -> Self {
        Self {
            base,
            bytes: vec![0; len],
        }
    }

    pub(super) fn offset(&self, gpa: u64) -> Option<usize> {
        gpa.checked_sub(self.base)
            .and_then(|value| usize::try_from(value).ok())
    }
}

impl GuestMemoryMut for TestMem {
    fn write_bytes(&mut self, gpa: u64, data: &[u8]) -> bool {
        let Some(start) = self.offset(gpa) else {
            return false;
        };
        let Some(end) = start.checked_add(data.len()) else {
            return false;
        };
        if end > self.bytes.len() {
            return false;
        }
        self.bytes[start..end].copy_from_slice(data);
        true
    }

    fn read_bytes(&self, gpa: u64, len: usize) -> Option<Vec<u8>> {
        let start = self.offset(gpa)?;
        let end = start.checked_add(len)?;
        (end <= self.bytes.len()).then(|| self.bytes[start..end].to_vec())
    }

    fn host_ptr(&self, gpa: u64, len: usize) -> Option<*mut u8> {
        let start = self.offset(gpa)?;
        let end = start.checked_add(len)?;
        (end <= self.bytes.len()).then(|| self.bytes.as_ptr().wrapping_add(start) as *mut u8)
    }
}

pub(super) fn ctrl_req(typ: u32, ctx_id: u32) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&typ.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0u64.to_le_bytes());
    out.extend_from_slice(&ctx_id.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out
}

pub(super) fn local_scanout_create_req(resource_id: u32, width: u32, height: u32) -> Vec<u8> {
    let mut req = ctrl_req(VIRTIO_GPU_CMD_RESOURCE_CREATE_3D, 0);
    for field in [
        resource_id,
        PIPE_TEXTURE_2D,
        1,
        0x4008a,
        width,
        height,
        1,
        1,
        0,
        0,
        0,
        0,
    ] {
        req.extend_from_slice(&field.to_le_bytes());
    }
    req
}

#[allow(clippy::too_many_arguments)]
pub(super) fn resource_copy_submit_req(
    ctx_id: u32,
    dst_resource_id: u32,
    dst_x: u32,
    dst_y: u32,
    src_resource_id: u32,
    src_x: u32,
    src_y: u32,
    width: u32,
    height: u32,
) -> Vec<u8> {
    let mut command = Vec::with_capacity(VIRGL_RESOURCE_COPY_REGION_BYTES);
    for dword in [
        VIRGL_CCMD_RESOURCE_COPY_REGION | (VIRGL_RESOURCE_COPY_REGION_PAYLOAD_DWORDS << 16),
        dst_resource_id,
        0,
        dst_x,
        dst_y,
        0,
        src_resource_id,
        0,
        src_x,
        src_y,
        0,
        width,
        height,
        1,
    ] {
        command.extend_from_slice(&dword.to_le_bytes());
    }
    let mut req = ctrl_req(VIRTIO_GPU_CMD_SUBMIT_3D, ctx_id);
    req.extend_from_slice(&(command.len() as u32).to_le_bytes());
    req.extend_from_slice(&0u32.to_le_bytes());
    req.extend_from_slice(&command);
    req
}

pub(super) fn create_blob_req(
    resource_id: u32,
    blob_mem: u32,
    blob_id: u64,
    size: u64,
    ctx_id: u32,
) -> Vec<u8> {
    let mut req = ctrl_req(VIRTIO_GPU_CMD_RESOURCE_CREATE_BLOB, ctx_id);
    req.extend_from_slice(&resource_id.to_le_bytes());
    req.extend_from_slice(&blob_mem.to_le_bytes());
    req.extend_from_slice(&0u32.to_le_bytes());
    req.extend_from_slice(&0u32.to_le_bytes());
    req.extend_from_slice(&blob_id.to_le_bytes());
    req.extend_from_slice(&size.to_le_bytes());
    req
}

pub(super) fn create_blob_req_with_entries(
    resource_id: u32,
    blob_mem: u32,
    blob_id: u64,
    size: u64,
    ctx_id: u32,
    entries: &[BlobMemEntry],
) -> Vec<u8> {
    let mut req = ctrl_req(VIRTIO_GPU_CMD_RESOURCE_CREATE_BLOB, ctx_id);
    req.extend_from_slice(&resource_id.to_le_bytes());
    req.extend_from_slice(&blob_mem.to_le_bytes());
    req.extend_from_slice(&0u32.to_le_bytes());
    req.extend_from_slice(&(entries.len() as u32).to_le_bytes());
    req.extend_from_slice(&blob_id.to_le_bytes());
    req.extend_from_slice(&size.to_le_bytes());
    for entry in entries {
        req.extend_from_slice(&entry.addr.to_le_bytes());
        req.extend_from_slice(&entry.len.to_le_bytes());
        req.extend_from_slice(&0u32.to_le_bytes());
    }
    req
}

pub(super) fn map_blob_req(resource_id: u32, offset: u64) -> Vec<u8> {
    let mut req = ctrl_req(VIRTIO_GPU_CMD_RESOURCE_MAP_BLOB, 0);
    req.extend_from_slice(&resource_id.to_le_bytes());
    req.extend_from_slice(&0u32.to_le_bytes());
    req.extend_from_slice(&offset.to_le_bytes());
    req
}

pub(super) fn unmap_blob_req(resource_id: u32) -> Vec<u8> {
    let mut req = ctrl_req(VIRTIO_GPU_CMD_RESOURCE_UNMAP_BLOB, 0);
    req.extend_from_slice(&resource_id.to_le_bytes());
    req.extend_from_slice(&0u32.to_le_bytes());
    req
}

pub(super) fn ctx_resource_req(typ: u32, ctx_id: u32, resource_id: u32) -> Vec<u8> {
    let mut req = ctrl_req(typ, ctx_id);
    req.extend_from_slice(&resource_id.to_le_bytes());
    req.extend_from_slice(&0u32.to_le_bytes());
    req
}
