//! Capset negotiation: count, GET_CAPSET_INFO, GET_CAPSET.

use super::*;

impl VirtioGpu3d {
    pub fn capset_count(&self) -> u32 {
        self.backend
            .as_ref()
            .map_or(0, |backend| backend.capset_count())
    }

    pub(crate) fn get_capset_info_into(
        &mut self,
        request: &[u8],
        hdr: CtrlHdr3d,
        out: &mut Vec<u8>,
    ) {
        let Some(backend) = self.backend.as_mut() else {
            venus_start_trace_reject("get_capset_info", "no 3D backend");
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        };
        let Some(index) = read_le_u32(request, 24) else {
            venus_start_trace_reject("get_capset_info", "short request");
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        };
        let Some(info) = backend.capset_info(index) else {
            venus_start_trace_reject("get_capset_info", "backend has no capset at index");
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        };
        response_hdr_into(out, VIRTIO_GPU_RESP_OK_CAPSET_INFO, Some(hdr));
        out.extend_from_slice(&info.capset_id.to_le_bytes());
        out.extend_from_slice(&info.max_version.to_le_bytes());
        out.extend_from_slice(&info.max_size.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
    }

    pub(crate) fn get_capset_into(&mut self, request: &[u8], hdr: CtrlHdr3d, out: &mut Vec<u8>) {
        let Some(backend) = self.backend.as_mut() else {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        };
        let Some(capset_id) = read_le_u32(request, 24) else {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        };
        let Some(version) = read_le_u32(request, 28) else {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        };
        let response_start = out.len();
        response_hdr_into(out, VIRTIO_GPU_RESP_OK_CAPSET, Some(hdr));
        if !backend.capset_into(capset_id, version, out) {
            venus_start_trace_reject("get_capset", "backend capset_into failed");
            out.truncate(response_start);
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
        }
    }
}
