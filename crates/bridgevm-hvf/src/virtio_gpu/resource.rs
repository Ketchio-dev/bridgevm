//! 2D resource lifecycle: create, unref, backing attach/detach, TRANSFER_TO_HOST_2D.

use super::backing_copy_cursor::{read_from_backing_into_from, BackingReadCursor};
use super::*;
use crate::fwcfg::GuestMemoryMut;
use crate::virtio_gpu_3d::BlobMemEntry;
use crate::virtio_gpu_3d::VIRTIO_GPU_RESP_ERR_OUT_OF_MEMORY;

/// The cumulative 2D pixel and retained backing-storage limit. Requests that
/// would reach it fail with ERR_OUT_OF_MEMORY.
pub(crate) const MAX_2D_HOSTMEM: usize = 256 << 20;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GpuResource {
    pub(crate) format: u32,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) host_pixels: Vec<u8>,
    pub(crate) backing: Vec<BackingEntry>,
    /// Logical byte length of the attached backing; moves with unlocked copies.
    pub(crate) backing_len: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BackingEntry {
    pub(crate) addr: u64,
    pub(crate) len: u32,
}

pub(crate) fn copy_backing_to_resource(
    mem: &dyn GuestMemoryMut,
    resource: &mut GpuResource,
    rect: Rect,
    offset: u64,
) {
    let x_end = rect.x.saturating_add(rect.width).min(resource.width);
    let y_end = rect.y.saturating_add(rect.height).min(resource.height);
    if x_end <= rect.x || y_end <= rect.y {
        return;
    }
    let backing_len = resource.backing_len;
    let stride = u64::from(resource.width) * 4;
    let row_bytes = ((x_end - rect.x) as usize) * 4;
    let mut cursor = BackingReadCursor::default();
    // Per the OASIS virtio-gpu contract, `offset` locates the box's top-left
    // (rect.x, rect.y) in the backing; source rows advance by `stride` from
    // there. So the backing offset for absolute pixel (x, y) is
    // offset + (y - rect.y) * stride + (x - rect.x) * 4 — NOT offset + y*stride
    // + x*4, which double-counts rect.{x,y} and sends every non-origin partial
    // update (taskbar, clock, cursor) out of bounds so it silently vanishes.
    for y in rect.y..y_end {
        let Some(guest_row_off) = offset.checked_add(u64::from(y - rect.y) * stride) else {
            return;
        };
        // Preserve complete pixels in a partially covered row, but never retry
        // its uncovered suffix once per pixel.
        let covered_bytes = backing_len
            .saturating_sub(guest_row_off)
            .min(row_bytes as u64) as usize
            / 4
            * 4;
        if covered_bytes == 0 {
            return;
        }
        let dst_row = ((y as usize) * (resource.width as usize) + (rect.x as usize)) * 4;
        if read_from_backing_into_from(
            mem,
            &resource.backing,
            guest_row_off,
            &mut resource.host_pixels[dst_row..dst_row + covered_bytes],
            &mut cursor,
        ) {
            continue;
        }
        // A memory hole may invalidate a row read while later pixels remain
        // readable. Reuse a forward cursor for that legitimate fallback.
        let mut pixel_cursor = BackingReadCursor::default();
        for x in rect.x..rect.x + (covered_bytes / 4) as u32 {
            let Some(guest_off) = guest_row_off.checked_add(u64::from(x - rect.x) * 4) else {
                break;
            };
            let mut pixel = [0u8; 4];
            if !read_from_backing_into_from(
                mem,
                &resource.backing,
                guest_off,
                &mut pixel,
                &mut pixel_cursor,
            ) {
                continue;
            }
            let dst = ((y as usize) * (resource.width as usize) + (x as usize)) * 4;
            resource.host_pixels[dst..dst + 4].copy_from_slice(&pixel);
        }
    }
}

impl VirtioGpu {
    pub(crate) fn resource_create_2d_into(
        &mut self,
        request: &[u8],
        hdr: Option<CtrlHdr>,
        out: &mut Vec<u8>,
    ) {
        let Some(resource_id) = read_le_u32(request, 24) else {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_UNSPEC, hdr);
            return;
        };
        let format = read_le_u32(request, 28).unwrap_or(0);
        let width = read_le_u32(request, 32).unwrap_or(0);
        let height = read_le_u32(request, 36).unwrap_or(0);
        if resource_id == 0 || width == 0 || height == 0 || !format_supported(format) {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_UNSPEC, hdr);
            return;
        }
        let Some(len) = u64::from(width)
            .checked_mul(u64::from(height))
            .and_then(|pixels| pixels.checked_mul(4))
            .and_then(|bytes| usize::try_from(bytes).ok())
            .filter(|len| self.hostmem_after_create(resource_id, *len) < MAX_2D_HOSTMEM)
        else {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_OUT_OF_MEMORY, hdr);
            return;
        };
        self.resources.insert(
            resource_id,
            GpuResource {
                format,
                width,
                height,
                host_pixels: vec![0; len],
                backing: Vec::new(),
                backing_len: 0,
            },
        );
        self.three_d.register_2d_resource(resource_id);
        response_hdr_into(out, VIRTIO_GPU_RESP_OK_NODATA, hdr);
    }

    fn hostmem_after_create(&self, resource_id: u32, len: usize) -> usize {
        let in_use = self
            .resources
            .iter()
            .filter(|(id, _)| **id != resource_id)
            .map(|(_, resource)| {
                resource.host_pixels.len()
                    + resource.backing.capacity() * std::mem::size_of::<BackingEntry>()
            })
            .sum::<usize>();
        in_use.saturating_add(len)
    }

    pub(crate) fn resource_unref_into(
        &mut self,
        request: &[u8],
        hdr: Option<CtrlHdr>,
        out: &mut Vec<u8>,
    ) {
        if let Some(resource_id) = read_le_u32(request, 24) {
            if self
                .blob_scanout
                .as_ref()
                .map(|scanout| scanout.resource_id)
                == Some(resource_id)
            {
                self.unbind_blob_scanout();
            }
            self.resources.remove(&resource_id);
            self.three_d.unref_resource(resource_id);
            if self.scanout_resource == Some(resource_id) {
                self.scanout_resource = None;
            }
        }
        response_hdr_into(out, VIRTIO_GPU_RESP_OK_NODATA, hdr);
    }

    pub(crate) fn attach_backing_into(
        &mut self,
        mem: &dyn GuestMemoryMut,
        request: &[u8],
        hdr: Option<CtrlHdr>,
        out: &mut Vec<u8>,
    ) {
        let Some(resource_id) = read_le_u32(request, 24) else {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_UNSPEC, hdr);
            return;
        };
        let nr_entries = read_le_u32(request, 28).unwrap_or(0);
        let Some(entries_len) = (nr_entries as usize).checked_mul(16) else {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_UNSPEC, hdr);
            return;
        };
        if request.len().saturating_sub(32) < entries_len {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_UNSPEC, hdr);
            return;
        }
        if let Some(resource) = self.resources.get(&resource_id) {
            let retained = resource.backing.capacity().max(nr_entries as usize)
                * std::mem::size_of::<BackingEntry>();
            let total = self
                .hostmem_after_create(resource_id, resource.host_pixels.len())
                .saturating_add(retained);
            if total >= MAX_2D_HOSTMEM {
                response_hdr_into(out, VIRTIO_GPU_RESP_ERR_OUT_OF_MEMORY, hdr);
                return;
            }
        }
        let mut backing = Vec::with_capacity(nr_entries as usize);
        let mut offset = 32usize;
        let mut backing_len = 0u64;
        for _ in 0..nr_entries {
            let addr = read_le_u64(request, offset).unwrap();
            let len = read_le_u32(request, offset + 8).unwrap();
            backing.push(BlobMemEntry { addr, len });
            backing_len += u64::from(len);
            offset += 16;
        }
        if let Some(resource) = self.resources.get_mut(&resource_id) {
            resource.backing.clear();
            resource.backing_len = backing_len;
            // Reserve exactly when growing: spare capacity is retained host memory.
            resource.backing.reserve_exact(nr_entries as usize);
            resource
                .backing
                .extend(backing.iter().map(|entry| BackingEntry {
                    addr: entry.addr,
                    len: entry.len,
                }));
        } else if self.three_d.is_3d_resource(resource_id) {
            if !self.three_d.attach_3d_backing(mem, resource_id, &backing) {
                response_hdr_into(out, VIRTIO_GPU_RESP_ERR_UNSPEC, hdr);
                return;
            }
        } else {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_UNSPEC, hdr);
            return;
        }
        response_hdr_into(out, VIRTIO_GPU_RESP_OK_NODATA, hdr);
    }

    pub(crate) fn detach_backing_into(
        &mut self,
        request: &[u8],
        hdr: Option<CtrlHdr>,
        out: &mut Vec<u8>,
    ) {
        if let Some(resource_id) = read_le_u32(request, 24) {
            if let Some(resource) = self.resources.get_mut(&resource_id) {
                resource.backing.clear();
                resource.backing_len = 0;
            } else if self.three_d.is_3d_resource(resource_id)
                && !self.three_d.detach_3d_backing(resource_id)
            {
                response_hdr_into(out, VIRTIO_GPU_RESP_ERR_UNSPEC, hdr);
                return;
            }
        }
        response_hdr_into(out, VIRTIO_GPU_RESP_OK_NODATA, hdr);
    }

    pub(crate) fn transfer_to_host_2d_into(
        &mut self,
        mem: &dyn GuestMemoryMut,
        request: &[u8],
        hdr: Option<CtrlHdr>,
        out: &mut Vec<u8>,
    ) {
        let rect = read_rect(request, 24).unwrap_or(Rect {
            x: 0,
            y: 0,
            width: 0,
            height: 0,
        });
        let offset = read_le_u64(request, 40).unwrap_or(0);
        let resource_id = read_le_u32(request, 48).unwrap_or(0);
        if self.queue_transfer(resource_id, rect, offset) {
            response_hdr_into(out, VIRTIO_GPU_RESP_OK_NODATA, hdr);
            return;
        }
        let Some(resource) = self.resources.get_mut(&resource_id) else {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_UNSPEC, hdr);
            return;
        };
        copy_backing_to_resource(mem, resource, rect, offset);
        response_hdr_into(out, VIRTIO_GPU_RESP_OK_NODATA, hdr);
    }
}
