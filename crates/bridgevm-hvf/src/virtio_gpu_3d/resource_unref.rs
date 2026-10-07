//! Resource unref: renderer destruction, per-context detach and bounded
//! destroyed-blob diagnostics.

use super::*;
use std::collections::BTreeSet;

/// Destroyed blob ids only classify late UNMAP_BLOB rejections for traces.
/// The guest chooses the ids, so each set keeps at most this many.
pub(crate) const MAX_DESTROYED_BLOB_IDS: usize = 4096;

fn remember_destroyed_blob(ids: &mut BTreeSet<u32>, resource_id: u32) {
    if ids.len() >= MAX_DESTROYED_BLOB_IDS && !ids.contains(&resource_id) {
        ids.pop_first();
    }
    ids.insert(resource_id);
}

impl VirtioGpu3d {
    pub fn unref_resource(&mut self, resource_id: u32) {
        // virgl_renderer_resource_unref detaches the resource from every
        // context, so a reused id stays unattached until the guest attaches it.
        for resources in self.ctx_resources.values_mut() {
            resources.remove(&resource_id);
        }
        self.resource_2d_ids.remove(&resource_id);
        let mut destroy_backend_resource = self.resource_3d_ids.remove(&resource_id);
        if self.local_3d_backing.remove(&resource_id).is_some() {
            destroy_backend_resource = false;
        }
        self.resource_3d_info.remove(&resource_id);
        if let Some(resource) = self.blob_resources.get(&resource_id) {
            if resource.mapped.is_some() {
                remember_destroyed_blob(&mut self.destroyed_blob_mapped_ids, resource_id);
                self.destroyed_blob_unmapped_ids.remove(&resource_id);
            } else {
                remember_destroyed_blob(&mut self.destroyed_blob_unmapped_ids, resource_id);
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
}
