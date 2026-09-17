//! Pixel compositing for 2D resources on the General Preview display path.

use super::*;

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
    if x_end <= rect.x || y_end <= rect.y {
        return;
    }
    if matches!(
        resource.format,
        FORMAT_B8G8R8A8_UNORM | FORMAT_B8G8R8X8_UNORM
    ) {
        let row_bytes = ((x_end - rect.x) as usize) * 4;
        for y in rect.y..y_end {
            let src = ((y as usize) * (resource.width as usize) + (rect.x as usize)) * 4;
            let dst = ((y as usize) * (scanout_width as usize) + (rect.x as usize)) * 4;
            for (source, target) in resource.host_pixels[src..src + row_bytes]
                .chunks_exact(4)
                .zip(scanout[dst..dst + row_bytes].chunks_exact_mut(4))
            {
                target.copy_from_slice(&[source[0], source[1], source[2], 0]);
            }
        }
        return;
    }
    for y in rect.y..y_end {
        for x in rect.x..x_end {
            let src = ((y as usize) * (resource.width as usize) + (x as usize)) * 4;
            let dst = ((y as usize) * (scanout_width as usize) + (x as usize)) * 4;
            let pixel = &resource.host_pixels[src..src + 4];
            scanout[dst..dst + 4].copy_from_slice(&to_xrgb8888(pixel, resource.format));
        }
    }
}

#[cfg(test)]
#[path = "compositor_2d_tests.rs"]
mod tests;
