//! Bounded walk of a VirGL command buffer looking for an unbacked transfer.

use super::submit_backing_preflight::*;
use super::*;

impl VirtioGpu3d {
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
            // A resource this device never registered is the renderer's to
            // judge; skip it and keep walking. Returning early would abandon
            // every later command, including the transfer this exists to catch.
            let resource_id = (header & 0xff == VIRGL_CCMD_TRANSFER3D
                && payload_dwords >= VIRGL_TRANSFER3D_PAYLOAD_DWORDS)
                .then(|| read_le_u32(cmdbuf, offset + VIRGL_TRANSFER3D_RESOURCE_OFFSET))
                .flatten();
            if resource_id.is_some_and(|id| self.unbacked_context_buffer(ctx_id, id)) {
                return Some(Submit3dDiagnostic {
                    renderer_status: EINVAL,
                    command_offset_dwords: u32::try_from(offset / 4).ok(),
                    command_id: Some(VIRGL_CCMD_TRANSFER3D),
                    command_header: Some(header),
                    resource_id,
                    resource_found: Some(true),
                    resource_backed: Some(false),
                });
            }
            offset = end;
        }
        None
    }
}
