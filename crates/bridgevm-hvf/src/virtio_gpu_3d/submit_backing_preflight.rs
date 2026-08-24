//! Fail-closed preflight for VirGL transfers that would poison a renderer context.

use super::*;

pub(crate) const VIRGL_CCMD_TRANSFER3D: u32 = 43;
pub(super) const VIRGL_TRANSFER3D_PAYLOAD_DWORDS: usize = 13;
pub(super) const VIRGL_TRANSFER3D_RESOURCE_OFFSET: usize = 4;
const PIPE_BUFFER: u32 = 0;
const VIRGL_BIND_CONSTANT_BUFFER: u32 = 1 << 6;
pub(super) const EINVAL: i32 = 22;

impl VirtioGpu3d {
    /// Reject before dispatch instead of after the renderer has already marked
    /// the context erroneous. The measured Windows failure submits TRANSFER3D
    /// for a constant buffer whose RESOURCE_ATTACH_BACKING never arrived;
    /// virglrenderer answers EINVAL and every later submit on that context
    /// fails too. Never guess backing and never accept the command.
    pub(crate) fn submit_rejected_before_renderer(&mut self, ctx_id: u32, cmdbuf: &[u8]) -> bool {
        match self.preflight_unbacked_buffer_transfer(ctx_id, cmdbuf) {
            Some(diagnostic) => {
                self.set_submit_diagnostic(Some(diagnostic));
                true
            }
            None => false,
        }
    }

    pub(crate) fn set_backend_backing(&mut self, resource_id: u32, backed: bool) {
        if backed {
            self.backend_backed_resource_ids.insert(resource_id);
        } else {
            self.backend_backed_resource_ids.remove(&resource_id);
        }
    }

    pub(super) fn unbacked_context_buffer(&self, ctx_id: u32, resource_id: u32) -> bool {
        self.resource_3d_info.get(&resource_id).is_some_and(|info| {
            info.target == PIPE_BUFFER && info.bind == VIRGL_BIND_CONSTANT_BUFFER
        }) && self.ctx_has_resource(ctx_id, resource_id)
            && !self.backend_backed_resource_ids.contains(&resource_id)
    }
}
