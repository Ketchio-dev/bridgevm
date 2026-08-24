//! Which resources the backend has accepted backing for.

use super::*;

impl VirtioGpu3d {
    pub(crate) fn set_backend_backing(&mut self, resource_id: u32, backed: bool) {
        if backed {
            self.backend_backed_resource_ids.insert(resource_id);
        } else {
            self.backend_backed_resource_ids.remove(&resource_id);
        }
    }
}
