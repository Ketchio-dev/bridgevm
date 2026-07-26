//! The VirtioGpu3d device state: struct, construction and wiring, stats, full reset.

use super::*;
include!("device_debug.rs");
use std::collections::BTreeMap;
use std::collections::BTreeSet;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct VirtioGpu3dStats {
    pub ctx_active: usize,
    pub submits: u64,
    pub fences_pending: usize,
    pub fences_completed: u64,
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
    pub(crate) last_submit_diagnostic: Option<Submit3dDiagnostic>,
    pub(crate) local_copy_submits: u64,
    pub(crate) submits: u64,
    pub(crate) fences_completed: u64,
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
        self.last_submit_diagnostic = None;
        self.local_copy_submits = 0;
        self.submits = 0;
    }
}
