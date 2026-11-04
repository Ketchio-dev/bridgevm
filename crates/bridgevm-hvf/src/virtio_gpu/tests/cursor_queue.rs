//! The cursor queue end to end: viogpudo's sequence for a new pointer shape
//! reaches display.cursor, and the guest sees the same completions as before.

use super::super::cursor_file::tests::{read_cursor_file, temp_cursor_path};
use super::super::*;
use super::helpers::*;

const CURSOR_DESC: u64 = 0x4001_1000;
const CURSOR_AVAIL: u64 = 0x4001_2000;
const CURSOR_USED: u64 = 0x4001_3000;
const CURSOR_REQ: u64 = 0x4001_4000;

fn cursor_request(typ: u32, resource_id: u32, x: u32, y: u32, hot: (u32, u32)) -> Vec<u8> {
    let mut out = ctrl_req(typ);
    for word in [0, x, y, 0, resource_id, hot.0, hot.1, 0] {
        out.extend_from_slice(&word.to_le_bytes());
    }
    out
}

/// Queue `request` on the cursor queue as viogpudo does (one readable
/// descriptor, no response buffer), notify, and return the used length.
fn submit_cursor(dev: &mut VirtioPciGpu, mem: &mut TestMem, request: &[u8]) -> u32 {
    let index = dev.stats().queues[QUEUE_CURSOR].last_avail_idx;
    let slot = u64::from(index % 16);
    let req = CURSOR_REQ + slot * 64;
    mem.write(req, request);
    write_desc(
        mem,
        CURSOR_DESC,
        slot as u16,
        req,
        request.len() as u32,
        0,
        0,
    );
    mem.write(CURSOR_AVAIL + 4 + slot * 2, &(slot as u16).to_le_bytes());
    mem.write(CURSOR_AVAIL + 2, &index.wrapping_add(1).to_le_bytes());
    pci_write(dev, PCI_NOTIFY_CFG_OFFSET, 4, QUEUE_CURSOR as u64, mem);
    assert_eq!(
        u16::from_le_bytes(mem.read(CURSOR_USED + 2, 2).try_into().unwrap()),
        index.wrapping_add(1)
    );
    u32::from_le_bytes(
        mem.read(CURSOR_USED + 4 + slot * 8 + 4, 4)
            .try_into()
            .unwrap(),
    )
}

#[test]
fn a_viogpudo_pointer_shape_and_move_reach_display_cursor() {
    let path = temp_cursor_path("queue");
    let mut dev = VirtioPciGpu::new(1280, 800);
    dev.gpu.cursor = GuestCursor::at_path(&path);
    let mut mem = TestMem::new(0x4000_0000, 0x30000);
    let backing = 0x4000_8000u64;

    // CreateCursor: a 64x64 B8G8R8A8 resource with its backing.
    let mut create = ctrl_req(VIRTIO_GPU_CMD_RESOURCE_CREATE_2D);
    for word in [9, FORMAT_B8G8R8A8_UNORM, 64, 64] {
        create.extend_from_slice(&word.to_le_bytes());
    }
    submit_control(&mut dev, &mut mem, &create, 24);
    let mut attach = ctrl_req(VIRTIO_GPU_CMD_RESOURCE_ATTACH_BACKING);
    attach.extend_from_slice(&9u32.to_le_bytes());
    attach.extend_from_slice(&1u32.to_le_bytes());
    attach.extend_from_slice(&backing.to_le_bytes());
    attach.extend_from_slice(&(64u32 * 64 * 4).to_le_bytes());
    attach.extend_from_slice(&0u32.to_le_bytes());
    submit_control(&mut dev, &mut mem, &attach, 24);

    // UpdateCursor: a 32x32 shape in the top left, transferred as 32x32.
    let mut guest = vec![0u8; 64 * 64 * 4];
    for row in 0..32 {
        guest[row * 256..row * 256 + 128].fill(0x80 + row as u8);
    }
    mem.write(backing, &guest);
    let mut transfer = ctrl_req(VIRTIO_GPU_CMD_TRANSFER_TO_HOST_2D);
    push_rect(
        &mut transfer,
        Rect {
            x: 0,
            y: 0,
            width: 32,
            height: 32,
        },
    );
    transfer.extend_from_slice(&0u64.to_le_bytes());
    transfer.extend_from_slice(&9u32.to_le_bytes());
    transfer.extend_from_slice(&0u32.to_le_bytes());
    submit_control(&mut dev, &mut mem, &transfer, 24);

    setup_queue(
        &mut dev,
        &mut mem,
        QUEUE_CURSOR as u16,
        CURSOR_DESC,
        CURSOR_AVAIL,
        CURSOR_USED,
        1,
    );
    let shape = cursor_request(VIRTIO_GPU_CMD_UPDATE_CURSOR, 9, 0, 0, (4, 6));
    assert_eq!(
        submit_cursor(&mut dev, &mut mem, &shape),
        0,
        "no response buffer, as before"
    );
    let (_, state, _, image) = read_cursor_file(&path);
    assert_eq!(image, guest);
    assert_eq!(
        (state.visible, state.serial, state.hot_x, state.hot_y),
        (true, 1, 4, 6)
    );

    let mov = cursor_request(VIRTIO_GPU_CMD_MOVE_CURSOR, 9, 640, 400, (0, 0));
    assert_eq!(submit_cursor(&mut dev, &mut mem, &mov), 0);
    let (_, state, _, _) = read_cursor_file(&path);
    assert_eq!((state.x, state.y, state.visible), (640, 400, true));
    let stats = dev.stats();
    assert_eq!(
        (stats.cursor_updates, stats.cursor_moves, stats.cursor_hides),
        (1, 1, 0)
    );

    // The driver goes away: the device reset hides the sprite.
    pci_write(&mut dev, COMMON_DEVICE_STATUS, 1, 0, &mut mem);
    assert!(!read_cursor_file(&path).1.visible);
    std::fs::remove_file(path).unwrap();
}
