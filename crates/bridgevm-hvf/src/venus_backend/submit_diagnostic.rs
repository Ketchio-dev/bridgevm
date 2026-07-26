// Conversion between virglrenderer's bounded submit diagnostic ABI and Rust values.

use crate::virtio_gpu_3d::Submit3dDiagnostic;

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct VirglSubmitDiagnostic {
    pub renderer_status: c_int,
    pub command_offset_dwords: u32,
    pub command_id: u32,
    pub command_header: u32,
    pub resource_id: u32,
    pub flags: u32,
}

pub(crate) const VALID_COMMAND: u32 = 1 << 0;
pub(crate) const VALID_RESOURCE: u32 = 1 << 1;
pub(crate) const RESOURCE_FOUND: u32 = 1 << 2;
pub(crate) const RESOURCE_BACKED: u32 = 1 << 3;

pub(crate) fn take_submit_diagnostic(renderer_status: i32) -> Submit3dDiagnostic {
    let mut raw = VirglSubmitDiagnostic::default();
    if unsafe { virgl_renderer_bridgevm_get_last_submit_diagnostic(&mut raw) } != 0
        || raw.renderer_status != renderer_status
    {
        return Submit3dDiagnostic {
            renderer_status,
            ..Submit3dDiagnostic::default()
        };
    }
    Submit3dDiagnostic {
        renderer_status,
        command_offset_dwords: (raw.flags & VALID_COMMAND != 0)
            .then_some(raw.command_offset_dwords),
        command_id: (raw.flags & VALID_COMMAND != 0).then_some(raw.command_id),
        command_header: (raw.flags & VALID_COMMAND != 0).then_some(raw.command_header),
        resource_id: (raw.flags & VALID_RESOURCE != 0).then_some(raw.resource_id),
        resource_found: (raw.flags & VALID_RESOURCE != 0)
            .then_some(raw.flags & RESOURCE_FOUND != 0),
        resource_backed: (raw.flags & VALID_RESOURCE != 0)
            .then_some(raw.flags & RESOURCE_BACKED != 0),
    }
}
