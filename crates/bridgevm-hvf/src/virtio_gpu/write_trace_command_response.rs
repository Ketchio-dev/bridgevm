//! Split out of virtio_gpu.rs to keep files under 850 lines.

use super::*;
use crate::fwcfg::GuestMemoryMut;
use crate::pcie::VIRTIO_GPU_MSIX_VECTOR_COUNT;
use crate::virtio_gpu_3d;
use crate::virtio_gpu_3d::BlobMemEntry;
use crate::virtio_gpu_3d::Create3dArgs;
use std::fmt::Write as _;
use std::fs::File;
use std::fs::OpenOptions;
use std::os::unix::io::AsRawFd;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::OnceLock;

pub(crate) fn write_trace_command_response_details(
    out: &mut String,
    response_type: u32,
    response: &[u8],
) {
    match response_type {
        VIRTIO_GPU_RESP_OK_DISPLAY_INFO => {
            let _ = write!(
                out,
                ",\"response_scanout0_x\":{},\"response_scanout0_y\":{},\"response_scanout0_width\":{},\"response_scanout0_height\":{},\"response_scanout0_enabled\":{},\"response_scanout0_flags\":{}",
                read_le_u32(response, 24).unwrap_or(0),
                read_le_u32(response, 28).unwrap_or(0),
                read_le_u32(response, 32).unwrap_or(0),
                read_le_u32(response, 36).unwrap_or(0),
                read_le_u32(response, 40).unwrap_or(0),
                read_le_u32(response, 44).unwrap_or(0)
            );
        }
        VIRTIO_GPU_RESP_OK_EDID => {
            let edid_size = read_le_u32(response, 24).unwrap_or(0) as usize;
            let available = response.len().saturating_sub(32);
            let checksum_valid = edid_size > 0
                && edid_size <= available
                && response[32..32 + edid_size]
                    .iter()
                    .fold(0u8, |sum, byte| sum.wrapping_add(*byte))
                    == 0;
            let _ = write!(
                out,
                ",\"response_edid_size\":{},\"response_edid_checksum_valid\":{}",
                edid_size, checksum_valid
            );
        }
        virtio_gpu_3d::VIRTIO_GPU_RESP_OK_CAPSET_INFO => {
            let _ = write!(
                out,
                ",\"response_capset_id\":{},\"response_capset_max_version\":{},\"response_capset_max_size\":{}",
                read_le_u32(response, 24).unwrap_or(0),
                read_le_u32(response, 28).unwrap_or(0),
                read_le_u32(response, 32).unwrap_or(0)
            );
        }
        virtio_gpu_3d::VIRTIO_GPU_RESP_OK_CAPSET => {
            let _ = write!(
                out,
                ",\"response_capset_bytes\":{}",
                response.len().saturating_sub(24)
            );
        }
        _ => {}
    }
}

pub(crate) fn write_descriptor_lengths(out: &mut String, descs: &[Descriptor], writable: bool) {
    let mut first = true;
    for desc in descs {
        if (desc.flags & DESC_F_WRITE != 0) != writable {
            continue;
        }
        if !first {
            out.push(',');
        }
        first = false;
        let _ = write!(out, "{}", desc.len);
    }
}

/// Bytes of SUBMIT_3D payload preserved in the JSONL trace. The 32-byte
/// default identifies the leading command; raising it via
/// BRIDGEVM_VIRTIO_GPU_TRACE_SUBMIT_PREFIX captures whole command streams for
/// offline decoding when diagnosing renderer-level divergence.
pub(crate) fn submit_trace_prefix_len() -> usize {
    static LEN: OnceLock<usize> = OnceLock::new();
    *LEN.get_or_init(|| {
        std::env::var("BRIDGEVM_VIRTIO_GPU_TRACE_SUBMIT_PREFIX")
            .ok()
            .and_then(|value| value.trim().parse::<usize>().ok())
            .filter(|&value| value > 0)
            .unwrap_or(32)
    })
}

pub(crate) fn write_hex_prefix_json(out: &mut String, bytes: &[u8], max_len: usize) {
    out.push('"');
    write_hex_prefix(out, bytes, max_len);
    out.push('"');
}

pub(crate) fn write_hex_prefix(out: &mut String, bytes: &[u8], max_len: usize) {
    for (index, byte) in bytes.iter().take(max_len).enumerate() {
        if index > 0 {
            out.push(' ');
        }
        let _ = write!(out, "{byte:02x}");
    }
    if bytes.len() > max_len {
        out.push_str(" ...");
    }
}

#[cfg(test)]
pub(crate) fn hex_prefix(bytes: &[u8], max_len: usize) -> String {
    let prefix_len = bytes.len().min(max_len);
    let mut out = String::with_capacity(prefix_len.saturating_mul(3).saturating_add(4));
    write_hex_prefix(&mut out, bytes, max_len);
    out
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

pub(crate) fn blob_surface_footprint(
    width: u32,
    height: u32,
    stride: u32,
    offset: u32,
) -> Option<u64> {
    u64::from(offset)
        .checked_add(u64::from(height.saturating_sub(1)).checked_mul(u64::from(stride))?)?
        .checked_add(u64::from(width).checked_mul(4)?)
}

pub(crate) fn build_edid(width: u32, height: u32) -> [u8; 128] {
    let mut edid = [0u8; 128];
    edid[0..8].copy_from_slice(&[0x00, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x00]);
    edid[8..10].copy_from_slice(&encode_manufacturer("BVM"));
    edid[10..12].copy_from_slice(&0x0001u16.to_le_bytes());
    edid[12..16].copy_from_slice(&1u32.to_le_bytes());
    edid[16] = 1;
    edid[17] = 34;
    edid[18] = 1;
    edid[19] = 4;
    edid[20] = 0xa5;
    edid[21] = ((width / 100).clamp(1, 255)) as u8;
    edid[22] = ((height / 100).clamp(1, 255)) as u8;
    edid[23] = 0x78;
    edid[24] = 0x0a;
    edid[25] = 0xcf;
    edid[26] = 0x74;
    edid[27] = 0xa3;
    edid[28] = 0x57;
    edid[29] = 0x4c;
    edid[30] = 0xb0;
    edid[31] = 0x23;
    edid[32] = 0x09;
    edid[35] = 0x81;
    edid[36] = 0x80;

    let dtd = detailed_timing_descriptor(width, height, 120);
    let pixel_clock_10khz = u16::from_le_bytes([dtd[0], dtd[1]]);
    let max_pixel_clock_10mhz = pixel_clock_10khz.div_ceil(1_000) as u8;
    edid[54..72].copy_from_slice(&dtd);
    edid[72..90].copy_from_slice(&monitor_descriptor(
        0xfd,
        &[
            48,
            144,
            30,
            160,
            max_pixel_clock_10mhz,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
        ],
    ));
    edid[90..108].copy_from_slice(&monitor_descriptor_text(0xfc, b"BridgeVM GPU"));
    edid[108..126].copy_from_slice(&monitor_descriptor_text(0xfe, b"virtio-gpu"));
    edid[126] = 0;
    let sum = edid[..127]
        .iter()
        .fold(0u8, |acc, byte| acc.wrapping_add(*byte));
    edid[127] = 0u8.wrapping_sub(sum);
    edid
}

pub(crate) fn detailed_timing_descriptor(width: u32, height: u32, refresh_hz: u32) -> [u8; 18] {
    let h_blank = 160u32.max(width / 8);
    let v_blank = 45u32.max(height / 20);
    let h_sync_offset = 48u32.min(h_blank / 3);
    let h_sync_width = 32u32.min(h_blank.saturating_sub(h_sync_offset).max(1));
    let v_sync_offset = 3u32;
    let v_sync_width = 5u32;
    let requested_pixel_clock_10khz = ((u64::from(width) + u64::from(h_blank))
        * (u64::from(height) + u64::from(v_blank))
        * u64::from(refresh_hz)
        / 10_000)
        .max(1);
    let pixel_clock_10khz = requested_pixel_clock_10khz.min(u64::from(u16::MAX));
    if requested_pixel_clock_10khz > u64::from(u16::MAX) {
        eprintln!(
            "virtio-gpu EDID: {width}x{height}@{refresh_hz} requires pixel clock \
             {requested_pixel_clock_10khz}0 kHz; clamping to {}0 kHz",
            u16::MAX
        );
    }

    let mut dtd = [0u8; 18];
    dtd[0..2].copy_from_slice(&(pixel_clock_10khz as u16).to_le_bytes());
    dtd[2] = width as u8;
    dtd[3] = h_blank as u8;
    dtd[4] = (((width >> 8) as u8) << 4) | ((h_blank >> 8) as u8 & 0x0f);
    dtd[5] = height as u8;
    dtd[6] = v_blank as u8;
    dtd[7] = (((height >> 8) as u8) << 4) | ((v_blank >> 8) as u8 & 0x0f);
    dtd[8] = h_sync_offset as u8;
    dtd[9] = h_sync_width as u8;
    dtd[10] = ((v_sync_offset as u8) << 4) | (v_sync_width as u8 & 0x0f);
    dtd[11] = (((h_sync_offset >> 8) as u8 & 0x03) << 6)
        | (((h_sync_width >> 8) as u8 & 0x03) << 4)
        | (((v_sync_offset >> 4) as u8 & 0x03) << 2)
        | ((v_sync_width >> 4) as u8 & 0x03);
    dtd[12] = ((width * 254 / 96) / 10).min(4095) as u8;
    dtd[13] = ((height * 254 / 96) / 10).min(4095) as u8;
    dtd[14] = ((((width * 254 / 96) / 10) >> 8) as u8 & 0x0f) << 4
        | ((((height * 254 / 96) / 10) >> 8) as u8 & 0x0f);
    dtd[17] = 0x1a;
    dtd
}

pub(crate) fn monitor_descriptor(tag: u8, payload: &[u8]) -> [u8; 18] {
    let mut desc = [0u8; 18];
    desc[3] = tag;
    let n = payload.len().min(13);
    desc[5..5 + n].copy_from_slice(&payload[..n]);
    desc
}

pub(crate) fn monitor_descriptor_text(tag: u8, text: &[u8]) -> [u8; 18] {
    let mut payload = [b' '; 13];
    let n = text.len().min(12);
    payload[..n].copy_from_slice(&text[..n]);
    payload[n] = b'\n';
    monitor_descriptor(tag, &payload)
}

pub(crate) fn encode_manufacturer(value: &str) -> [u8; 2] {
    let mut code = 0u16;
    for byte in value.bytes().take(3) {
        let letter = u16::from(byte.to_ascii_uppercase().saturating_sub(b'@') & 0x1f);
        code = (code << 5) | letter;
    }
    code.to_be_bytes()
}

pub(crate) fn push_rect(out: &mut Vec<u8>, rect: Rect) {
    out.extend_from_slice(&rect.x.to_le_bytes());
    out.extend_from_slice(&rect.y.to_le_bytes());
    out.extend_from_slice(&rect.width.to_le_bytes());
    out.extend_from_slice(&rect.height.to_le_bytes());
}

pub(crate) fn read_rect(bytes: &[u8], offset: usize) -> Option<Rect> {
    Some(Rect {
        x: read_le_u32(bytes, offset)?,
        y: read_le_u32(bytes, offset + 4)?,
        width: read_le_u32(bytes, offset + 8)?,
        height: read_le_u32(bytes, offset + 12)?,
    })
}

pub(crate) fn format_supported(format: u32) -> bool {
    matches!(
        format,
        FORMAT_B8G8R8A8_UNORM
            | FORMAT_B8G8R8X8_UNORM
            | FORMAT_X8R8G8B8_UNORM
            | FORMAT_R8G8B8X8_UNORM
    )
}

pub(crate) fn parse_resolution_env() -> (u32, u32) {
    let value = std::env::var("BRIDGEVM_VIRTIO_GPU_RES").unwrap_or_else(|_| "1280x800".into());
    parse_resolution(&value).unwrap_or_else(|| {
        panic!("BRIDGEVM_VIRTIO_GPU_RES must be WIDTHxHEIGHT, for example 1600x900")
    })
}

pub(crate) fn parse_resolution(value: &str) -> Option<(u32, u32)> {
    let (width, height) = value.trim().split_once('x')?;
    let width = width.parse::<u32>().ok()?;
    let height = height.parse::<u32>().ok()?;
    (width > 0 && height > 0).then_some((width, height))
}

pub(crate) fn scanout_len(width: u32, height: u32) -> usize {
    u64::from(width)
        .checked_mul(u64::from(height))
        .and_then(|pixels| pixels.checked_mul(4))
        .and_then(|bytes| usize::try_from(bytes).ok())
        .expect("virtio-gpu scanout size overflow")
}

pub(crate) fn set_low(current: u64, value: u64) -> u64 {
    (current & !0xffff_ffff) | (value & 0xffff_ffff)
}

pub(crate) fn set_high(current: u64, value: u64) -> u64 {
    (current & 0xffff_ffff) | ((value & 0xffff_ffff) << 32)
}

pub(crate) fn is_supported_common_access_size(size: u8) -> bool {
    matches!(size, 1 | 2 | 4 | 8)
}

pub(crate) fn common_access_touches(base: u64, width: u8, offset: u64, size: u8) -> bool {
    let access_end = offset.saturating_add(u64::from(size));
    let field_end = base + u64::from(width);
    offset < field_end && base < access_end
}

pub(crate) fn common_access_touches_queue_field(offset: u64, size: u8) -> bool {
    [
        (COMMON_QUEUE_SIZE, 2),
        (COMMON_QUEUE_MSIX_VECTOR, 2),
        (COMMON_QUEUE_ENABLE, 2),
        (COMMON_QUEUE_DESC, 8),
        (COMMON_QUEUE_DRIVER, 8),
        (COMMON_QUEUE_DEVICE, 8),
    ]
    .iter()
    .any(|(base, width)| common_access_touches(*base, *width, offset, size))
}

pub(crate) fn read_common_register(
    base: u64,
    width: u8,
    value: u64,
    offset: u64,
    size: u8,
) -> Option<u64> {
    if !common_access_touches(base, width, offset, size) {
        return None;
    }
    let mut out = 0u64;
    for access_byte in 0..size {
        let byte_offset = offset + u64::from(access_byte);
        if byte_offset < base || byte_offset >= base + u64::from(width) {
            continue;
        }
        let field_byte = byte_offset - base;
        let byte = (value >> (field_byte * 8)) & 0xff;
        out |= byte << (u64::from(access_byte) * 8);
    }
    Some(mask_to_size(out, size))
}

pub(crate) fn write_common_register(
    current: u64,
    base: u64,
    width: u8,
    offset: u64,
    size: u8,
    value: u64,
) -> u64 {
    let mut out = current;
    for access_byte in 0..size {
        let byte_offset = offset + u64::from(access_byte);
        if byte_offset < base || byte_offset >= base + u64::from(width) {
            continue;
        }
        let field_byte = byte_offset - base;
        let shift = field_byte * 8;
        let byte = (value >> (u64::from(access_byte) * 8)) & 0xff;
        out = (out & !(0xff << shift)) | (byte << shift);
    }
    let bits = u64::from(width) * 8;
    if bits == 64 {
        out
    } else {
        out & ((1u64 << bits) - 1)
    }
}

pub(crate) fn mask_to_size(value: u64, size: u8) -> u64 {
    match size {
        1 => value & 0xff,
        2 => value & 0xffff,
        4 => value & 0xffff_ffff,
        _ => value,
    }
}

pub(crate) fn valid_msix_vector(vector: u16) -> u16 {
    if vector < VIRTIO_GPU_MSIX_VECTOR_COUNT || vector == VIRTIO_MSI_NO_VECTOR {
        vector
    } else {
        VIRTIO_MSI_NO_VECTOR
    }
}

pub(crate) fn read_le_from_bytes(bytes: &[u8], offset: u64, size: u8) -> Option<u64> {
    let offset = usize::try_from(offset).ok()?;
    let size = usize::from(size);
    if offset.checked_add(size)? > bytes.len() || size > 8 {
        return None;
    }
    let mut buf = [0u8; 8];
    buf[..size].copy_from_slice(&bytes[offset..offset + size]);
    Some(u64::from_le_bytes(buf))
}

pub(crate) fn read_u16(mem: &dyn GuestMemoryMut, gpa: u64) -> Option<u16> {
    let mut bytes = [0u8; 2];
    mem.read_into(gpa, &mut bytes)
        .then(|| u16::from_le_bytes(bytes))
}

pub(crate) fn read_le_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

pub(crate) fn read_le_u64(bytes: &[u8], offset: usize) -> Option<u64> {
    Some(u64::from_le_bytes(
        bytes.get(offset..offset + 8)?.try_into().ok()?,
    ))
}

pub(crate) struct FbSink {
    pub(crate) path: PathBuf,
    pub(crate) file: Option<File>,
    pub(crate) map: *mut u8,
    pub(crate) map_len: usize,
    pub(crate) capacity: usize,
    pub(crate) seq: u64,
}

// The device owns FbSink single-threadedly on the vCPU thread. The raw mmap
// pointer is never shared across threads; this only satisfies VirtioGpu's Send bound.
unsafe impl Send for FbSink {}

impl std::fmt::Debug for FbSink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FbSink")
            .field("path", &self.path)
            .field("capacity", &self.capacity)
            .field("seq", &self.seq)
            .finish()
    }
}

impl FbSink {
    pub(crate) fn from_env() -> Option<FbSink> {
        let path = std::env::var_os("BRIDGEVM_DISPLAY_EXPORT_FB")?;
        if path.is_empty() {
            return None;
        }

        Some(FbSink {
            path: PathBuf::from(path),
            file: None,
            map: std::ptr::null_mut(),
            map_len: 0,
            capacity: 0,
            seq: 0,
        })
    }

    pub(crate) fn write(
        &mut self,
        width: u32,
        height: u32,
        stride: u32,
        fourcc: u32,
        bytes: &[u8],
    ) {
        let needed = 64 + (height as usize) * (stride as usize);

        if self.map.is_null() || self.capacity < needed {
            if !self.map.is_null() {
                unsafe {
                    libc::munmap(self.map.cast(), self.map_len);
                }
            }
            self.map = std::ptr::null_mut();
            self.map_len = 0;
            self.capacity = 0;
            self.file = None;

            if let Some(parent) = self.path.parent() {
                if !parent.as_os_str().is_empty() {
                    if let Err(err) = std::fs::create_dir_all(parent) {
                        eprintln!("virtio-gpu fb export failed: {err}");
                        return;
                    }
                }
            }

            let file = match OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(true)
                .open(&self.path)
            {
                Ok(file) => file,
                Err(err) => {
                    eprintln!("virtio-gpu fb export failed: {err}");
                    return;
                }
            };

            if let Err(err) = file.set_len(needed as u64) {
                eprintln!("virtio-gpu fb export failed: {err}");
                return;
            }

            let map = unsafe {
                libc::mmap(
                    std::ptr::null_mut(),
                    needed,
                    libc::PROT_READ | libc::PROT_WRITE,
                    libc::MAP_SHARED,
                    file.as_raw_fd(),
                    0,
                )
            };
            if map == libc::MAP_FAILED {
                eprintln!(
                    "virtio-gpu fb export failed: {}",
                    std::io::Error::last_os_error()
                );
                self.map = std::ptr::null_mut();
                self.map_len = 0;
                self.capacity = 0;
                self.file = None;
                return;
            }

            self.file = Some(file);
            self.map = map.cast();
            self.map_len = needed;
            self.capacity = needed;
        }

        self.seq = self.seq.wrapping_add(1);

        let mut header = [0u8; 24];
        header[0..4].copy_from_slice(&0x4256_4642u32.to_le_bytes());
        header[4..8].copy_from_slice(&1u32.to_le_bytes());
        header[8..12].copy_from_slice(&width.to_le_bytes());
        header[12..16].copy_from_slice(&height.to_le_bytes());
        header[16..20].copy_from_slice(&stride.to_le_bytes());
        header[20..24].copy_from_slice(&fourcc.to_le_bytes());

        unsafe {
            std::ptr::copy_nonoverlapping(header.as_ptr(), self.map, header.len());
            (&*(self.map.add(24) as *const std::sync::atomic::AtomicU64))
                .store(self.seq, Ordering::Release);
        }
        std::sync::atomic::fence(Ordering::Release);

        unsafe {
            std::ptr::copy_nonoverlapping(
                bytes.as_ptr(),
                self.map.add(64),
                bytes.len().min(needed - 64),
            );
        }

        std::sync::atomic::fence(Ordering::Release);
        self.seq = self.seq.wrapping_add(1);
        unsafe {
            (&*(self.map.add(24) as *const std::sync::atomic::AtomicU64))
                .store(self.seq, Ordering::Release);
        }
    }
}
