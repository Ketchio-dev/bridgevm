//! Split out of virtio_gpu_3d.rs to keep files under 850 lines.

use super::*;
use std::collections::BTreeMap;
use std::collections::BTreeSet;

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
pub(crate) const HVF_PAGE_SIZE: u64 = 16 * 1024;
pub(crate) const PIPE_TEXTURE_2D: u32 = 2;
pub(crate) const VIRGL_BIND_DISPLAY_TARGET: u32 = 1 << 7;
pub(crate) const VIRGL_BIND_SCANOUT: u32 = 1 << 18;
pub(crate) const MAX_LOCAL_SCANOUT_DIMENSION: u32 = 16_384;
pub(crate) const VIRGL_CCMD_RESOURCE_COPY_REGION: u32 = 17;
pub(crate) const VIRGL_RESOURCE_COPY_REGION_PAYLOAD_DWORDS: u32 = 13;
pub(crate) const VIRGL_RESOURCE_COPY_REGION_DWORDS: usize = 14;
pub(crate) const VIRGL_RESOURCE_COPY_REGION_BYTES: usize = VIRGL_RESOURCE_COPY_REGION_DWORDS * 4;

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
    /// Whether legacy VirGL RESOURCE_CREATE_3D objects can be created in this
    /// renderer instance. A Venus-only backend may return false; the Windows
    /// Venus WDDM stack additionally needs a VirGL shadow renderer for present.
    fn supports_legacy_3d_resources(&self) -> bool {
        true
    }
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
    fn scanout_read(
        &mut self,
        _resource_id: u32,
        _width: u32,
        _height: u32,
        _out: &mut [u8],
    ) -> bool {
        false
    }
    /// GPU-blit the scanout resource into a host-shareable IOSurface and
    /// return the surface's global ID. Default: unsupported.
    fn scanout_blit_iosurface(
        &mut self,
        _resource_id: u32,
        _width: u32,
        _height: u32,
    ) -> Option<u32> {
        None
    }
    /// Checksum the IOSurface contents (validation only — stalls the GPU).
    fn scanout_iosurface_checksum(&mut self) -> Option<u64> {
        None
    }
    /// Dump the raw IOSurface pixels to a file (diagnostics only).
    fn scanout_iosurface_dump(&mut self, _path: &std::path::Path) -> bool {
        false
    }
    fn destroy_resource(&mut self, resource_id: u32);
    fn create_fence(&mut self, ctx_id: u32, ring_idx: u8, fence_id: u64) -> bool;
    fn poll_fences(&mut self);
    /// Poll after one complete virtqueue notification batch. Backends that
    /// proxy renderer calls to another thread may keep idle polling local but
    /// must preserve this explicit batch boundary.
    fn poll_fences_after_queue(&mut self) {
        self.poll_fences();
    }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LocalResourceCopyRegion {
    pub(crate) dst_resource_id: u32,
    pub(crate) dst_x: u32,
    pub(crate) dst_y: u32,
    pub(crate) src_resource_id: u32,
    pub(crate) src_x: u32,
    pub(crate) src_y: u32,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LocalResourceCopyResult {
    NotApplicable,
    Invalid,
    Copied { regions: usize },
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
    pub(crate) backend: Option<Box<dyn VirtioGpu3dBackend>>,
    pub(crate) shm_port: Option<Box<dyn GpuShmMapPort>>,
    pub(crate) shm_window_size: u64,
    pub(crate) live_contexts: BTreeSet<u32>,
    pub(crate) ctx_resources: BTreeMap<u32, BTreeSet<u32>>,
    pub(crate) resource_2d_ids: BTreeSet<u32>,
    pub(crate) resource_3d_ids: BTreeSet<u32>,
    pub(crate) resource_3d_info: BTreeMap<u32, Create3dArgs>,
    pub(crate) local_3d_backing: BTreeMap<u32, Vec<BlobMemEntry>>,
    pub(crate) blob_resources: BTreeMap<u32, BlobResource>,
    pub(crate) mapped_intervals: BTreeMap<u64, (u64, u32)>,
    pub(crate) destroyed_blob_mapped_ids: BTreeSet<u32>,
    pub(crate) destroyed_blob_unmapped_ids: BTreeSet<u32>,
    pub(crate) unmap_blob_reject_counts: UnmapBlobRejectCounts,
    pub(crate) host_iovecs_scratch: Vec<BlobHostIovec>,
    pub(crate) blob_unmap_ids_scratch: Vec<u32>,
    pub(crate) local_copy_scratch: Vec<u8>,
    pub(crate) local_copy_submits: u64,
    pub(crate) submits: u64,
    pub(crate) fences_completed: u64,
}

/// Classified `RESOURCE_UNMAP_BLOB` invalid-parameter rejections. The guest
/// driver's cleanup order determines which class fires: an unmap that arrives
/// after `RESOURCE_UNREF` of a still-mapped blob is late-but-harmless cleanup
/// (the host already unmapped at destroy), while `never_created` points at a
/// real mapping-lifecycle bug or resource-id confusion.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct UnmapBlobRejectCounts {
    pub short_request: u64,
    pub destroyed_while_mapped: u64,
    pub destroyed_after_unmap: u64,
    pub never_created: u64,
}

impl UnmapBlobRejectCounts {
    pub fn total(&self) -> u64 {
        self.short_request
            + self.destroyed_while_mapped
            + self.destroyed_after_unmap
            + self.never_created
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BlobResource {
    pub(crate) blob_mem: u32,
    pub(crate) size: u64,
    pub(crate) mapped: Option<(u64, usize)>,
    pub(crate) backing: Vec<BlobMemEntry>,
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
            .field("resource_3d_info", &self.resource_3d_info)
            .field("local_3d_backing", &self.local_3d_backing.keys())
            .field("blob_resources", &self.blob_resources)
            .field("local_copy_submits", &self.local_copy_submits)
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

    pub fn has_live_context(&self, ctx_id: u32) -> bool {
        self.live_contexts.contains(&ctx_id)
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
        self.resource_3d_info.clear();
        self.local_3d_backing.clear();
        self.unmap_all_blobs();
        self.blob_resources.clear();
        self.mapped_intervals.clear();
        self.destroyed_blob_mapped_ids.clear();
        self.destroyed_blob_unmapped_ids.clear();
        self.unmap_blob_reject_counts = UnmapBlobRejectCounts::default();
        self.local_copy_scratch.clear();
        self.local_copy_submits = 0;
        self.submits = 0;
    }

    pub fn unref_resource(&mut self, resource_id: u32) {
        self.resource_2d_ids.remove(&resource_id);
        let mut destroy_backend_resource = self.resource_3d_ids.remove(&resource_id);
        if self.local_3d_backing.remove(&resource_id).is_some() {
            destroy_backend_resource = false;
        }
        self.resource_3d_info.remove(&resource_id);
        if let Some(resource) = self.blob_resources.get(&resource_id) {
            if resource.mapped.is_some() {
                self.destroyed_blob_mapped_ids.insert(resource_id);
                self.destroyed_blob_unmapped_ids.remove(&resource_id);
            } else {
                self.destroyed_blob_unmapped_ids.insert(resource_id);
                self.destroyed_blob_mapped_ids.remove(&resource_id);
            }
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

    pub fn scanout_3d_info(&self, resource_id: u32) -> Option<Create3dArgs> {
        self.resource_3d_info.get(&resource_id).copied()
    }
}
