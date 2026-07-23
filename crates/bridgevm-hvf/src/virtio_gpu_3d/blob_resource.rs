//! Blob resource creation, iovec resolution, and backing bookkeeping.

use super::*;
use crate::fwcfg::GuestMemoryMut;

pub(crate) fn resolve_blob_iovecs_into(
    mem: &dyn GuestMemoryMut,
    backing: &[BlobMemEntry],
    out: &mut Vec<BlobHostIovec>,
) -> bool {
    let start = out.len();
    out.reserve(backing.len());
    for entry in backing {
        let len = entry.len as usize;
        let Some(host_ptr) = mem.host_ptr(entry.addr, len) else {
            out.truncate(start);
            return false;
        };
        if host_ptr.is_null() {
            out.truncate(start);
            return false;
        }
        out.push(BlobHostIovec { host_ptr, len });
    }
    true
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlobMemEntry {
    pub addr: u64,
    pub len: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlobResourceInfo {
    pub blob_mem: u32,
    pub size: u64,
    pub backing: Vec<BlobMemEntry>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct BlobResourceInfoRef<'a> {
    pub blob_mem: u32,
    pub size: u64,
    pub backing: &'a [BlobMemEntry],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BlobResource {
    pub(crate) blob_mem: u32,
    pub(crate) size: u64,
    pub(crate) mapped: Option<(u64, usize)>,
    pub(crate) backing: Vec<BlobMemEntry>,
}

impl VirtioGpu3d {
    pub fn blob_resource_info(&self, resource_id: u32) -> Option<BlobResourceInfo> {
        let info = self.blob_resource_info_ref(resource_id)?;
        Some(BlobResourceInfo {
            blob_mem: info.blob_mem,
            size: info.size,
            backing: info.backing.to_vec(),
        })
    }

    pub(crate) fn blob_resource_info_ref(
        &self,
        resource_id: u32,
    ) -> Option<BlobResourceInfoRef<'_>> {
        let resource = self.blob_resources.get(&resource_id)?;
        Some(BlobResourceInfoRef {
            blob_mem: resource.blob_mem,
            size: resource.size,
            backing: &resource.backing,
        })
    }

    pub(crate) fn resource_create_blob_into(
        &mut self,
        mem: Option<&dyn GuestMemoryMut>,
        request: &[u8],
        hdr: CtrlHdr3d,
        out: &mut Vec<u8>,
    ) {
        if request.len() < RESOURCE_CREATE_BLOB_LEN {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        }
        if self.backend.is_none() {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        }
        let resource_id = read_le_u32(request, 24).unwrap_or(0);
        let blob_mem = read_le_u32(request, 28).unwrap_or(0);
        let blob_flags = read_le_u32(request, 32).unwrap_or(0);
        let nr_entries = read_le_u32(request, 36).unwrap_or(0);
        let blob_id = read_le_u64(request, 40).unwrap_or(0);
        let size = read_le_u64(request, 48).unwrap_or(0);
        if resource_id == 0 || size == 0 || self.blob_resources.contains_key(&resource_id) {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        }
        if blob_mem == VIRTIO_GPU_BLOB_MEM_HOST3D_GUEST {
            venus_start_trace_reject("create_blob", "blob_mem HOST3D_GUEST unsupported");
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        }
        if blob_mem != VIRTIO_GPU_BLOB_MEM_HOST3D && blob_mem != VIRTIO_GPU_BLOB_MEM_GUEST {
            venus_start_trace_reject("create_blob", "blob_mem invalid");
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        }
        let Some(entries_len) = (nr_entries as usize).checked_mul(MEM_ENTRY_LEN) else {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        };
        if request.len().saturating_sub(RESOURCE_CREATE_BLOB_LEN) < entries_len {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        }
        let mut backing = Vec::with_capacity(nr_entries as usize);
        let mut offset = RESOURCE_CREATE_BLOB_LEN;
        for _ in 0..nr_entries {
            let Some(addr) = read_le_u64(request, offset) else {
                response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
                return;
            };
            let Some(len) = read_le_u32(request, offset + 8) else {
                response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
                return;
            };
            backing.push(BlobMemEntry { addr, len });
            offset += MEM_ENTRY_LEN;
        }
        self.host_iovecs_scratch.clear();
        if blob_mem == VIRTIO_GPU_BLOB_MEM_GUEST {
            let Some(mem) = mem else {
                response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
                return;
            };
            if !resolve_blob_iovecs_into(mem, &backing, &mut self.host_iovecs_scratch) {
                response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
                return;
            }
        }
        if blob_mem == VIRTIO_GPU_BLOB_MEM_HOST3D || blob_mem == VIRTIO_GPU_BLOB_MEM_GUEST {
            let args = CreateBlobArgs {
                ctx_id: hdr.ctx_id,
                resource_id,
                blob_mem,
                blob_flags,
                blob_id,
                size,
                iovecs: &self.host_iovecs_scratch,
            };
            let created = self.backend.as_mut().unwrap().create_blob(args);
            self.host_iovecs_scratch.clear();
            if !created {
                venus_start_trace_reject("create_blob", "backend create_blob failed");
                response_hdr_into(out, VIRTIO_GPU_RESP_ERR_UNSPEC, Some(hdr));
                return;
            }
        }
        // A reused id starts a new lifecycle; stale destroyed-id classification
        // must not label its future late unmaps.
        self.destroyed_blob_mapped_ids.remove(&resource_id);
        self.destroyed_blob_unmapped_ids.remove(&resource_id);
        self.blob_resources.insert(
            resource_id,
            BlobResource {
                blob_mem,
                size,
                mapped: None,
                backing,
            },
        );
        response_hdr_into(out, VIRTIO_GPU_RESP_OK_NODATA, Some(hdr));
    }
}
