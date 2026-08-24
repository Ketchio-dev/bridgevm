//! Fail-closed preflight for VirGL transfers that would poison a renderer context.

use super::*;

pub(crate) const VIRGL_CCMD_TRANSFER3D: u32 = 43;
const VIRGL_TRANSFER3D_PAYLOAD_DWORDS: usize = 13;
const VIRGL_TRANSFER3D_RESOURCE_DWORD: usize = 1;
const PIPE_BUFFER: u32 = 0;
const VIRGL_BIND_CONSTANT_BUFFER: u32 = 1 << 6;
const EINVAL: i32 = 22;

impl VirtioGpu3d {
    pub(crate) fn set_backend_backing(&mut self, resource_id: u32, backed: bool) {
        if backed {
            self.backend_backed_resource_ids.insert(resource_id);
        } else {
            self.backend_backed_resource_ids.remove(&resource_id);
        }
    }

    /// Reject before dispatch instead of after the renderer has already marked
    /// the context erroneous. The measured Windows failure submits TRANSFER3D
    /// for a constant buffer whose RESOURCE_ATTACH_BACKING never arrived;
    /// virglrenderer answers EINVAL and every later submit on that context
    /// fails too. Never guess backing and never accept the command.
    pub(crate) fn submit_rejected_before_renderer(&mut self, ctx_id: u32, cmdbuf: &[u8]) -> bool {
        let diagnostic = self.preflight_unbacked_buffer_transfer(ctx_id, cmdbuf);
        let rejected = diagnostic.is_some();
        if rejected {
            self.set_submit_diagnostic(diagnostic);
        }
        rejected
    }

    pub(crate) fn preflight_unbacked_buffer_transfer(
        &self,
        ctx_id: u32,
        cmdbuf: &[u8],
    ) -> Option<Submit3dDiagnostic> {
        let mut offset = 0usize;
        while offset < cmdbuf.len() {
            let header = read_le_u32(cmdbuf, offset)?;
            let payload_dwords = (header >> 16) as usize;
            let command_bytes = payload_dwords.checked_add(1)?.checked_mul(4)?;
            let end = offset.checked_add(command_bytes)?;
            if end > cmdbuf.len() {
                return None;
            }
            if header & 0xff == VIRGL_CCMD_TRANSFER3D
                && payload_dwords >= VIRGL_TRANSFER3D_PAYLOAD_DWORDS
            {
                let resource_id =
                    read_le_u32(cmdbuf, offset + VIRGL_TRANSFER3D_RESOURCE_DWORD * 4)?;
                let info = self.resource_3d_info.get(&resource_id)?;
                if info.target == PIPE_BUFFER
                    && info.bind == VIRGL_BIND_CONSTANT_BUFFER
                    && self.ctx_has_resource(ctx_id, resource_id)
                    && !self.backend_backed_resource_ids.contains(&resource_id)
                {
                    return Some(Submit3dDiagnostic {
                        renderer_status: EINVAL,
                        command_offset_dwords: u32::try_from(offset / 4).ok(),
                        command_id: Some(VIRGL_CCMD_TRANSFER3D),
                        command_header: Some(header),
                        resource_id: Some(resource_id),
                        resource_found: Some(true),
                        resource_backed: Some(false),
                    });
                }
            }
            offset = end;
        }
        None
    }
}
