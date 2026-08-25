// Full device reset: renderer reset plus every registry this device owns.

impl VirtioGpu3d {
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
