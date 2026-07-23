//! The host-renderer abstraction and the plain value types that cross it.

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
