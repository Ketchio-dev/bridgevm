use super::super::cursor_file::tests::{read_cursor_file, temp_cursor_path};
use super::super::{FORMAT_B8G8R8X8_UNORM, VIRTIO_GPU_CMD_UPDATE_CURSOR};
use super::*;

const CURSOR: u32 = 1;

fn resource(width: u32, height: u32, format: u32, fill: u8) -> GpuResource {
    GpuResource {
        format,
        width,
        height,
        host_pixels: vec![fill; (width * height * 4) as usize],
        backing: Vec::new(),
    }
}

fn resources() -> BTreeMap<u32, GpuResource> {
    let mut map = BTreeMap::new();
    let mut image = resource(64, 64, FORMAT_B8G8R8A8_UNORM, 0);
    for (index, byte) in image.host_pixels.iter_mut().enumerate() {
        *byte = (index % 251) as u8;
    }
    map.insert(CURSOR, image);
    map
}

/// struct virtio_gpu_update_cursor for scanout 0.
fn request(typ: u32, resource_id: u32, x: u32, y: u32, hot: (u32, u32)) -> Vec<u8> {
    let mut out = Vec::new();
    for word in [typ, 0, 0, 0, 0, 0, 0, x, y, 0, resource_id, hot.0, hot.1, 0] {
        out.extend_from_slice(&word.to_le_bytes());
    }
    out
}

fn update(resource_id: u32, x: u32, y: u32, hot: (u32, u32)) -> Vec<u8> {
    request(VIRTIO_GPU_CMD_UPDATE_CURSOR, resource_id, x, y, hot)
}

fn moved(resource_id: u32, x: u32, y: u32) -> Vec<u8> {
    request(VIRTIO_GPU_CMD_MOVE_CURSOR, resource_id, x, y, (0, 0))
}

fn cursor(label: &str) -> (GuestCursor, PathBuf) {
    let path = temp_cursor_path(label);
    (GuestCursor::at_path(&path), path)
}

fn send(cursor: &mut GuestCursor, request: &[u8], resources: &BTreeMap<u32, GpuResource>) {
    let typ = read_le_u32(request, 0).unwrap();
    cursor.command(typ, request, resources);
}

#[test]
fn update_publishes_the_resource_image_and_hot_spot_and_move_follows() {
    let (mut cursor, path) = cursor("update");
    let resources = resources();
    let (_, start, _, _) = read_cursor_file(&path);
    assert!(!start.visible && !start.shape_this_boot);

    // viogpudo's SetPointerShape: UPDATE_CURSOR at (0,0) with the hot spot.
    send(&mut cursor, &update(CURSOR, 0, 0, (5, 7)), &resources);
    let (_, state, _, image) = read_cursor_file(&path);
    assert_eq!(image, resources[&CURSOR].host_pixels);
    assert_eq!(
        (state.serial, state.visible, state.shape_this_boot),
        (1, true, true)
    );
    assert_eq!((state.hot_x, state.hot_y), (5, 7));

    send(&mut cursor, &moved(CURSOR, 100, 200), &resources);
    let (_, state, _, _) = read_cursor_file(&path);
    assert_eq!(
        (state.x, state.y, state.visible, state.serial),
        (100, 200, true, 1)
    );

    // The next shape arrives at (0,0) again, possibly after a newer move:
    // the position stays.
    send(&mut cursor, &update(CURSOR, 0, 0, (1, 2)), &resources);
    let (_, state, _, _) = read_cursor_file(&path);
    assert_eq!((state.x, state.y, state.serial), (100, 200, 2));
    assert_eq!((cursor.updates, cursor.moves, cursor.hides), (2, 1, 0));
    std::fs::remove_file(path).unwrap();
}

#[test]
fn a_move_to_the_origin_hides_the_sprite_as_viogpudo_parks_it_there() {
    let (mut cursor, path) = cursor("park");
    let resources = resources();
    send(&mut cursor, &update(CURSOR, 0, 0, (0, 0)), &resources);
    send(&mut cursor, &moved(CURSOR, 100, 200), &resources);
    send(&mut cursor, &moved(CURSOR, 0, 0), &resources);
    let (_, state, _, _) = read_cursor_file(&path);
    assert!(!state.visible, "viogpudo 0.1-299 hides by parking at (0,0)");
    assert_eq!((state.x, state.y), (100, 200), "the park is not a position");
    assert_eq!(cursor.hides, 1);
    send(&mut cursor, &moved(CURSOR, 1, 0), &resources);
    let (_, state, _, _) = read_cursor_file(&path);
    assert!(state.visible);
    assert_eq!((state.x, state.y), (1, 0));
    std::fs::remove_file(path).unwrap();
}

#[test]
fn resource_zero_hides_and_a_later_update_shows_the_kept_image_again() {
    let (mut cursor, path) = cursor("hide");
    let resources = resources();
    send(&mut cursor, &update(CURSOR, 0, 0, (3, 3)), &resources);
    // Upstream viogpudo's hide, then its re-attach with position and hot spot.
    send(&mut cursor, &update(0, 0, 0, (0, 0)), &resources);
    let (_, state, _, image) = read_cursor_file(&path);
    assert_eq!((state.visible, state.serial), (false, 1));
    assert_eq!(image, resources[&CURSOR].host_pixels);
    // A move naming resource 0 shows nothing either.
    send(&mut cursor, &moved(0, 9, 9), &resources);
    assert!(!read_cursor_file(&path).1.visible);
    send(&mut cursor, &update(CURSOR, 50, 60, (3, 3)), &resources);
    let (_, state, _, _) = read_cursor_file(&path);
    assert_eq!((state.visible, state.x, state.y), (true, 50, 60));
    assert_eq!(cursor.hides, 1);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn guest_sizes_and_fields_never_size_or_place_the_image() {
    let (mut cursor, path) = cursor("bounds");
    let mut resources = resources();
    resources.insert(2, resource(128, 128, FORMAT_B8G8R8A8_UNORM, 0xee));
    resources.insert(3, resource(32, 32, FORMAT_B8G8R8A8_UNORM, 0xee));
    resources.insert(4, resource(64, 64, FORMAT_B8G8R8X8_UNORM, 0xee));
    let (seq, _, _, _) = read_cursor_file(&path);

    // Too short, or for a scanout that does not exist: nothing changes.
    let mut short = update(CURSOR, 0, 0, (0, 0));
    short.truncate(55);
    send(&mut cursor, &short, &resources);
    let mut other_scanout = update(CURSOR, 0, 0, (0, 0));
    other_scanout[24..28].copy_from_slice(&1u32.to_le_bytes());
    send(&mut cursor, &other_scanout, &resources);
    assert_eq!(read_cursor_file(&path).0, seq);

    // Not a 64x64 image with alpha, or no such resource: no shape, nothing shown.
    for id in [2, 3, 4, 99] {
        send(&mut cursor, &update(id, 10, 10, (0, 0)), &resources);
        let (_, state, _, image) = read_cursor_file(&path);
        assert_eq!((state.serial, state.visible), (0, false), "resource {id}");
        assert!(image.iter().all(|byte| *byte == 0));
    }

    // A hot spot outside the image is clamped into it.
    send(
        &mut cursor,
        &update(CURSOR, 0, 0, (1000, u32::MAX)),
        &resources,
    );
    let (_, state, _, _) = read_cursor_file(&path);
    assert_eq!((state.hot_x, state.hot_y, state.serial), (63, 63, 1));

    // A bad resource later keeps the accepted image shown, as in QEMU.
    send(&mut cursor, &update(2, 0, 0, (0, 0)), &resources);
    let (_, state, _, image) = read_cursor_file(&path);
    assert_eq!((state.serial, state.visible), (1, true));
    assert_eq!(image, resources[&CURSOR].host_pixels);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn reset_hides_and_a_new_boot_starts_hidden_with_the_serial_continued() {
    let path = temp_cursor_path("boot");
    let resources = resources();
    let mut first = GuestCursor::at_path(&path);
    send(&mut first, &update(CURSOR, 0, 0, (0, 0)), &resources);
    send(&mut first, &moved(CURSOR, 40, 40), &resources);
    first.reset();
    let (_, state, _, _) = read_cursor_file(&path);
    assert_eq!((state.visible, state.shape_this_boot), (false, true));
    assert_eq!(first.hides, 1);
    drop(first);

    // The next boot's device: hidden, no shape yet, the serial goes on.
    let mut second = GuestCursor::at_path(&path);
    let (_, state, _, _) = read_cursor_file(&path);
    assert_eq!(
        (state.visible, state.shape_this_boot, state.serial),
        (false, false, 1)
    );
    send(&mut second, &moved(CURSOR, 40, 40), &resources);
    assert!(
        !read_cursor_file(&path).1.visible,
        "no image yet in this boot"
    );
    send(&mut second, &update(CURSOR, 0, 0, (0, 0)), &resources);
    assert_eq!(read_cursor_file(&path).1.serial, 2);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn without_a_display_file_the_cursor_is_only_tracked() {
    let mut cursor = GuestCursor::default();
    let resources = resources();
    send(&mut cursor, &update(CURSOR, 0, 0, (0, 0)), &resources);
    send(&mut cursor, &moved(CURSOR, 3, 4), &resources);
    assert_eq!((cursor.updates, cursor.moves), (1, 1));
    assert!(cursor.state.visible);
}
