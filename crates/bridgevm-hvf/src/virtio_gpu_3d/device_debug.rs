// Debug rendering for VirtioGpu3d state.

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
