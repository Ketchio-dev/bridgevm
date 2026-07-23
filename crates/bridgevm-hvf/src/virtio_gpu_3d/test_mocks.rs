//! The test-only mock renderer backend and mock shm map port.

use super::backend::*;

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
    pub mapped: std::collections::BTreeMap<u32, MappedBlob>,
    pub unmapped: Vec<u32>,
    pub scanout_reads: Vec<(u32, u32, u32)>,
    pub scanout_blits: Vec<(u32, u32, u32)>,
    pub destroyed_resources: Vec<u32>,
    pub fences: Vec<CompletedFence>,
    pub completed: Vec<CompletedFence>,
    pub fence_polls: u64,
    pub fence_after_queue_polls: u64,
    pub reject_fence_ring: Option<u8>,
    pub reject_legacy_3d: bool,
}

#[cfg(test)]
#[derive(Debug, Default)]
pub(crate) struct MockMapPort {
    pub(crate) maps: Vec<(usize, usize, u64)>,
    pub(crate) unmaps: Vec<(u64, usize)>,
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

    fn supports_legacy_3d_resources(&self) -> bool {
        !self.lock().unwrap().reject_legacy_3d
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

    fn scanout_read(&mut self, resource_id: u32, width: u32, height: u32, out: &mut [u8]) -> bool {
        self.lock()
            .unwrap()
            .scanout_reads
            .push((resource_id, width, height));
        for (index, byte) in out.iter_mut().enumerate() {
            *byte = index as u8;
        }
        true
    }

    fn scanout_blit_iosurface(&mut self, resource_id: u32, width: u32, height: u32) -> Option<u32> {
        self.lock()
            .unwrap()
            .scanout_blits
            .push((resource_id, width, height));
        Some(42)
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

    fn poll_fences(&mut self) {
        self.lock().unwrap().fence_polls += 1;
    }

    fn poll_fences_after_queue(&mut self) {
        self.lock().unwrap().fence_after_queue_polls += 1;
    }

    fn drain_completed_fences_into(&mut self, out: &mut Vec<CompletedFence>) {
        out.append(&mut self.lock().unwrap().completed);
    }

    fn reset(&mut self) {
        self.lock().unwrap().completed.clear();
    }
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
