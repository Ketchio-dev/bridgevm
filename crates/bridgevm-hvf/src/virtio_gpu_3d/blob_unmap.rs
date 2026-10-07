//! Blob unmap requests and their classified rejection counters.

use super::*;

/// Classified `RESOURCE_UNMAP_BLOB` invalid-parameter rejections. The guest
/// driver's cleanup order determines which class fires: an unmap that arrives
/// after `RESOURCE_UNREF` of a still-mapped blob is late-but-harmless cleanup
/// (the host already unmapped at destroy), while `never_created` points at a
/// real mapping-lifecycle bug or resource-id confusion.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct UnmapBlobRejectCounts {
    pub short_request: u64,
    pub destroyed_while_mapped: u64,
    pub destroyed_after_unmap: u64,
    pub never_created: u64,
}

impl UnmapBlobRejectCounts {
    pub fn total(&self) -> u64 {
        self.short_request
            + self.destroyed_while_mapped
            + self.destroyed_after_unmap
            + self.never_created
    }
}

impl VirtioGpu3d {
    pub(crate) fn resource_unmap_blob_into(
        &mut self,
        request: &[u8],
        hdr: CtrlHdr3d,
        out: &mut Vec<u8>,
    ) {
        if request.len() < RESOURCE_UNMAP_BLOB_LEN {
            self.unmap_blob_reject_counts.short_request += 1;
            venus_start_trace_unmap_blob_reject(0, "short_request");
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        }
        let resource_id = read_le_u32(request, 24).unwrap_or(0);
        if !self.blob_resources.contains_key(&resource_id) {
            let reason = if self.destroyed_blob_mapped_ids.contains(&resource_id) {
                self.unmap_blob_reject_counts.destroyed_while_mapped += 1;
                "already_destroyed_was_mapped"
            } else if self.destroyed_blob_unmapped_ids.contains(&resource_id) {
                self.unmap_blob_reject_counts.destroyed_after_unmap += 1;
                "already_destroyed_was_unmapped"
            } else {
                self.unmap_blob_reject_counts.never_created += 1;
                "never_created"
            };
            venus_start_trace_unmap_blob_reject(resource_id, reason);
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        }
        self.unmap_blob_resource(resource_id);
        response_hdr_into(out, VIRTIO_GPU_RESP_OK_NODATA, Some(hdr));
    }

    pub fn unmap_blob_reject_counts(&self) -> UnmapBlobRejectCounts {
        self.unmap_blob_reject_counts
    }
}
