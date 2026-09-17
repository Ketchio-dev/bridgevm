use super::*;

fn reference(
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
            scanout[dst..dst + 4].copy_from_slice(&to_xrgb8888(
                &resource.host_pixels[src..src + 4],
                resource.format,
            ));
        }
    }
}

fn resource(format: u32, width: u32, height: u32) -> GpuResource {
    let len = width as usize * height as usize * 4;
    GpuResource {
        format,
        width,
        height,
        host_pixels: (0..len)
            .map(|index| (index.wrapping_mul(37) & 0xff) as u8)
            .collect(),
        backing: Vec::new(),
    }
}

#[test]
fn every_supported_format_matches_the_pixel_reference_for_a_clipped_rectangle() {
    for format in [
        FORMAT_B8G8R8A8_UNORM,
        FORMAT_B8G8R8X8_UNORM,
        FORMAT_X8R8G8B8_UNORM,
        FORMAT_R8G8B8X8_UNORM,
    ] {
        let resource = resource(format, 7, 5);
        let rect = Rect {
            x: 2,
            y: 1,
            width: 8,
            height: 8,
        };
        let mut expected = vec![0xa5; 9 * 6 * 4];
        let mut actual = expected.clone();

        reference(&mut expected, 9, 6, &resource, rect);
        composite_resource_to_scanout(&mut actual, 9, 6, &resource, rect);

        assert_eq!(actual, expected, "format {format}");
    }
}

#[test]
fn bgr_full_frame_preserves_color_and_zeroes_the_fourth_byte() {
    let resource = GpuResource {
        format: FORMAT_B8G8R8A8_UNORM,
        width: 2,
        height: 1,
        host_pixels: vec![1, 2, 3, 4, 5, 6, 7, 8],
        backing: Vec::new(),
    };
    let mut scanout = vec![0xff; 8];

    composite_resource_to_scanout(
        &mut scanout,
        2,
        1,
        &resource,
        Rect {
            x: 0,
            y: 0,
            width: 2,
            height: 1,
        },
    );

    assert_eq!(scanout, [1, 2, 3, 0, 5, 6, 7, 0]);
}

#[test]
fn empty_and_fully_clipped_rectangles_leave_scanout_unchanged() {
    let resource = resource(FORMAT_B8G8R8X8_UNORM, 3, 2);
    for rect in [
        Rect {
            x: 1,
            y: 1,
            width: 0,
            height: 1,
        },
        Rect {
            x: 4,
            y: 0,
            width: 2,
            height: 1,
        },
    ] {
        let mut scanout = vec![0xa5; 3 * 2 * 4];
        composite_resource_to_scanout(&mut scanout, 3, 2, &resource, rect);
        assert_eq!(scanout, vec![0xa5; 3 * 2 * 4]);
    }
}
