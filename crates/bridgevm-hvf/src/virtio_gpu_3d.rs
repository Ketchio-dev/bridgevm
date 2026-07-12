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

const CTRL_HDR_LEN: usize = 24;
const CTX_CREATE_LEN: usize = 24 + 4 + 4 + 64;
const CTX_RESOURCE_LEN: usize = 24 + 4 + 4;
const RESOURCE_CREATE_3D_LEN: usize = 24 + 12 * 4;
const TRANSFER_3D_LEN: usize = 24 + 6 * 4 + 8 + 4 * 4;
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
    fn capset_into(&mut self, capset_id: u32, version: u32, out: &mut Vec<u8>) -> bool {
        let Some(capset) = self.capset(capset_id, version) else {
            return false;
        };
        out.extend_from_slice(&capset);
        true
    }
    fn ctx_create(&mut self, ctx_id: u32, context_init: u32, name: &[u8]) -> bool;
    fn ctx_destroy(&mut self, ctx_id: u32);
    fn ctx_attach_resource(&mut self, ctx_id: u32, resource_id: u32);
    fn ctx_detach_resource(&mut self, ctx_id: u32, resource_id: u32);
    fn create_3d(&mut self, _args: Create3dArgs) -> bool {
        false
    }
    fn attach_backing(&mut self, _resource_id: u32, _iovecs: &[BlobHostIovec]) -> bool {
        false
    }
    fn detach_backing(&mut self, _resource_id: u32) -> bool {
        false
    }
    fn transfer_3d(&mut self, _args: Transfer3dArgs, _to_host: bool) -> bool {
        false
    }
    fn submit_3d(&mut self, ctx_id: u32, cmdbuf: &[u8]) -> bool;
    fn create_blob(&mut self, args: CreateBlobArgs<'_>) -> bool;
    fn map_blob(&mut self, resource_id: u32) -> Option<MappedBlob>;
    fn unmap_blob(&mut self, resource_id: u32);
    fn scanout_map(&mut self, resource_id: u32) -> Option<ScanoutMappedBlob>;
    fn scanout_unmap(&mut self, resource_id: u32);
    fn destroy_resource(&mut self, resource_id: u32);
    fn create_fence(&mut self, ctx_id: u32, ring_idx: u8, fence_id: u64) -> bool;
    fn poll_fences(&mut self);
    fn drain_completed_fences_into(&mut self, out: &mut Vec<CompletedFence>);
    fn drain_completed_fences(&mut self) -> Vec<CompletedFence> {
        let mut completed = Vec::new();
        self.drain_completed_fences_into(&mut completed);
        completed
    }
    fn reset(&mut self);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Create3dArgs {
    pub resource_id: u32,
    pub target: u32,
    pub format: u32,
    pub bind: u32,
    pub width: u32,
    pub height: u32,
    pub depth: u32,
    pub array_size: u32,
    pub last_level: u32,
    pub nr_samples: u32,
    pub flags: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Transfer3dArgs {
    pub ctx_id: u32,
    pub resource_id: u32,
    pub x: u32,
    pub y: u32,
    pub z: u32,
    pub width: u32,
    pub height: u32,
    pub depth: u32,
    pub offset: u64,
    pub level: u32,
    pub stride: u32,
    pub layer_stride: u32,
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

unsafe impl Send for BlobHostIovec {}

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

#[derive(Debug, Clone, Copy)]
pub(crate) struct BlobResourceInfoRef<'a> {
    pub blob_mem: u32,
    pub size: u64,
    pub backing: &'a [BlobMemEntry],
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
    resource_3d_ids: BTreeSet<u32>,
    blob_resources: BTreeMap<u32, BlobResource>,
    mapped_intervals: BTreeMap<u64, (u64, u32)>,
    host_iovecs_scratch: Vec<BlobHostIovec>,
    blob_unmap_ids_scratch: Vec<u32>,
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
            .field("resource_3d_ids", &self.resource_3d_ids)
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
        self.resource_3d_ids.clear();
        self.unmap_all_blobs();
        self.blob_resources.clear();
        self.mapped_intervals.clear();
        self.submits = 0;
    }

    pub fn unref_resource(&mut self, resource_id: u32) {
        self.resource_2d_ids.remove(&resource_id);
        let mut destroy_backend_resource = self.resource_3d_ids.remove(&resource_id);
        if self.blob_resources.contains_key(&resource_id) {
            self.unmap_blob_resource(resource_id);
            self.blob_resources.remove(&resource_id);
            self.mapped_intervals
                .retain(|_, (_, mapped_resource)| *mapped_resource != resource_id);
            destroy_backend_resource = true;
        }
        if destroy_backend_resource {
            if let Some(backend) = self.backend.as_mut() {
                backend.destroy_resource(resource_id);
            }
        }
    }

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

    pub fn is_3d_resource(&self, resource_id: u32) -> bool {
        self.resource_3d_ids.contains(&resource_id)
    }

    pub fn attach_3d_backing(
        &mut self,
        mem: &dyn GuestMemoryMut,
        resource_id: u32,
        backing: &[BlobMemEntry],
    ) -> bool {
        if !self.resource_3d_ids.contains(&resource_id) || backing.is_empty() {
            return false;
        }
        self.host_iovecs_scratch.clear();
        if !resolve_blob_iovecs_into(mem, backing, &mut self.host_iovecs_scratch) {
            return false;
        }
        let attached = self
            .backend
            .as_mut()
            .is_some_and(|backend| backend.attach_backing(resource_id, &self.host_iovecs_scratch));
        self.host_iovecs_scratch.clear();
        attached
    }

    pub fn detach_3d_backing(&mut self, resource_id: u32) -> bool {
        self.resource_3d_ids.contains(&resource_id)
            && self
                .backend
                .as_mut()
                .is_some_and(|backend| backend.detach_backing(resource_id))
    }

    pub fn drain_completed_fences(&mut self) -> Vec<CompletedFence> {
        let mut completed = Vec::new();
        self.drain_completed_fences_into(&mut completed);
        completed
    }

    pub fn drain_completed_fences_into(&mut self, out: &mut Vec<CompletedFence>) {
        let Some(backend) = self.backend.as_mut() else {
            return;
        };
        // Venus on macOS retires fences synchronously: polling the backend may
        // invoke the fence callback inline, then drain_completed_fences takes
        // the callbacks queued by that poll.
        backend.poll_fences();
        let start = out.len();
        backend.drain_completed_fences_into(out);
        self.fences_completed = self
            .fences_completed
            .saturating_add((out.len() - start) as u64);
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
        let mut out = Vec::new();
        self.handle_with_mem_into(mem, request, hdr, &mut out)
            .then_some(out)
    }

    pub fn handle_with_mem_into(
        &mut self,
        mem: Option<&dyn GuestMemoryMut>,
        request: &[u8],
        hdr: CtrlHdr3d,
        out: &mut Vec<u8>,
    ) -> bool {
        match hdr.typ {
            VIRTIO_GPU_CMD_GET_CAPSET_INFO => self.get_capset_info_into(request, hdr, out),
            VIRTIO_GPU_CMD_GET_CAPSET => self.get_capset_into(request, hdr, out),
            VIRTIO_GPU_CMD_RESOURCE_CREATE_BLOB => {
                self.resource_create_blob_into(mem, request, hdr, out)
            }
            VIRTIO_GPU_CMD_CTX_CREATE => self.ctx_create_into(request, hdr, out),
            VIRTIO_GPU_CMD_CTX_DESTROY => self.ctx_destroy_into(hdr, out),
            VIRTIO_GPU_CMD_CTX_ATTACH_RESOURCE => self.ctx_attach_resource_into(request, hdr, out),
            VIRTIO_GPU_CMD_CTX_DETACH_RESOURCE => self.ctx_detach_resource_into(request, hdr, out),
            VIRTIO_GPU_CMD_RESOURCE_CREATE_3D => self.resource_create_3d_into(request, hdr, out),
            VIRTIO_GPU_CMD_TRANSFER_TO_HOST_3D => self.transfer_3d_into(request, hdr, true, out),
            VIRTIO_GPU_CMD_TRANSFER_FROM_HOST_3D => self.transfer_3d_into(request, hdr, false, out),
            VIRTIO_GPU_CMD_SUBMIT_3D => self.submit_3d_into(request, hdr, out),
            VIRTIO_GPU_CMD_RESOURCE_MAP_BLOB => self.resource_map_blob_into(request, hdr, out),
            VIRTIO_GPU_CMD_RESOURCE_UNMAP_BLOB => self.resource_unmap_blob_into(request, hdr, out),
            _ => return false,
        }
        true
    }

    fn get_capset_info_into(&mut self, request: &[u8], hdr: CtrlHdr3d, out: &mut Vec<u8>) {
        let Some(backend) = self.backend.as_mut() else {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        };
        let Some(index) = read_le_u32(request, 24) else {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        };
        let Some(info) = backend.capset_info(index) else {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        };
        response_hdr_into(out, VIRTIO_GPU_RESP_OK_CAPSET_INFO, Some(hdr));
        out.extend_from_slice(&info.capset_id.to_le_bytes());
        out.extend_from_slice(&info.max_version.to_le_bytes());
        out.extend_from_slice(&info.max_size.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
    }

    fn get_capset_into(&mut self, request: &[u8], hdr: CtrlHdr3d, out: &mut Vec<u8>) {
        let Some(backend) = self.backend.as_mut() else {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        };
        let Some(capset_id) = read_le_u32(request, 24) else {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        };
        let Some(version) = read_le_u32(request, 28) else {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        };
        response_hdr_into(out, VIRTIO_GPU_RESP_OK_CAPSET, Some(hdr));
        if !backend.capset_into(capset_id, version, out) {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
        }
    }

    fn ctx_create_into(&mut self, request: &[u8], hdr: CtrlHdr3d, out: &mut Vec<u8>) {
        let Some(backend) = self.backend.as_mut() else {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        };
        if request.len() < CTX_CREATE_LEN
            || hdr.ctx_id == 0
            || self.live_contexts.contains(&hdr.ctx_id)
        {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        }
        let nlen = read_le_u32(request, 24).unwrap_or(0).min(64) as usize;
        let context_init = read_le_u32(request, 28).unwrap_or(0);
        let name = &request[32..32 + nlen];
        if !backend.ctx_create(hdr.ctx_id, context_init, name) {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_UNSPEC, Some(hdr));
            return;
        }
        self.live_contexts.insert(hdr.ctx_id);
        self.ctx_resources.entry(hdr.ctx_id).or_default();
        response_hdr_into(out, VIRTIO_GPU_RESP_OK_NODATA, Some(hdr));
    }

    fn ctx_destroy_into(&mut self, hdr: CtrlHdr3d, out: &mut Vec<u8>) {
        if !self.live_contexts.remove(&hdr.ctx_id) {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        }
        self.ctx_resources.remove(&hdr.ctx_id);
        if let Some(backend) = self.backend.as_mut() {
            backend.ctx_destroy(hdr.ctx_id);
        }
        response_hdr_into(out, VIRTIO_GPU_RESP_OK_NODATA, Some(hdr));
    }

    fn ctx_attach_resource_into(&mut self, request: &[u8], hdr: CtrlHdr3d, out: &mut Vec<u8>) {
        if request.len() < CTX_RESOURCE_LEN {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        }
        let resource_id = read_le_u32(request, 24).unwrap_or(0);
        if !self.resource_exists(resource_id) {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        }
        if let Some(resources) = self.ctx_resources.get_mut(&hdr.ctx_id) {
            resources.insert(resource_id);
        }
        if let Some(backend) = self.backend.as_mut() {
            backend.ctx_attach_resource(hdr.ctx_id, resource_id);
        }
        response_hdr_into(out, VIRTIO_GPU_RESP_OK_NODATA, Some(hdr));
    }

    fn ctx_detach_resource_into(&mut self, request: &[u8], hdr: CtrlHdr3d, out: &mut Vec<u8>) {
        if request.len() < CTX_RESOURCE_LEN {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        }
        let resource_id = read_le_u32(request, 24).unwrap_or(0);
        if !self.resource_exists(resource_id) {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        }
        if let Some(resources) = self.ctx_resources.get_mut(&hdr.ctx_id) {
            resources.remove(&resource_id);
        }
        if let Some(backend) = self.backend.as_mut() {
            backend.ctx_detach_resource(hdr.ctx_id, resource_id);
        }
        response_hdr_into(out, VIRTIO_GPU_RESP_OK_NODATA, Some(hdr));
    }

    fn resource_create_3d_into(&mut self, request: &[u8], hdr: CtrlHdr3d, out: &mut Vec<u8>) {
        if request.len() < RESOURCE_CREATE_3D_LEN {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        }
        let args = Create3dArgs {
            resource_id: read_le_u32(request, 24).unwrap_or(0),
            target: read_le_u32(request, 28).unwrap_or(0),
            format: read_le_u32(request, 32).unwrap_or(0),
            bind: read_le_u32(request, 36).unwrap_or(0),
            width: read_le_u32(request, 40).unwrap_or(0),
            height: read_le_u32(request, 44).unwrap_or(0),
            depth: read_le_u32(request, 48).unwrap_or(0),
            array_size: read_le_u32(request, 52).unwrap_or(0),
            last_level: read_le_u32(request, 56).unwrap_or(0),
            nr_samples: read_le_u32(request, 60).unwrap_or(0),
            flags: read_le_u32(request, 64).unwrap_or(0),
        };
        if args.resource_id == 0 || self.resource_exists(args.resource_id) {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        }
        let created = self
            .backend
            .as_mut()
            .is_some_and(|backend| backend.create_3d(args));
        if !created {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_UNSPEC, Some(hdr));
            return;
        }
        self.resource_3d_ids.insert(args.resource_id);
        response_hdr_into(out, VIRTIO_GPU_RESP_OK_NODATA, Some(hdr));
    }

    fn transfer_3d_into(
        &mut self,
        request: &[u8],
        hdr: CtrlHdr3d,
        to_host: bool,
        out: &mut Vec<u8>,
    ) {
        if request.len() < TRANSFER_3D_LEN {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        }
        let args = Transfer3dArgs {
            ctx_id: hdr.ctx_id,
            resource_id: read_le_u32(request, 56).unwrap_or(0),
            x: read_le_u32(request, 24).unwrap_or(0),
            y: read_le_u32(request, 28).unwrap_or(0),
            z: read_le_u32(request, 32).unwrap_or(0),
            width: read_le_u32(request, 36).unwrap_or(0),
            height: read_le_u32(request, 40).unwrap_or(0),
            depth: read_le_u32(request, 44).unwrap_or(0),
            offset: read_le_u64(request, 48).unwrap_or(0),
            level: read_le_u32(request, 60).unwrap_or(0),
            stride: read_le_u32(request, 64).unwrap_or(0),
            layer_stride: read_le_u32(request, 68).unwrap_or(0),
        };
        if !self.resource_3d_ids.contains(&args.resource_id)
            || args.width == 0
            || args.height == 0
            || args.depth == 0
        {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        }
        let transferred = self
            .backend
            .as_mut()
            .is_some_and(|backend| backend.transfer_3d(args, to_host));
        response_hdr_into(
            out,
            if transferred {
                VIRTIO_GPU_RESP_OK_NODATA
            } else {
                VIRTIO_GPU_RESP_ERR_UNSPEC
            },
            Some(hdr),
        );
    }

    fn submit_3d_into(&mut self, request: &[u8], hdr: CtrlHdr3d, out: &mut Vec<u8>) {
        let Some(backend) = self.backend.as_mut() else {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        };
        if request.len() < SUBMIT_3D_LEN {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        }
        let size = read_le_u32(request, 24).unwrap_or(0) as usize;
        // The Windows VirGL driver uses an empty context-0 submit as a queue
        // synchronization no-op. It has no renderer payload or context state to
        // validate, so acknowledge it without calling virglrenderer.
        if size == 0 && hdr.ctx_id == 0 {
            response_hdr_into(out, VIRTIO_GPU_RESP_OK_NODATA, Some(hdr));
            return;
        }
        if !self.live_contexts.contains(&hdr.ctx_id) {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        }
        if size > MAX_SUBMIT_3D_BYTES || request.len().saturating_sub(SUBMIT_3D_LEN) < size {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        }
        let cmdbuf = &request[SUBMIT_3D_LEN..SUBMIT_3D_LEN + size];
        if !backend.submit_3d(hdr.ctx_id, cmdbuf) {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_UNSPEC, Some(hdr));
            return;
        }
        self.submits = self.submits.saturating_add(1);
        response_hdr_into(out, VIRTIO_GPU_RESP_OK_NODATA, Some(hdr));
    }

    fn resource_create_blob_into(
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
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        }
        if blob_mem != VIRTIO_GPU_BLOB_MEM_HOST3D && blob_mem != VIRTIO_GPU_BLOB_MEM_GUEST {
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
                response_hdr_into(out, VIRTIO_GPU_RESP_ERR_UNSPEC, Some(hdr));
                return;
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
        response_hdr_into(out, VIRTIO_GPU_RESP_OK_NODATA, Some(hdr));
    }

    fn resource_map_blob_into(&mut self, request: &[u8], hdr: CtrlHdr3d, out: &mut Vec<u8>) {
        if request.len() < RESOURCE_MAP_BLOB_LEN {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        }
        let resource_id = read_le_u32(request, 24).unwrap_or(0);
        let shm_offset = read_le_u64(request, 32).unwrap_or(u64::MAX);
        let Some(resource) = self.blob_resources.get(&resource_id) else {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        };
        if resource.mapped.is_some() || resource.blob_mem != VIRTIO_GPU_BLOB_MEM_HOST3D {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
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
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        }
        let Some(backend) = self.backend.as_mut() else {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        };
        let Some(mapped) = backend.map_blob(resource_id) else {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_OUT_OF_MEMORY, Some(hdr));
            return;
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
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_OUT_OF_MEMORY, Some(hdr));
            return;
        }
        let Some(port) = self.shm_port.as_mut() else {
            backend.unmap_blob(resource_id);
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_OUT_OF_MEMORY, Some(hdr));
            return;
        };
        if port.map(mapped.host_ptr, map_size, shm_offset).is_err() {
            backend.unmap_blob(resource_id);
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_OUT_OF_MEMORY, Some(hdr));
            return;
        }
        if let Some(resource) = self.blob_resources.get_mut(&resource_id) {
            resource.mapped = Some((shm_offset, map_size));
        }
        self.mapped_intervals
            .insert(shm_offset, (map_size as u64, resource_id));
        response_hdr_into(out, VIRTIO_GPU_RESP_OK_MAP_INFO, Some(hdr));
        out.extend_from_slice(&(mapped.map_info & VIRTIO_GPU_MAP_CACHE_MASK).to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
    }

    fn resource_unmap_blob_into(&mut self, request: &[u8], hdr: CtrlHdr3d, out: &mut Vec<u8>) {
        if request.len() < RESOURCE_UNMAP_BLOB_LEN {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        }
        let resource_id = read_le_u32(request, 24).unwrap_or(0);
        if !self.blob_resources.contains_key(&resource_id) {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        }
        self.unmap_blob_resource(resource_id);
        response_hdr_into(out, VIRTIO_GPU_RESP_OK_NODATA, Some(hdr));
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
        self.blob_unmap_ids_scratch.clear();
        self.blob_unmap_ids_scratch
            .extend(self.blob_resources.keys().copied());
        let mut ids = std::mem::take(&mut self.blob_unmap_ids_scratch);
        for resource_id in ids.drain(..) {
            self.unmap_blob_resource(resource_id);
        }
        self.blob_unmap_ids_scratch = ids;
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
                || self.resource_3d_ids.contains(&resource_id)
                || self.blob_resources.contains_key(&resource_id))
    }
}

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

fn resolve_blob_iovecs_into(
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

#[cfg(test)]
#[derive(Debug, Default)]
pub struct MockBackend {
    pub capset_info: Option<CapsetInfo>,
    pub capset: Vec<u8>,
    pub capset_calls: u32,
    pub created: Vec<(u32, u32, Vec<u8>)>,
    pub destroyed: Vec<u32>,
    pub attached: Vec<(u32, u32)>,
    pub detached: Vec<(u32, u32)>,
    pub created_3d: Vec<Create3dArgs>,
    pub backing_attached: Vec<(u32, usize, usize)>,
    pub backing_detached: Vec<u32>,
    pub transfers_3d: Vec<(Transfer3dArgs, bool)>,
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
        let mut inner = self.lock().unwrap();
        inner.capset_calls += 1;
        (inner.capset_info.map(|info| info.capset_id) == Some(capset_id))
            .then(|| inner.capset.clone())
    }

    fn capset_into(&mut self, capset_id: u32, _version: u32, out: &mut Vec<u8>) -> bool {
        let inner = self.lock().unwrap();
        if inner.capset_info.map(|info| info.capset_id) != Some(capset_id) {
            return false;
        }
        out.extend_from_slice(&inner.capset);
        true
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

    fn create_3d(&mut self, args: Create3dArgs) -> bool {
        self.lock().unwrap().created_3d.push(args);
        true
    }

    fn attach_backing(&mut self, resource_id: u32, iovecs: &[BlobHostIovec]) -> bool {
        self.lock().unwrap().backing_attached.push((
            resource_id,
            iovecs.len(),
            iovecs.iter().map(|iov| iov.len).sum(),
        ));
        true
    }

    fn detach_backing(&mut self, resource_id: u32) -> bool {
        self.lock().unwrap().backing_detached.push(resource_id);
        true
    }

    fn transfer_3d(&mut self, args: Transfer3dArgs, to_host: bool) -> bool {
        self.lock().unwrap().transfers_3d.push((args, to_host));
        true
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

    fn drain_completed_fences_into(&mut self, out: &mut Vec<CompletedFence>) {
        out.append(&mut self.lock().unwrap().completed);
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
    fn get_capset_uses_backend_capset_into_without_cloning_capset_vec() {
        let backend = Arc::new(Mutex::new(MockBackend::new_venus()));
        let mut gpu = VirtioGpu3d::with_backend(Box::new(backend.clone()));
        let mut get = ctrl_req(VIRTIO_GPU_CMD_GET_CAPSET, 0);
        get.extend_from_slice(&4u32.to_le_bytes());
        get.extend_from_slice(&1u32.to_le_bytes());
        let hdr = CtrlHdr3d::parse(&get).unwrap();

        let mut out = Vec::with_capacity(24 + 160);
        let response_ptr = out.as_ptr();
        assert!(gpu.handle_with_mem_into(None, &get, hdr, &mut out));

        assert_eq!(read_le_u32(&out, 0), Some(VIRTIO_GPU_RESP_OK_CAPSET));
        assert_eq!(read_le_u32(&out, 24), Some(1));
        assert_eq!(out.as_ptr(), response_ptr);
        assert_eq!(backend.lock().unwrap().capset_calls, 0);
    }

    #[test]
    fn drain_completed_fences_into_reuses_caller_storage_and_counts() {
        let backend = Arc::new(Mutex::new(MockBackend::new_venus()));
        let mut gpu = VirtioGpu3d::with_backend(Box::new(backend.clone()));
        let sentinel = CompletedFence {
            ctx_id: 99,
            ring_idx: 0,
            fence_id: 1,
        };
        let completed = [
            CompletedFence {
                ctx_id: 1,
                ring_idx: 2,
                fence_id: 3,
            },
            CompletedFence {
                ctx_id: 4,
                ring_idx: 5,
                fence_id: 6,
            },
        ];
        backend.lock().unwrap().completed.extend(completed);

        let mut out = Vec::with_capacity(4);
        out.push(sentinel);
        let out_ptr = out.as_ptr();
        let out_capacity = out.capacity();

        gpu.drain_completed_fences_into(&mut out);

        assert_eq!(out.as_ptr(), out_ptr);
        assert_eq!(out.capacity(), out_capacity);
        assert_eq!(out, vec![sentinel, completed[0], completed[1]]);
        assert_eq!(gpu.stats(0).fences_completed, 2);
        assert!(backend.lock().unwrap().completed.is_empty());

        let wrapper_fence = CompletedFence {
            ctx_id: 7,
            ring_idx: 8,
            fence_id: 9,
        };
        backend.lock().unwrap().completed.push(wrapper_fence);
        assert_eq!(gpu.drain_completed_fences(), vec![wrapper_fence]);
        assert_eq!(gpu.stats(0).fences_completed, 3);
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

    #[test]
    fn guest_blob_create_reuses_host_iovec_scratch() {
        let backend = Arc::new(Mutex::new(MockBackend::new_venus()));
        let mut gpu = VirtioGpu3d::with_backend(Box::new(backend.clone()));
        let mem = TestMem::new(0x8000_0000, 0x20_000);
        let entries = [
            BlobMemEntry {
                addr: 0x8000_1000,
                len: 0x1000,
            },
            BlobMemEntry {
                addr: 0x8000_4000,
                len: 0x2000,
            },
        ];

        let mut previous_scratch = None;
        for resource_id in [29, 30] {
            let create = create_blob_req_with_entries(
                resource_id,
                VIRTIO_GPU_BLOB_MEM_GUEST,
                77,
                0x3000,
                3,
                &entries,
            );
            let hdr = CtrlHdr3d::parse(&create).unwrap();
            assert_eq!(
                read_le_u32(&gpu.handle_with_mem(Some(&mem), &create, hdr).unwrap(), 0),
                Some(VIRTIO_GPU_RESP_OK_NODATA)
            );
            assert!(gpu.host_iovecs_scratch.is_empty());
            let scratch = (
                gpu.host_iovecs_scratch.as_ptr() as usize,
                gpu.host_iovecs_scratch.capacity(),
            );
            assert!(scratch.1 >= entries.len());
            if let Some(previous) = previous_scratch {
                assert_eq!(scratch, previous);
            }
            previous_scratch = Some(scratch);
        }

        assert_eq!(
            backend.lock().unwrap().blob_iovecs,
            vec![(29, 2, 0x3000), (30, 2, 0x3000)]
        );
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

    #[test]
    fn legacy_virgl_resource_backing_and_bidirectional_transfers_reach_backend() {
        let backend = Arc::new(Mutex::new(MockBackend::new_venus()));
        let mut gpu = VirtioGpu3d::with_backend(Box::new(backend.clone()));
        let mem = TestMem::new(0x1000, 0x4000);

        let create_args = Create3dArgs {
            resource_id: 41,
            target: 2,
            format: 1,
            bind: 0x402,
            width: 640,
            height: 480,
            depth: 1,
            array_size: 1,
            last_level: 0,
            nr_samples: 0,
            flags: 0,
        };
        let mut create = ctrl_req(VIRTIO_GPU_CMD_RESOURCE_CREATE_3D, 0);
        for field in [
            create_args.resource_id,
            create_args.target,
            create_args.format,
            create_args.bind,
            create_args.width,
            create_args.height,
            create_args.depth,
            create_args.array_size,
            create_args.last_level,
            create_args.nr_samples,
            create_args.flags,
            0,
        ] {
            create.extend_from_slice(&field.to_le_bytes());
        }
        let hdr = CtrlHdr3d::parse(&create).unwrap();
        let response = gpu.handle(&create, hdr).unwrap();
        assert_eq!(read_le_u32(&response, 0), Some(VIRTIO_GPU_RESP_OK_NODATA));
        assert!(gpu.is_3d_resource(41));

        assert!(gpu.attach_3d_backing(
            &mem,
            41,
            &[
                BlobMemEntry {
                    addr: 0x1000,
                    len: 0x1000,
                },
                BlobMemEntry {
                    addr: 0x2000,
                    len: 0x2000,
                },
            ],
        ));

        for (typ, to_host) in [
            (VIRTIO_GPU_CMD_TRANSFER_TO_HOST_3D, true),
            (VIRTIO_GPU_CMD_TRANSFER_FROM_HOST_3D, false),
        ] {
            let mut transfer = ctrl_req(typ, 7);
            for field in [3u32, 4, 0, 32, 16, 1] {
                transfer.extend_from_slice(&field.to_le_bytes());
            }
            transfer.extend_from_slice(&128u64.to_le_bytes());
            for field in [41u32, 2, 256, 4096] {
                transfer.extend_from_slice(&field.to_le_bytes());
            }
            let hdr = CtrlHdr3d::parse(&transfer).unwrap();
            let response = gpu.handle(&transfer, hdr).unwrap();
            assert_eq!(read_le_u32(&response, 0), Some(VIRTIO_GPU_RESP_OK_NODATA));
            assert_eq!(
                backend.lock().unwrap().transfers_3d.last().unwrap().1,
                to_host
            );
        }

        assert!(gpu.detach_3d_backing(41));
        gpu.unref_resource(41);
        let inner = backend.lock().unwrap();
        assert_eq!(inner.created_3d, vec![create_args]);
        assert_eq!(inner.backing_attached, vec![(41, 2, 0x3000)]);
        assert_eq!(inner.backing_detached, vec![41]);
        assert_eq!(inner.transfers_3d.len(), 2);
        assert_eq!(inner.transfers_3d[0].0.resource_id, 41);
        assert_eq!(inner.transfers_3d[0].0.ctx_id, 7);
        assert_eq!(inner.transfers_3d[0].0.width, 32);
        assert_eq!(inner.destroyed_resources, vec![41]);
    }

    #[test]
    fn empty_context_zero_submit_is_an_immediate_noop() {
        let backend = Arc::new(Mutex::new(MockBackend::new_venus()));
        let mut gpu = VirtioGpu3d::with_backend(Box::new(backend.clone()));
        let mut submit = ctrl_req(VIRTIO_GPU_CMD_SUBMIT_3D, 0);
        submit.extend_from_slice(&0u32.to_le_bytes());
        submit.extend_from_slice(&0u32.to_le_bytes());
        let hdr = CtrlHdr3d::parse(&submit).unwrap();
        let response = gpu.handle(&submit, hdr).unwrap();
        assert_eq!(read_le_u32(&response, 0), Some(VIRTIO_GPU_RESP_OK_NODATA));
        assert!(backend.lock().unwrap().submits.is_empty());
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
