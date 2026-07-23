//! Host-visible blob mapping into the shm window: map/unmap, interval allocation, rejection accounting.

use super::*;
use crate::virtio_gpu_trace::venus_start_trace_enabled;

pub(crate) fn round_up_usize(value: usize, align: usize) -> usize {
    value.div_ceil(align) * align
}

pub(crate) fn aligned_u64(value: u64, align: u64) -> bool {
    value % align == 0
}

pub(crate) fn aligned_usize(value: usize, align: usize) -> bool {
    value % align == 0
}

pub(crate) const HVF_PAGE_SIZE: u64 = 16 * 1024;

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
    pub(crate) fn resource_map_blob_into(
        &mut self,
        request: &[u8],
        hdr: CtrlHdr3d,
        out: &mut Vec<u8>,
    ) {
        if request.len() < RESOURCE_MAP_BLOB_LEN {
            venus_start_trace_map_blob_reject(
                0,
                u64::MAX,
                0,
                self.shm_window_size,
                "short request",
            );
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        }
        let resource_id = read_le_u32(request, 24).unwrap_or(0);
        let shm_offset = read_le_u64(request, 32).unwrap_or(u64::MAX);
        let Some(resource) = self.blob_resources.get(&resource_id) else {
            venus_start_trace_map_blob_reject(
                resource_id,
                shm_offset,
                0,
                self.shm_window_size,
                "unknown blob resource",
            );
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        };
        if resource.mapped.is_some() || resource.blob_mem != VIRTIO_GPU_BLOB_MEM_HOST3D {
            venus_start_trace_map_blob_reject(
                resource_id,
                shm_offset,
                resource.size,
                self.shm_window_size,
                if resource.mapped.is_some() {
                    "already mapped"
                } else {
                    "blob_mem not HOST3D"
                },
            );
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        }
        let size = resource.size;
        // Validate against the page-rounded footprint the mapping will occupy.
        let rounded_size = round_up_usize(size as usize, HVF_PAGE_SIZE as usize) as u64;
        if !aligned_u64(shm_offset, HVF_PAGE_SIZE)
            || shm_offset
                .checked_add(rounded_size)
                .is_none_or(|end| end > self.shm_window_size)
            || self.interval_overlaps(shm_offset, rounded_size)
        {
            let reason = if !aligned_u64(shm_offset, HVF_PAGE_SIZE) {
                "shm_offset not 16KiB aligned"
            } else if shm_offset
                .checked_add(rounded_size)
                .is_none_or(|end| end > self.shm_window_size)
            {
                "exceeds shm window"
            } else {
                "overlaps mapped interval"
            };
            venus_start_trace_map_blob_reject(
                resource_id,
                shm_offset,
                size,
                self.shm_window_size,
                reason,
            );
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        }
        let Some(backend) = self.backend.as_mut() else {
            venus_start_trace_map_blob_reject(
                resource_id,
                shm_offset,
                size,
                self.shm_window_size,
                "no 3D backend",
            );
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        };
        let Some(mapped) = backend.map_blob(resource_id) else {
            venus_start_trace_map_blob_reject(
                resource_id,
                shm_offset,
                size,
                self.shm_window_size,
                "backend map_blob failed",
            );
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_OUT_OF_MEMORY, Some(hdr));
            return;
        };
        // Guests may create blobs at their own (4 KiB) page granularity while
        // hv_vm_map needs 16 KiB pages. The host allocation backing a Vulkan
        // mapping is vm-page (16 KiB) granular on macOS, so it is safe to map
        // the blob's pages rounded up to the HVF page size as long as the host
        // pointer itself is page-aligned; the guest-visible blob size stays
        // `size`.
        let map_size = rounded_size as usize;
        if mapped.host_ptr.is_null()
            || !aligned_usize(mapped.host_ptr as usize, HVF_PAGE_SIZE as usize)
            || (mapped.size as u64) < size
        {
            venus_start_trace_map_blob_reject(
                resource_id,
                shm_offset,
                size,
                self.shm_window_size,
                if mapped.host_ptr.is_null() {
                    "backend host_ptr null"
                } else if !aligned_usize(mapped.host_ptr as usize, HVF_PAGE_SIZE as usize) {
                    "backend host_ptr unaligned"
                } else {
                    "backend mapping smaller than blob"
                },
            );
            backend.unmap_blob(resource_id);
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_OUT_OF_MEMORY, Some(hdr));
            return;
        }
        let Some(port) = self.shm_port.as_mut() else {
            venus_start_trace_map_blob_reject(
                resource_id,
                shm_offset,
                size,
                self.shm_window_size,
                "no shm map port",
            );
            backend.unmap_blob(resource_id);
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_OUT_OF_MEMORY, Some(hdr));
            return;
        };
        if port.map(mapped.host_ptr, map_size, shm_offset).is_err() {
            venus_start_trace_map_blob_reject(
                resource_id,
                shm_offset,
                size,
                self.shm_window_size,
                "shm port map failed",
            );
            backend.unmap_blob(resource_id);
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_OUT_OF_MEMORY, Some(hdr));
            return;
        }
        if let Some(resource) = self.blob_resources.get_mut(&resource_id) {
            resource.mapped = Some((shm_offset, map_size));
        }
        self.mapped_intervals
            .insert(shm_offset, (map_size as u64, resource_id));
        if venus_start_trace_enabled() {
            println!(
                "venus-start: map_blob OK resource={resource_id} shm_offset={shm_offset:#x} size={size} map_size={map_size} map_info={:#x}",
                mapped.map_info & VIRTIO_GPU_MAP_CACHE_MASK
            );
        }
        response_hdr_into(out, VIRTIO_GPU_RESP_OK_MAP_INFO, Some(hdr));
        out.extend_from_slice(&(mapped.map_info & VIRTIO_GPU_MAP_CACHE_MASK).to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
    }

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

    pub(crate) fn unmap_blob_resource(&mut self, resource_id: u32) {
        let Some((shm_offset, mapped_size)) = self
            .blob_resources
            .get_mut(&resource_id)
            .and_then(|resource| resource.mapped.take())
        else {
            return;
        };
        if let Some(port) = self.shm_port.as_mut() {
            let _ = port.unmap(shm_offset, mapped_size);
        }
        if let Some(backend) = self.backend.as_mut() {
            backend.unmap_blob(resource_id);
        }
        self.mapped_intervals.remove(&shm_offset);
    }

    pub(crate) fn unmap_all_blobs(&mut self) {
        self.blob_unmap_ids_scratch.clear();
        self.blob_unmap_ids_scratch
            .extend(self.blob_resources.keys().copied());
        let mut ids = std::mem::take(&mut self.blob_unmap_ids_scratch);
        for resource_id in ids.drain(..) {
            self.unmap_blob_resource(resource_id);
        }
        self.blob_unmap_ids_scratch = ids;
    }

    pub(crate) fn interval_overlaps(&self, start: u64, size: u64) -> bool {
        let Some(end) = start.checked_add(size) else {
            return true;
        };
        self.mapped_intervals
            .iter()
            .any(|(other_start, (other_size, _))| {
                let other_end = other_start.saturating_add(*other_size);
                start < other_end && *other_start < end
            })
    }
}
