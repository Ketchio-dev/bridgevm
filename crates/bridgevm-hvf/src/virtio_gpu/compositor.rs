//! Pixel compositing from resources and blobs into the scanout buffer, with format conversion.

use super::*;
use crate::fwcfg::GuestMemoryMut;
use crate::virtio_gpu_3d;
use crate::virtio_gpu_3d::BlobMemEntry;
use crate::virtio_gpu_3d::Create3dArgs;

pub(crate) fn composite_resource_to_scanout(
    scanout: &mut [u8],
    scanout_width: u32,
    scanout_height: u32,
    resource: &GpuResource,
    rect: Rect,
) {
    let x_end = rect
        .x
        .saturating_add(rect.width)
        .min(scanout_width)
        .min(resource.width);
    let y_end = rect
        .y
        .saturating_add(rect.height)
        .min(scanout_height)
        .min(resource.height);
    for y in rect.y..y_end {
        for x in rect.x..x_end {
            let src = ((y as usize) * (resource.width as usize) + (x as usize)) * 4;
            let dst = ((y as usize) * (scanout_width as usize) + (x as usize)) * 4;
            let pixel = &resource.host_pixels[src..src + 4];
            scanout[dst..dst + 4].copy_from_slice(&to_xrgb8888(pixel, resource.format));
        }
    }
}

pub(crate) fn composite_host_3d_to_scanout(
    pixels: &[u8],
    resource_width: u32,
    resource_height: u32,
    scanout: &mut [u8],
    scanout_width: u32,
    scanout_height: u32,
    rect: Rect,
) -> bool {
    if pixels.len() < scanout_len(resource_width, resource_height)
        || scanout.len() < scanout_len(scanout_width, scanout_height)
    {
        return false;
    }
    let x_end = rect
        .x
        .saturating_add(rect.width)
        .min(resource_width)
        .min(scanout_width);
    let y_end = rect
        .y
        .saturating_add(rect.height)
        .min(resource_height)
        .min(scanout_height);
    if x_end <= rect.x || y_end <= rect.y {
        return false;
    }

    let row_bytes = ((x_end - rect.x) as usize) * 4;
    for y in rect.y..y_end {
        let src = ((y as usize) * (resource_width as usize) + (rect.x as usize)) * 4;
        let dst = ((y as usize) * (scanout_width as usize) + (rect.x as usize)) * 4;
        scanout[dst..dst + row_bytes].copy_from_slice(&pixels[src..src + row_bytes]);
    }
    true
}

pub(crate) fn composite_local_3d_to_scanout(
    mem: &dyn GuestMemoryMut,
    backing: &[BlobMemEntry],
    info: Create3dArgs,
    scanout: &mut [u8],
    scanout_width: u32,
    scanout_height: u32,
    rect: Rect,
    row_pixels: &mut Vec<u8>,
) -> bool {
    if backing.is_empty() || !format_supported(info.format) {
        return false;
    }
    let x_end = rect
        .x
        .saturating_add(rect.width)
        .min(info.width)
        .min(scanout_width);
    let y_end = rect
        .y
        .saturating_add(rect.height)
        .min(info.height)
        .min(scanout_height);
    if x_end <= rect.x || y_end <= rect.y {
        return false;
    }

    let resource_stride = u64::from(info.width) * 4;
    let row_bytes = ((x_end - rect.x) as usize) * 4;
    row_pixels.resize(row_bytes, 0);
    let mut copied_any = false;
    for y in rect.y..y_end {
        let row_offset = u64::from(y) * resource_stride + u64::from(rect.x) * 4;
        if read_from_blob_backing_into(mem, backing, row_offset, row_pixels) {
            for x in rect.x..x_end {
                let src = ((x - rect.x) as usize) * 4;
                let dst = ((y as usize) * (scanout_width as usize) + (x as usize)) * 4;
                scanout[dst..dst + 4]
                    .copy_from_slice(&to_xrgb8888(&row_pixels[src..src + 4], info.format));
            }
            copied_any = true;
            continue;
        }
        for x in rect.x..x_end {
            let offset = u64::from(y) * resource_stride + u64::from(x) * 4;
            let mut pixel = [0u8; 4];
            if !read_from_blob_backing_into(mem, backing, offset, &mut pixel) {
                continue;
            }
            let dst = ((y as usize) * (scanout_width as usize) + (x as usize)) * 4;
            scanout[dst..dst + 4].copy_from_slice(&to_xrgb8888(&pixel, info.format));
            copied_any = true;
        }
    }
    row_pixels.clear();
    copied_any
}

pub(crate) struct GuestBlobComposite<'a> {
    pub(crate) mem: &'a dyn GuestMemoryMut,
    pub(crate) backing: &'a [virtio_gpu_3d::BlobMemEntry],
    pub(crate) scanout: &'a mut [u8],
    pub(crate) scanout_width: u32,
    pub(crate) blob: &'a BlobScanout,
    pub(crate) row_pixels: &'a mut Vec<u8>,
}

pub(crate) fn composite_guest_blob_to_scanout(
    composite: GuestBlobComposite<'_>,
    rect: Rect,
    x_end: u32,
    y_end: u32,
) {
    composite.row_pixels.clear();
    if x_end <= rect.x || y_end <= rect.y {
        return;
    }
    let row_bytes = ((x_end - rect.x) as usize) * 4;
    composite.row_pixels.resize(row_bytes, 0);
    for y in rect.y..y_end {
        let row_src = u64::from(composite.blob.offset)
            + u64::from(y) * u64::from(composite.blob.stride)
            + u64::from(rect.x) * 4;
        if read_from_blob_backing_into(
            composite.mem,
            composite.backing,
            row_src,
            composite.row_pixels,
        ) {
            for x in rect.x..x_end {
                let src = ((x - rect.x) as usize) * 4;
                let dst = ((y as usize) * (composite.scanout_width as usize) + (x as usize)) * 4;
                composite.scanout[dst..dst + 4].copy_from_slice(&to_xrgb8888(
                    &composite.row_pixels[src..src + 4],
                    composite.blob.format,
                ));
            }
            continue;
        }
        for x in rect.x..x_end {
            let src = u64::from(composite.blob.offset)
                + u64::from(y) * u64::from(composite.blob.stride)
                + u64::from(x) * 4;
            let mut pixel = [0u8; 4];
            if !read_from_blob_backing_into(composite.mem, composite.backing, src, &mut pixel) {
                continue;
            }
            let dst = ((y as usize) * (composite.scanout_width as usize) + (x as usize)) * 4;
            composite.scanout[dst..dst + 4]
                .copy_from_slice(&to_xrgb8888(&pixel, composite.blob.format));
        }
    }
    composite.row_pixels.clear();
}

pub(crate) fn composite_host_blob_to_scanout(
    pixels: &[u8],
    scanout: &mut [u8],
    scanout_width: u32,
    blob: &BlobScanout,
    rect: Rect,
    x_end: u32,
    y_end: u32,
) {
    for y in rect.y..y_end {
        for x in rect.x..x_end {
            let src = (blob.offset as usize)
                .saturating_add((y as usize).saturating_mul(blob.stride as usize))
                .saturating_add((x as usize).saturating_mul(4));
            if !matches!(src.checked_add(4), Some(end) if end <= pixels.len()) {
                continue;
            }
            let dst = ((y as usize) * (scanout_width as usize) + (x as usize)) * 4;
            scanout[dst..dst + 4].copy_from_slice(&to_xrgb8888(&pixels[src..src + 4], blob.format));
        }
    }
}

pub(crate) fn to_xrgb8888(pixel: &[u8], format: u32) -> [u8; 4] {
    match format {
        FORMAT_B8G8R8A8_UNORM | FORMAT_B8G8R8X8_UNORM => [pixel[0], pixel[1], pixel[2], 0],
        FORMAT_X8R8G8B8_UNORM => [pixel[3], pixel[2], pixel[1], 0],
        FORMAT_R8G8B8X8_UNORM => [pixel[2], pixel[1], pixel[0], 0],
        _ => [0, 0, 0, 0],
    }
}

pub(crate) fn read_from_blob_backing_into(
    mem: &dyn GuestMemoryMut,
    backing: &[virtio_gpu_3d::BlobMemEntry],
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
