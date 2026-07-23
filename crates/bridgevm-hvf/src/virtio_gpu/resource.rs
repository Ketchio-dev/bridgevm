//! 2D resource lifecycle: create, unref, backing attach/detach, TRANSFER_TO_HOST_2D.

use super::*;
use crate::fwcfg::GuestMemoryMut;
use crate::virtio_gpu_3d::BlobMemEntry;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GpuResource {
    pub(crate) format: u32,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) host_pixels: Vec<u8>,
    pub(crate) backing: Vec<BackingEntry>,
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
    let stride = u64::from(resource.width) * 4;
    let row_bytes = ((x_end - rect.x) as usize) * 4;
    // Per the virtio-gpu spec (and QEMU), `offset` locates the box's top-left
    // (rect.x, rect.y) in the backing; source rows advance by `stride` from
    // there. So the backing offset for absolute pixel (x, y) is
    // offset + (y - rect.y) * stride + (x - rect.x) * 4 — NOT offset + y*stride
    // + x*4, which double-counts rect.{x,y} and sends every non-origin partial
    // update (taskbar, clock, cursor) out of bounds so it silently vanishes.
    for y in rect.y..y_end {
        let guest_row_off = offset + u64::from(y - rect.y) * stride;
        let dst_row = ((y as usize) * (resource.width as usize) + (rect.x as usize)) * 4;
        if read_from_backing_into(
            mem,
            &resource.backing,
            guest_row_off,
            &mut resource.host_pixels[dst_row..dst_row + row_bytes],
        ) {
            continue;
        }
        for x in rect.x..x_end {
            let guest_off = guest_row_off + u64::from(x - rect.x) * 4;
            let mut pixel = [0u8; 4];
            if !read_from_backing_into(mem, &resource.backing, guest_off, &mut pixel) {
                continue;
            }
            let dst = ((y as usize) * (resource.width as usize) + (x as usize)) * 4;
            resource.host_pixels[dst..dst + 4].copy_from_slice(&pixel);
        }
    }
}

pub(crate) fn read_from_backing_into(
    mem: &dyn GuestMemoryMut,
    backing: &[BackingEntry],
    offset: u64,
    dst: &mut [u8],
) -> bool {
    let mut base = 0u64;
    let Ok(len_u64) = u64::try_from(dst.len()) else {
        return false;
    };
    for entry in backing {
        let Some(entry_end) = base.checked_add(u64::from(entry.len)) else {
            return false;
        };
        if offset >= base
            && offset
                .checked_add(len_u64)
                .is_some_and(|range_end| range_end <= entry_end)
        {
            let rel = offset - base;
            return mem.read_into(entry.addr + rel, dst);
        }
        base = entry_end;
    }
    false
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
        else {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_UNSPEC, hdr);
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
            },
        );
        self.three_d.register_2d_resource(resource_id);
        response_hdr_into(out, VIRTIO_GPU_RESP_OK_NODATA, hdr);
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
        let mut backing = Vec::with_capacity(nr_entries as usize);
        let mut offset = 32usize;
        for _ in 0..nr_entries {
            let addr = read_le_u64(request, offset).unwrap();
            let len = read_le_u32(request, offset + 8).unwrap();
            backing.push(BlobMemEntry { addr, len });
            offset += 16;
        }
        if let Some(resource) = self.resources.get_mut(&resource_id) {
            resource.backing.clear();
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
        let Some(resource) = self.resources.get_mut(&resource_id) else {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_UNSPEC, hdr);
            return;
        };
        copy_backing_to_resource(mem, resource, rect, offset);
        response_hdr_into(out, VIRTIO_GPU_RESP_OK_NODATA, hdr);
    }
}
