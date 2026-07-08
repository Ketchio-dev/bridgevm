use std::collections::{BTreeMap, BTreeSet};

use crate::fwcfg::GuestMemoryMut;

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

const CTRL_HDR_LEN: usize = 24;
const CTX_CREATE_LEN: usize = 24 + 4 + 4 + 64;
const CTX_RESOURCE_LEN: usize = 24 + 4 + 4;
const SUBMIT_3D_LEN: usize = 24 + 4 + 4;
const RESOURCE_CREATE_BLOB_LEN: usize = 24 + 4 + 4 + 4 + 4 + 8 + 8;
const RESOURCE_MAP_BLOB_LEN: usize = 24 + 4 + 4 + 8;
const RESOURCE_UNMAP_BLOB_LEN: usize = 24 + 4 + 4;
const MEM_ENTRY_LEN: usize = 16;
const MAX_SUBMIT_3D_BYTES: usize = 4 * 1024 * 1024;
const HVF_PAGE_SIZE: u64 = 16 * 1024;

pub trait GpuShmMapPort: Send {
    fn map(&mut self, host_ptr: *mut u8, size: usize, shm_offset: u64) -> Result<(), i32>;
    fn unmap(&mut self, shm_offset: u64, size: usize) -> Result<(), i32>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CapsetInfo {
    pub capset_id: u32,
    pub max_version: u32,
    pub max_size: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompletedFence {
    pub ctx_id: u32,
    pub ring_idx: u8,
    pub fence_id: u64,
}

pub trait VirtioGpu3dBackend: Send {
    fn capset_count(&self) -> u32 {
        1
    }
    fn capset_info(&mut self, capset_index: u32) -> Option<CapsetInfo>;
    fn capset(&mut self, capset_id: u32, version: u32) -> Option<Vec<u8>>;
    fn ctx_create(&mut self, ctx_id: u32, context_init: u32, name: &[u8]) -> bool;
    fn ctx_destroy(&mut self, ctx_id: u32);
    fn ctx_attach_resource(&mut self, ctx_id: u32, resource_id: u32);
    fn ctx_detach_resource(&mut self, ctx_id: u32, resource_id: u32);
    fn submit_3d(&mut self, ctx_id: u32, cmdbuf: &[u8]) -> bool;
    fn create_blob(&mut self, args: CreateBlobArgs<'_>) -> bool;
    fn map_blob(&mut self, resource_id: u32) -> Option<MappedBlob>;
    fn unmap_blob(&mut self, resource_id: u32);
    fn scanout_map(&mut self, resource_id: u32) -> Option<ScanoutMappedBlob>;
    fn scanout_unmap(&mut self, resource_id: u32);
    fn destroy_resource(&mut self, resource_id: u32);
    fn create_fence(&mut self, ctx_id: u32, ring_idx: u8, fence_id: u64) -> bool;
    fn poll_fences(&mut self);
    fn drain_completed_fences(&mut self) -> Vec<CompletedFence>;
    fn reset(&mut self);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlobMemEntry {
    pub addr: u64,
    pub len: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct CreateBlobArgs<'a> {
    pub ctx_id: u32,
    pub resource_id: u32,
    pub blob_mem: u32,
    pub blob_flags: u32,
    pub blob_id: u64,
    pub size: u64,
    pub iovecs: &'a [BlobHostIovec],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlobHostIovec {
    pub host_ptr: *mut u8,
    pub len: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MappedBlob {
    pub host_ptr: *mut u8,
    pub size: usize,
    pub map_info: u32,
}

unsafe impl Send for MappedBlob {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScanoutMappedBlob {
    pub host_ptr: *const u8,
    pub size: usize,
}

unsafe impl Send for ScanoutMappedBlob {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlobResourceInfo {
    pub blob_mem: u32,
    pub size: u64,
    pub backing: Vec<BlobMemEntry>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct VirtioGpu3dStats {
    pub ctx_active: usize,
    pub submits: u64,
    pub fences_pending: usize,
    pub fences_completed: u64,
}

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

#[derive(Default)]
pub struct VirtioGpu3d {
    backend: Option<Box<dyn VirtioGpu3dBackend>>,
    shm_port: Option<Box<dyn GpuShmMapPort>>,
    shm_window_size: u64,
    live_contexts: BTreeSet<u32>,
    ctx_resources: BTreeMap<u32, BTreeSet<u32>>,
    resource_2d_ids: BTreeSet<u32>,
    blob_resources: BTreeMap<u32, BlobResource>,
    mapped_intervals: BTreeMap<u64, (u64, u32)>,
    submits: u64,
    fences_completed: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BlobResource {
    blob_mem: u32,
    size: u64,
    mapped: Option<(u64, usize)>,
    backing: Vec<BlobMemEntry>,
}

impl std::fmt::Debug for VirtioGpu3d {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VirtioGpu3d")
            .field("has_backend", &self.backend.is_some())
            .field("has_shm_port", &self.shm_port.is_some())
            .field("shm_window_size", &self.shm_window_size)
            .field("live_contexts", &self.live_contexts)
            .field("ctx_resources", &self.ctx_resources)
            .field("resource_2d_ids", &self.resource_2d_ids)
            .field("blob_resources", &self.blob_resources)
            .field("submits", &self.submits)
            .field("fences_completed", &self.fences_completed)
            .finish()
    }
}

impl VirtioGpu3d {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_backend(backend: Box<dyn VirtioGpu3dBackend>) -> Self {
        Self {
            backend: Some(backend),
            ..Self::default()
        }
    }

    pub fn set_shm_map_port(&mut self, port: Box<dyn GpuShmMapPort>, window_size: u64) {
        self.shm_port = Some(port);
        self.shm_window_size = window_size;
    }

    pub fn has_backend(&self) -> bool {
        self.backend.is_some()
    }

    pub fn capset_count(&self) -> u32 {
        self.backend
            .as_ref()
            .map_or(0, |backend| backend.capset_count())
    }

    pub fn stats(&self, fences_pending: usize) -> VirtioGpu3dStats {
        VirtioGpu3dStats {
            ctx_active: self.live_contexts.len(),
            submits: self.submits,
            fences_pending,
            fences_completed: self.fences_completed,
        }
    }

    pub fn reset(&mut self) {
        if let Some(backend) = self.backend.as_mut() {
            backend.reset();
        }
        self.live_contexts.clear();
        self.ctx_resources.clear();
        self.resource_2d_ids.clear();
        self.unmap_all_blobs();
        self.blob_resources.clear();
        self.mapped_intervals.clear();
        self.submits = 0;
    }

    pub fn unref_resource(&mut self, resource_id: u32) {
        self.resource_2d_ids.remove(&resource_id);
        if self.blob_resources.contains_key(&resource_id) {
            self.unmap_blob_resource(resource_id);
            self.blob_resources.remove(&resource_id);
            self.mapped_intervals
                .retain(|_, (_, mapped_resource)| *mapped_resource != resource_id);
            if let Some(backend) = self.backend.as_mut() {
                backend.destroy_resource(resource_id);
            }
        }
    }

    pub fn blob_resource_info(&self, resource_id: u32) -> Option<BlobResourceInfo> {
        let resource = self.blob_resources.get(&resource_id)?;
        Some(BlobResourceInfo {
            blob_mem: resource.blob_mem,
            size: resource.size,
            backing: resource.backing.clone(),
        })
    }

    pub fn scanout_map_blob(&mut self, resource_id: u32) -> Option<ScanoutMappedBlob> {
        self.backend.as_mut()?.scanout_map(resource_id)
    }

    pub fn scanout_unmap_blob(&mut self, resource_id: u32) {
        if let Some(backend) = self.backend.as_mut() {
            backend.scanout_unmap(resource_id);
        }
    }

    pub fn ctx_has_resource(&self, ctx_id: u32, resource_id: u32) -> bool {
        self.ctx_resources
            .get(&ctx_id)
            .is_some_and(|resources| resources.contains(&resource_id))
    }

    pub fn register_2d_resource(&mut self, resource_id: u32) {
        if resource_id != 0 {
            self.resource_2d_ids.insert(resource_id);
        }
    }

    pub fn drain_completed_fences(&mut self) -> Vec<CompletedFence> {
        let Some(backend) = self.backend.as_mut() else {
            return Vec::new();
        };
        // Venus on macOS retires fences synchronously: polling the backend may
        // invoke the fence callback inline, then drain_completed_fences takes
        // the callbacks queued by that poll.
        backend.poll_fences();
        let completed = backend.drain_completed_fences();
        self.fences_completed = self.fences_completed.saturating_add(completed.len() as u64);
        completed
    }

    pub fn create_fence(&mut self, fence: CompletedFence) -> bool {
        let Some(backend) = self.backend.as_mut() else {
            return false;
        };
        backend.create_fence(fence.ctx_id, fence.ring_idx, fence.fence_id)
    }

    pub fn handle(&mut self, request: &[u8], hdr: CtrlHdr3d) -> Option<Vec<u8>> {
        self.handle_with_mem(None, request, hdr)
    }

    pub fn handle_with_mem(
        &mut self,
        mem: Option<&dyn GuestMemoryMut>,
        request: &[u8],
        hdr: CtrlHdr3d,
    ) -> Option<Vec<u8>> {
        match hdr.typ {
            VIRTIO_GPU_CMD_GET_CAPSET_INFO => Some(self.get_capset_info(request, hdr)),
            VIRTIO_GPU_CMD_GET_CAPSET => Some(self.get_capset(request, hdr)),
            VIRTIO_GPU_CMD_RESOURCE_CREATE_BLOB => {
                Some(self.resource_create_blob(mem, request, hdr))
            }
            VIRTIO_GPU_CMD_CTX_CREATE => Some(self.ctx_create(request, hdr)),
            VIRTIO_GPU_CMD_CTX_DESTROY => Some(self.ctx_destroy(hdr)),
            VIRTIO_GPU_CMD_CTX_ATTACH_RESOURCE => Some(self.ctx_attach_resource(request, hdr)),
            VIRTIO_GPU_CMD_CTX_DETACH_RESOURCE => Some(self.ctx_detach_resource(request, hdr)),
            VIRTIO_GPU_CMD_SUBMIT_3D => Some(self.submit_3d(request, hdr)),
            VIRTIO_GPU_CMD_RESOURCE_MAP_BLOB => Some(self.resource_map_blob(request, hdr)),
            VIRTIO_GPU_CMD_RESOURCE_UNMAP_BLOB => Some(self.resource_unmap_blob(request, hdr)),
            _ => None,
        }
    }

    fn get_capset_info(&mut self, request: &[u8], hdr: CtrlHdr3d) -> Vec<u8> {
        let Some(backend) = self.backend.as_mut() else {
            return response_hdr(VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
        };
        let Some(index) = read_le_u32(request, 24) else {
            return response_hdr(VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
        };
        let Some(info) = backend.capset_info(index) else {
            return response_hdr(VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
        };
        let mut out = response_hdr(VIRTIO_GPU_RESP_OK_CAPSET_INFO, Some(hdr));
        out.extend_from_slice(&info.capset_id.to_le_bytes());
        out.extend_from_slice(&info.max_version.to_le_bytes());
        out.extend_from_slice(&info.max_size.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out
    }

    fn get_capset(&mut self, request: &[u8], hdr: CtrlHdr3d) -> Vec<u8> {
        let Some(backend) = self.backend.as_mut() else {
            return response_hdr(VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
        };
        let Some(capset_id) = read_le_u32(request, 24) else {
            return response_hdr(VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
        };
        let Some(version) = read_le_u32(request, 28) else {
            return response_hdr(VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
        };
        let Some(bytes) = backend.capset(capset_id, version) else {
            return response_hdr(VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
        };
        let mut out = response_hdr(VIRTIO_GPU_RESP_OK_CAPSET, Some(hdr));
        out.extend_from_slice(&bytes);
        out
    }

    fn ctx_create(&mut self, request: &[u8], hdr: CtrlHdr3d) -> Vec<u8> {
        let Some(backend) = self.backend.as_mut() else {
            return response_hdr(VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
        };
        if request.len() < CTX_CREATE_LEN
            || hdr.ctx_id == 0
            || self.live_contexts.contains(&hdr.ctx_id)
        {
            return response_hdr(VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
        }
        let nlen = read_le_u32(request, 24).unwrap_or(0).min(64) as usize;
        let context_init = read_le_u32(request, 28).unwrap_or(0);
        let name = &request[32..32 + nlen];
        if !backend.ctx_create(hdr.ctx_id, context_init, name) {
            return response_hdr(VIRTIO_GPU_RESP_ERR_UNSPEC, Some(hdr));
        }
        self.live_contexts.insert(hdr.ctx_id);
        self.ctx_resources.entry(hdr.ctx_id).or_default();
        response_hdr(VIRTIO_GPU_RESP_OK_NODATA, Some(hdr))
    }

    fn ctx_destroy(&mut self, hdr: CtrlHdr3d) -> Vec<u8> {
        if !self.live_contexts.remove(&hdr.ctx_id) {
            return response_hdr(VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
        }
        self.ctx_resources.remove(&hdr.ctx_id);
        if let Some(backend) = self.backend.as_mut() {
            backend.ctx_destroy(hdr.ctx_id);
        }
        response_hdr(VIRTIO_GPU_RESP_OK_NODATA, Some(hdr))
    }

    fn ctx_attach_resource(&mut self, request: &[u8], hdr: CtrlHdr3d) -> Vec<u8> {
        if request.len() < CTX_RESOURCE_LEN {
            return response_hdr(VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
        }
        let resource_id = read_le_u32(request, 24).unwrap_or(0);
        if !self.resource_exists(resource_id) {
            return response_hdr(VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
        }
        if let Some(resources) = self.ctx_resources.get_mut(&hdr.ctx_id) {
            resources.insert(resource_id);
        }
        if let Some(backend) = self.backend.as_mut() {
            backend.ctx_attach_resource(hdr.ctx_id, resource_id);
        }
        response_hdr(VIRTIO_GPU_RESP_OK_NODATA, Some(hdr))
    }

    fn ctx_detach_resource(&mut self, request: &[u8], hdr: CtrlHdr3d) -> Vec<u8> {
        if request.len() < CTX_RESOURCE_LEN {
            return response_hdr(VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
        }
        let resource_id = read_le_u32(request, 24).unwrap_or(0);
        if !self.resource_exists(resource_id) {
            return response_hdr(VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
        }
        if let Some(resources) = self.ctx_resources.get_mut(&hdr.ctx_id) {
            resources.remove(&resource_id);
        }
        if let Some(backend) = self.backend.as_mut() {
            backend.ctx_detach_resource(hdr.ctx_id, resource_id);
        }
        response_hdr(VIRTIO_GPU_RESP_OK_NODATA, Some(hdr))
    }

    fn submit_3d(&mut self, request: &[u8], hdr: CtrlHdr3d) -> Vec<u8> {
        let Some(backend) = self.backend.as_mut() else {
            return response_hdr(VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
        };
        if !self.live_contexts.contains(&hdr.ctx_id) || request.len() < SUBMIT_3D_LEN {
            return response_hdr(VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
        }
        let size = read_le_u32(request, 24).unwrap_or(0) as usize;
        if size > MAX_SUBMIT_3D_BYTES || request.len().saturating_sub(SUBMIT_3D_LEN) < size {
            return response_hdr(VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
        }
        let cmdbuf = &request[SUBMIT_3D_LEN..SUBMIT_3D_LEN + size];
        if !backend.submit_3d(hdr.ctx_id, cmdbuf) {
            return response_hdr(VIRTIO_GPU_RESP_ERR_UNSPEC, Some(hdr));
        }
        self.submits = self.submits.saturating_add(1);
        response_hdr(VIRTIO_GPU_RESP_OK_NODATA, Some(hdr))
    }

    fn resource_create_blob(
        &mut self,
        mem: Option<&dyn GuestMemoryMut>,
        request: &[u8],
        hdr: CtrlHdr3d,
    ) -> Vec<u8> {
        if request.len() < RESOURCE_CREATE_BLOB_LEN {
            return response_hdr(VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
        }
        if self.backend.is_none() {
            return response_hdr(VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
        }
        let resource_id = read_le_u32(request, 24).unwrap_or(0);
        let blob_mem = read_le_u32(request, 28).unwrap_or(0);
        let blob_flags = read_le_u32(request, 32).unwrap_or(0);
        let nr_entries = read_le_u32(request, 36).unwrap_or(0);
        let blob_id = read_le_u64(request, 40).unwrap_or(0);
        let size = read_le_u64(request, 48).unwrap_or(0);
        if resource_id == 0 || size == 0 || self.blob_resources.contains_key(&resource_id) {
            return response_hdr(VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
        }
        if blob_mem == VIRTIO_GPU_BLOB_MEM_HOST3D_GUEST {
            return response_hdr(VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
        }
        if blob_mem != VIRTIO_GPU_BLOB_MEM_HOST3D && blob_mem != VIRTIO_GPU_BLOB_MEM_GUEST {
            return response_hdr(VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
        }
        let Some(entries_len) = (nr_entries as usize).checked_mul(MEM_ENTRY_LEN) else {
            return response_hdr(VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
        };
        if request.len().saturating_sub(RESOURCE_CREATE_BLOB_LEN) < entries_len {
            return response_hdr(VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
        }
        let mut backing = Vec::with_capacity(nr_entries as usize);
        let mut offset = RESOURCE_CREATE_BLOB_LEN;
        for _ in 0..nr_entries {
            let Some(addr) = read_le_u64(request, offset) else {
                return response_hdr(VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            };
            let Some(len) = read_le_u32(request, offset + 8) else {
                return response_hdr(VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            };
            backing.push(BlobMemEntry { addr, len });
            offset += MEM_ENTRY_LEN;
        }
        let host_iovecs = if blob_mem == VIRTIO_GPU_BLOB_MEM_GUEST {
            let Some(mem) = mem else {
                return response_hdr(VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            };
            let Some(iovecs) = resolve_blob_iovecs(mem, &backing) else {
                return response_hdr(VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            };
            iovecs
        } else {
            Vec::new()
        };
        if blob_mem == VIRTIO_GPU_BLOB_MEM_HOST3D || blob_mem == VIRTIO_GPU_BLOB_MEM_GUEST {
            let args = CreateBlobArgs {
                ctx_id: hdr.ctx_id,
                resource_id,
                blob_mem,
                blob_flags,
                blob_id,
                size,
                iovecs: &host_iovecs,
            };
            if !self.backend.as_mut().unwrap().create_blob(args) {
                return response_hdr(VIRTIO_GPU_RESP_ERR_UNSPEC, Some(hdr));
            }
        }
        self.blob_resources.insert(
            resource_id,
            BlobResource {
                blob_mem,
                size,
                mapped: None,
                backing,
            },
        );
        response_hdr(VIRTIO_GPU_RESP_OK_NODATA, Some(hdr))
    }

    fn resource_map_blob(&mut self, request: &[u8], hdr: CtrlHdr3d) -> Vec<u8> {
        if request.len() < RESOURCE_MAP_BLOB_LEN {
            return response_hdr(VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
        }
        let resource_id = read_le_u32(request, 24).unwrap_or(0);
        let shm_offset = read_le_u64(request, 32).unwrap_or(u64::MAX);
        let Some(resource) = self.blob_resources.get(&resource_id) else {
            return response_hdr(VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
        };
        if resource.mapped.is_some() || resource.blob_mem != VIRTIO_GPU_BLOB_MEM_HOST3D {
            return response_hdr(VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
        }
        let size = resource.size;
        // Validate against the page-rounded footprint the mapping will occupy.
        let rounded_size = round_up_usize(size as usize, HVF_PAGE_SIZE as usize) as u64;
        if !aligned_u64(shm_offset, HVF_PAGE_SIZE)
            || shm_offset
                .checked_add(rounded_size)
                .map_or(true, |end| end > self.shm_window_size)
            || self.interval_overlaps(shm_offset, rounded_size)
        {
            return response_hdr(VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
        }
        let Some(backend) = self.backend.as_mut() else {
            return response_hdr(VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
        };
        let Some(mapped) = backend.map_blob(resource_id) else {
            return response_hdr(VIRTIO_GPU_RESP_ERR_OUT_OF_MEMORY, Some(hdr));
        };
        // Guests may create blobs at their own (4 KiB) page granularity while
        // hv_vm_map needs 16 KiB pages. The host allocation backing a Vulkan
        // mapping is vm-page (16 KiB) granular on macOS, so it is safe to map
        // the blob's pages rounded up to the HVF page size as long as the host
        // pointer itself is page-aligned; the guest-visible blob size stays
        // `size`.
        let map_size = rounded_size as usize;
        if mapped.host_ptr.is_null()
            || !aligned_usize(mapped.host_ptr as usize, HVF_PAGE_SIZE as usize)
            || (mapped.size as u64) < size
        {
            backend.unmap_blob(resource_id);
            return response_hdr(VIRTIO_GPU_RESP_ERR_OUT_OF_MEMORY, Some(hdr));
        }
        let Some(port) = self.shm_port.as_mut() else {
            backend.unmap_blob(resource_id);
            return response_hdr(VIRTIO_GPU_RESP_ERR_OUT_OF_MEMORY, Some(hdr));
        };
        if port.map(mapped.host_ptr, map_size, shm_offset).is_err() {
            backend.unmap_blob(resource_id);
            return response_hdr(VIRTIO_GPU_RESP_ERR_OUT_OF_MEMORY, Some(hdr));
        }
        if let Some(resource) = self.blob_resources.get_mut(&resource_id) {
            resource.mapped = Some((shm_offset, map_size));
        }
        self.mapped_intervals
            .insert(shm_offset, (map_size as u64, resource_id));
        let mut out = response_hdr(VIRTIO_GPU_RESP_OK_MAP_INFO, Some(hdr));
        out.extend_from_slice(&(mapped.map_info & VIRTIO_GPU_MAP_CACHE_MASK).to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out
    }

    fn resource_unmap_blob(&mut self, request: &[u8], hdr: CtrlHdr3d) -> Vec<u8> {
        if request.len() < RESOURCE_UNMAP_BLOB_LEN {
            return response_hdr(VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
        }
        let resource_id = read_le_u32(request, 24).unwrap_or(0);
        if !self.blob_resources.contains_key(&resource_id) {
            return response_hdr(VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
        }
        self.unmap_blob_resource(resource_id);
        response_hdr(VIRTIO_GPU_RESP_OK_NODATA, Some(hdr))
    }

    fn unmap_blob_resource(&mut self, resource_id: u32) {
        let Some((shm_offset, mapped_size)) = self
            .blob_resources
            .get_mut(&resource_id)
            .and_then(|resource| resource.mapped.take())
        else {
            return;
        };
        if let Some(port) = self.shm_port.as_mut() {
            let _ = port.unmap(shm_offset, mapped_size);
        }
        if let Some(backend) = self.backend.as_mut() {
            backend.unmap_blob(resource_id);
        }
        self.mapped_intervals.remove(&shm_offset);
    }

    fn unmap_all_blobs(&mut self) {
        let ids: Vec<u32> = self.blob_resources.keys().copied().collect();
        for resource_id in ids {
            self.unmap_blob_resource(resource_id);
        }
    }

    fn interval_overlaps(&self, start: u64, size: u64) -> bool {
        let Some(end) = start.checked_add(size) else {
            return true;
        };
        self.mapped_intervals
            .iter()
            .any(|(other_start, (other_size, _))| {
                let other_end = other_start.saturating_add(*other_size);
                start < other_end && *other_start < end
            })
    }

    fn resource_exists(&self, resource_id: u32) -> bool {
        resource_id != 0
            && (self.resource_2d_ids.contains(&resource_id)
                || self.blob_resources.contains_key(&resource_id))
    }
}

pub fn response_hdr(typ: u32, request: Option<CtrlHdr3d>) -> Vec<u8> {
    let mut out = Vec::with_capacity(CTRL_HDR_LEN);
    let (flags, fence_id, ctx_id, padding) = request.map_or((0, 0, 0, 0), |hdr| {
        (
            hdr.flags & (VIRTIO_GPU_FLAG_FENCE | VIRTIO_GPU_FLAG_INFO_RING_IDX),
            if hdr.fenced() { hdr.fence_id } else { 0 },
            hdr.ctx_id,
            hdr.padding,
        )
    });
    out.extend_from_slice(&typ.to_le_bytes());
    out.extend_from_slice(&flags.to_le_bytes());
    out.extend_from_slice(&fence_id.to_le_bytes());
    out.extend_from_slice(&ctx_id.to_le_bytes());
    out.extend_from_slice(&padding.to_le_bytes());
    out
}

fn read_le_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

fn read_le_u64(bytes: &[u8], offset: usize) -> Option<u64> {
    Some(u64::from_le_bytes(
        bytes.get(offset..offset + 8)?.try_into().ok()?,
    ))
}

fn round_up_usize(value: usize, align: usize) -> usize {
    value.div_ceil(align) * align
}

fn aligned_u64(value: u64, align: u64) -> bool {
    value % align == 0
}

fn aligned_usize(value: usize, align: usize) -> bool {
    value % align == 0
}

fn resolve_blob_iovecs(
    mem: &dyn GuestMemoryMut,
    backing: &[BlobMemEntry],
) -> Option<Vec<BlobHostIovec>> {
    let mut iovecs = Vec::with_capacity(backing.len());
    for entry in backing {
        let len = entry.len as usize;
        let host_ptr = mem.host_ptr(entry.addr, len)?;
        if host_ptr.is_null() {
            return None;
        }
        iovecs.push(BlobHostIovec { host_ptr, len });
    }
    Some(iovecs)
}

#[cfg(test)]
#[derive(Debug, Default)]
pub struct MockBackend {
    pub capset_info: Option<CapsetInfo>,
    pub capset: Vec<u8>,
    pub created: Vec<(u32, u32, Vec<u8>)>,
    pub destroyed: Vec<u32>,
    pub attached: Vec<(u32, u32)>,
    pub detached: Vec<(u32, u32)>,
    pub submits: Vec<(u32, Vec<u8>)>,
    pub blobs: Vec<(u32, u32, u64, u64)>,
    pub blob_iovecs: Vec<(u32, usize, usize)>,
    pub mapped: BTreeMap<u32, MappedBlob>,
    pub unmapped: Vec<u32>,
    pub destroyed_resources: Vec<u32>,
    pub fences: Vec<CompletedFence>,
    pub completed: Vec<CompletedFence>,
    pub reject_fence_ring: Option<u8>,
}

#[cfg(test)]
impl MockBackend {
    pub fn new_venus() -> Self {
        let mut capset = vec![0u8; 160];
        capset[0..4].copy_from_slice(&1u32.to_le_bytes());
        Self {
            capset_info: Some(CapsetInfo {
                capset_id: 4,
                max_version: 1,
                max_size: 160,
            }),
            capset,
            ..Self::default()
        }
    }
}

#[cfg(test)]
impl VirtioGpu3dBackend for std::sync::Arc<std::sync::Mutex<MockBackend>> {
    fn capset_info(&mut self, capset_index: u32) -> Option<CapsetInfo> {
        (capset_index == 0)
            .then(|| self.lock().unwrap().capset_info)
            .flatten()
    }

    fn capset(&mut self, capset_id: u32, _version: u32) -> Option<Vec<u8>> {
        let inner = self.lock().unwrap();
        (inner.capset_info.map(|info| info.capset_id) == Some(capset_id))
            .then(|| inner.capset.clone())
    }

    fn ctx_create(&mut self, ctx_id: u32, context_init: u32, name: &[u8]) -> bool {
        self.lock()
            .unwrap()
            .created
            .push((ctx_id, context_init, name.to_vec()));
        true
    }

    fn ctx_destroy(&mut self, ctx_id: u32) {
        self.lock().unwrap().destroyed.push(ctx_id);
    }

    fn ctx_attach_resource(&mut self, ctx_id: u32, resource_id: u32) {
        self.lock().unwrap().attached.push((ctx_id, resource_id));
    }

    fn ctx_detach_resource(&mut self, ctx_id: u32, resource_id: u32) {
        self.lock().unwrap().detached.push((ctx_id, resource_id));
    }

    fn submit_3d(&mut self, ctx_id: u32, cmdbuf: &[u8]) -> bool {
        self.lock().unwrap().submits.push((ctx_id, cmdbuf.to_vec()));
        true
    }

    fn create_blob(&mut self, args: CreateBlobArgs<'_>) -> bool {
        let mut inner = self.lock().unwrap();
        inner
            .blobs
            .push((args.resource_id, args.blob_mem, args.blob_id, args.size));
        inner.blob_iovecs.push((
            args.resource_id,
            args.iovecs.len(),
            args.iovecs.iter().map(|iov| iov.len).sum(),
        ));
        true
    }

    fn map_blob(&mut self, resource_id: u32) -> Option<MappedBlob> {
        self.lock().unwrap().mapped.get(&resource_id).copied()
    }

    fn unmap_blob(&mut self, resource_id: u32) {
        self.lock().unwrap().unmapped.push(resource_id);
    }

    fn scanout_map(&mut self, resource_id: u32) -> Option<ScanoutMappedBlob> {
        self.lock()
            .unwrap()
            .mapped
            .get(&resource_id)
            .map(|mapped| ScanoutMappedBlob {
                host_ptr: mapped.host_ptr.cast_const(),
                size: mapped.size,
            })
    }

    fn scanout_unmap(&mut self, resource_id: u32) {
        self.lock().unwrap().unmapped.push(resource_id);
    }

    fn destroy_resource(&mut self, resource_id: u32) {
        self.lock().unwrap().destroyed_resources.push(resource_id);
    }

    fn create_fence(&mut self, ctx_id: u32, ring_idx: u8, fence_id: u64) -> bool {
        let mut inner = self.lock().unwrap();
        inner.fences.push(CompletedFence {
            ctx_id,
            ring_idx,
            fence_id,
        });
        inner.reject_fence_ring != Some(ring_idx)
    }

    fn poll_fences(&mut self) {}

    fn drain_completed_fences(&mut self) -> Vec<CompletedFence> {
        std::mem::take(&mut self.lock().unwrap().completed)
    }

    fn reset(&mut self) {
        self.lock().unwrap().completed.clear();
    }
}

#[cfg(test)]
#[derive(Debug, Default)]
struct MockMapPort {
    maps: Vec<(usize, usize, u64)>,
    unmaps: Vec<(u64, usize)>,
}

#[cfg(test)]
impl GpuShmMapPort for std::sync::Arc<std::sync::Mutex<MockMapPort>> {
    fn map(&mut self, host_ptr: *mut u8, size: usize, shm_offset: u64) -> Result<(), i32> {
        self.lock()
            .unwrap()
            .maps
            .push((host_ptr as usize, size, shm_offset));
        Ok(())
    }

    fn unmap(&mut self, shm_offset: u64, size: usize) -> Result<(), i32> {
        self.lock().unwrap().unmaps.push((shm_offset, size));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        alloc::{alloc_zeroed, Layout},
        sync::{Arc, Mutex},
    };

    #[test]
    fn host3d_blob_maps_through_shm_port_then_unmaps_before_unref() {
        let backend = Arc::new(Mutex::new(MockBackend::new_venus()));
        let port = Arc::new(Mutex::new(MockMapPort::default()));
        let layout = Layout::from_size_align(0x1_0000, HVF_PAGE_SIZE as usize).unwrap();
        let ptr = unsafe { alloc_zeroed(layout) };
        assert!(!ptr.is_null());
        backend.lock().unwrap().mapped.insert(
            7,
            MappedBlob {
                host_ptr: ptr,
                size: 0x1_0000,
                map_info: 0x13,
            },
        );

        let mut gpu = VirtioGpu3d::with_backend(Box::new(backend.clone()));
        gpu.set_shm_map_port(Box::new(port.clone()), 0x20_0000);

        let create = create_blob_req(7, VIRTIO_GPU_BLOB_MEM_HOST3D, 0, 0x1_0000, 1);
        let hdr = CtrlHdr3d::parse(&create).unwrap();
        assert_eq!(
            read_le_u32(&gpu.handle(&create, hdr).unwrap(), 0),
            Some(VIRTIO_GPU_RESP_OK_NODATA)
        );

        let map = map_blob_req(7, 0x4000);
        let hdr = CtrlHdr3d::parse(&map).unwrap();
        let response = gpu.handle(&map, hdr).unwrap();
        assert_eq!(read_le_u32(&response, 0), Some(VIRTIO_GPU_RESP_OK_MAP_INFO));
        assert_eq!(read_le_u32(&response, 24), Some(0x3));
        assert_eq!(
            port.lock().unwrap().maps,
            vec![(ptr as usize, 0x1_0000, 0x4000)]
        );

        let unmap = unmap_blob_req(7);
        let hdr = CtrlHdr3d::parse(&unmap).unwrap();
        assert_eq!(
            read_le_u32(&gpu.handle(&unmap, hdr).unwrap(), 0),
            Some(VIRTIO_GPU_RESP_OK_NODATA)
        );
        assert_eq!(port.lock().unwrap().unmaps, vec![(0x4000, 0x1_0000)]);

        gpu.unref_resource(7);
        assert_eq!(backend.lock().unwrap().destroyed_resources, vec![7]);
    }

    #[test]
    fn host3d_blob_map_rejects_zero_shm_window_without_shm_port() {
        let backend = Arc::new(Mutex::new(MockBackend::new_venus()));
        let mut gpu = VirtioGpu3d::with_backend(Box::new(backend.clone()));

        let create = create_blob_req(7, VIRTIO_GPU_BLOB_MEM_HOST3D, 0, 0x4000, 1);
        let hdr = CtrlHdr3d::parse(&create).unwrap();
        assert_eq!(
            read_le_u32(&gpu.handle(&create, hdr).unwrap(), 0),
            Some(VIRTIO_GPU_RESP_OK_NODATA)
        );

        let map = map_blob_req(7, 0);
        let hdr = CtrlHdr3d::parse(&map).unwrap();
        assert_eq!(
            read_le_u32(&gpu.handle(&map, hdr).unwrap(), 0),
            Some(VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER)
        );
        assert!(backend.lock().unwrap().unmapped.is_empty());

        let mut info = ctrl_req(VIRTIO_GPU_CMD_GET_CAPSET_INFO, 0);
        info.extend_from_slice(&0u32.to_le_bytes());
        info.extend_from_slice(&0u32.to_le_bytes());
        let hdr = CtrlHdr3d::parse(&info).unwrap();
        assert_eq!(
            read_le_u32(&gpu.handle(&info, hdr).unwrap(), 0),
            Some(VIRTIO_GPU_RESP_OK_CAPSET_INFO)
        );
    }

    #[test]
    fn ctx_attach_detach_blob_resource_without_live_context_forwards_to_backend() {
        let backend = Arc::new(Mutex::new(MockBackend::new_venus()));
        let mut gpu = VirtioGpu3d::with_backend(Box::new(backend.clone()));

        let create = create_blob_req(11, VIRTIO_GPU_BLOB_MEM_HOST3D, 44, 0x4000, 9);
        let hdr = CtrlHdr3d::parse(&create).unwrap();
        assert_eq!(
            read_le_u32(&gpu.handle(&create, hdr).unwrap(), 0),
            Some(VIRTIO_GPU_RESP_OK_NODATA)
        );

        let attach = ctx_resource_req(VIRTIO_GPU_CMD_CTX_ATTACH_RESOURCE, 27, 11);
        let hdr = CtrlHdr3d::parse(&attach).unwrap();
        assert_eq!(
            read_le_u32(&gpu.handle(&attach, hdr).unwrap(), 0),
            Some(VIRTIO_GPU_RESP_OK_NODATA)
        );

        let detach = ctx_resource_req(VIRTIO_GPU_CMD_CTX_DETACH_RESOURCE, 27, 11);
        let hdr = CtrlHdr3d::parse(&detach).unwrap();
        assert_eq!(
            read_le_u32(&gpu.handle(&detach, hdr).unwrap(), 0),
            Some(VIRTIO_GPU_RESP_OK_NODATA)
        );

        let backend = backend.lock().unwrap();
        assert_eq!(backend.attached, vec![(27, 11)]);
        assert_eq!(backend.detached, vec![(27, 11)]);
    }

    #[test]
    fn ctx_attach_unknown_resource_errors_without_forwarding() {
        let backend = Arc::new(Mutex::new(MockBackend::new_venus()));
        let mut gpu = VirtioGpu3d::with_backend(Box::new(backend.clone()));

        let attach = ctx_resource_req(VIRTIO_GPU_CMD_CTX_ATTACH_RESOURCE, 27, 99);
        let hdr = CtrlHdr3d::parse(&attach).unwrap();
        assert_eq!(
            read_le_u32(&gpu.handle(&attach, hdr).unwrap(), 0),
            Some(VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER)
        );

        assert!(backend.lock().unwrap().attached.is_empty());
    }

    #[test]
    fn ctx_attach_registered_2d_resource_forwards_to_backend() {
        let backend = Arc::new(Mutex::new(MockBackend::new_venus()));
        let mut gpu = VirtioGpu3d::with_backend(Box::new(backend.clone()));
        gpu.register_2d_resource(5);

        let attach = ctx_resource_req(VIRTIO_GPU_CMD_CTX_ATTACH_RESOURCE, 31, 5);
        let hdr = CtrlHdr3d::parse(&attach).unwrap();
        assert_eq!(
            read_le_u32(&gpu.handle(&attach, hdr).unwrap(), 0),
            Some(VIRTIO_GPU_RESP_OK_NODATA)
        );

        assert_eq!(backend.lock().unwrap().attached, vec![(31, 5)]);
    }

    #[test]
    fn guest_blob_create_forwards_resolved_iovecs_to_backend() {
        let backend = Arc::new(Mutex::new(MockBackend::new_venus()));
        let mut gpu = VirtioGpu3d::with_backend(Box::new(backend.clone()));
        let mem = TestMem::new(0x8000_0000, 0x20_000);

        let create = create_blob_req_with_entries(
            19,
            VIRTIO_GPU_BLOB_MEM_GUEST,
            77,
            0x3000,
            3,
            &[
                BlobMemEntry {
                    addr: 0x8000_1000,
                    len: 0x1000,
                },
                BlobMemEntry {
                    addr: 0x8000_4000,
                    len: 0x2000,
                },
            ],
        );
        let hdr = CtrlHdr3d::parse(&create).unwrap();

        assert_eq!(
            read_le_u32(&gpu.handle_with_mem(Some(&mem), &create, hdr).unwrap(), 0),
            Some(VIRTIO_GPU_RESP_OK_NODATA)
        );

        let backend = backend.lock().unwrap();
        assert_eq!(
            backend.blobs,
            vec![(19, VIRTIO_GPU_BLOB_MEM_GUEST, 77, 0x3000)]
        );
        assert_eq!(backend.blob_iovecs, vec![(19, 2, 0x3000)]);
    }

    #[derive(Debug)]
    struct TestMem {
        base: u64,
        bytes: Vec<u8>,
    }

    impl TestMem {
        fn new(base: u64, len: usize) -> Self {
            Self {
                base,
                bytes: vec![0; len],
            }
        }

        fn offset(&self, gpa: u64) -> Option<usize> {
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

    fn ctrl_req(typ: u32, ctx_id: u32) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&typ.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&0u64.to_le_bytes());
        out.extend_from_slice(&ctx_id.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out
    }

    fn create_blob_req(
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

    fn create_blob_req_with_entries(
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

    fn map_blob_req(resource_id: u32, offset: u64) -> Vec<u8> {
        let mut req = ctrl_req(VIRTIO_GPU_CMD_RESOURCE_MAP_BLOB, 0);
        req.extend_from_slice(&resource_id.to_le_bytes());
        req.extend_from_slice(&0u32.to_le_bytes());
        req.extend_from_slice(&offset.to_le_bytes());
        req
    }

    fn unmap_blob_req(resource_id: u32) -> Vec<u8> {
        let mut req = ctrl_req(VIRTIO_GPU_CMD_RESOURCE_UNMAP_BLOB, 0);
        req.extend_from_slice(&resource_id.to_le_bytes());
        req.extend_from_slice(&0u32.to_le_bytes());
        req
    }

    fn ctx_resource_req(typ: u32, ctx_id: u32, resource_id: u32) -> Vec<u8> {
        let mut req = ctrl_req(typ, ctx_id);
        req.extend_from_slice(&resource_id.to_le_bytes());
        req.extend_from_slice(&0u32.to_le_bytes());
        req
    }
}
