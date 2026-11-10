//! 2D copies without the platform lock (unlocked_copy.rs). A kick queues the
//! copy; the vCPU takes it, runs it unlocked and finishes it under the lock,
//! which posts the command. `Guest` drives the queues as viogpudo does and
//! finishes copies as the probe's vCPU does; `Mutex<Guest>` stands in for the
//! platform lock where another thread acts during a held copy.

use super::super::cursor_file::tests::{read_cursor_file, temp_cursor_path};
use super::super::*;
use super::helpers::*;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, TryLockError};
use std::time::{Duration, Instant};

const DESC: u64 = 0x4000_1000;
const AVAIL: u64 = 0x4000_2000;
const USED: u64 = 0x4000_3000;
const REQ: u64 = 0x4000_4000;
const RESP: u64 = 0x4000_6000;
const CURSOR_DESC: u64 = 0x4000_7000;
const CURSOR_AVAIL: u64 = 0x4000_7400;
const CURSOR_USED: u64 = 0x4000_7800;
const CURSOR_REQ: u64 = 0x4000_8000;
const BACKING: u64 = 0x4001_0000;
const CURSOR_BACKING: u64 = 0x4002_0000;
/// The scanout: small, so every rectangle below is cheap.
const W: u32 = 8;
const H: u32 = 4;

fn fb_path(label: &str) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    std::env::temp_dir().join(format!(
        "bridgevm-unlocked-copy-{label}-{}-{}.fb",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}

fn rect(x: u32, y: u32, width: u32, height: u32) -> Rect {
    Rect {
        x,
        y,
        width,
        height,
    }
}

fn request(typ: u32, words: &[u32]) -> Vec<u8> {
    let mut out = ctrl_req(typ);
    for word in words {
        out.extend_from_slice(&word.to_le_bytes());
    }
    out
}

fn transfer_req(resource_id: u32, area: Rect, offset: u64) -> Vec<u8> {
    let mut out = ctrl_req(VIRTIO_GPU_CMD_TRANSFER_TO_HOST_2D);
    push_rect(&mut out, area);
    out.extend_from_slice(&offset.to_le_bytes());
    out.extend_from_slice(&resource_id.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out
}

fn attach_req(resource_id: u32, addr: u64, len: u32) -> Vec<u8> {
    let mut out = request(VIRTIO_GPU_CMD_RESOURCE_ATTACH_BACKING, &[resource_id, 1]);
    out.extend_from_slice(&addr.to_le_bytes());
    out.extend_from_slice(&[len, 0].map(u32::to_le_bytes).concat());
    out
}

/// A guest with viogpudo's 2D scanout: resource 1, `W`x`H` in `format`,
/// backed at `BACKING`, bound to scanout 0, publishing to its own file.
struct Guest {
    dev: VirtioPciGpu,
    mem: TestMem,
    /// The control queue's next avail index.
    next: u16,
    path: PathBuf,
    /// Copies the vCPU took off the lock.
    copies: usize,
}

impl Guest {
    fn new(format: u32, unlocked: bool) -> Self {
        let path = fb_path("guest");
        let mut dev = VirtioPciGpu::new(W, H);
        dev.gpu.fb_sink = Some(FbSink::at_path(path.clone()));
        assert_eq!(dev.set_unlocked_copy(unlocked), unlocked);
        let mut guest = Self {
            dev,
            mem: TestMem::new(0x4000_0000, 0x30000),
            next: 0,
            path,
            copies: 0,
        };
        guest.setup_queues();
        for command in [
            request(VIRTIO_GPU_CMD_RESOURCE_CREATE_2D, &[1, format, W, H]),
            attach_req(1, BACKING, W * H * 4),
            request(VIRTIO_GPU_CMD_SET_SCANOUT, &[0, 0, W, H, 0, 1]),
        ] {
            guest.command(&command);
        }
        guest
    }

    fn setup_queues(&mut self) {
        setup_queue(&mut self.dev, &mut self.mem, 0, DESC, AVAIL, USED, 0);
        let cursor = QUEUE_CURSOR as u16;
        setup_queue(
            &mut self.dev,
            &mut self.mem,
            cursor,
            CURSOR_DESC,
            CURSOR_AVAIL,
            CURSOR_USED,
            1,
        );
    }

    /// The guest queues `request` on the control queue and kicks; returns
    /// the chain's head.
    fn kick(&mut self, request: &[u8]) -> u16 {
        let slot = self.next % 8;
        let (req, resp, head) = (
            REQ + u64::from(slot) * 0x100,
            RESP + u64::from(slot) * 0x40,
            slot * 2,
        );
        self.mem.write(req, request);
        self.mem.write(resp, &[0xee; 24]);
        write_desc(
            &mut self.mem,
            DESC,
            head,
            req,
            request.len() as u32,
            DESC_F_NEXT,
            head + 1,
        );
        write_desc(&mut self.mem, DESC, head + 1, resp, 24, DESC_F_WRITE, 0);
        let ring = AVAIL + 4 + u64::from(self.next % 16) * 2;
        self.mem.write(ring, &head.to_le_bytes());
        self.next = self.next.wrapping_add(1);
        self.mem.write(AVAIL + 2, &self.next.to_le_bytes());
        pci_write(&mut self.dev, PCI_NOTIFY_CFG_OFFSET, 4, 0, &mut self.mem);
        head
    }

    /// The rest of the kick's exit: each queued copy runs off the lock and
    /// is finished under it, as the probe's vCPU does.
    fn vcpu_finish(&mut self) {
        while let Some(mut copy) = self.dev.take_copy() {
            self.copies += 1;
            copy.run(&self.mem);
            self.dev.finish_copy(copy, &mut self.mem);
        }
    }

    /// One command to completion; its response status.
    fn command(&mut self, request: &[u8]) -> u32 {
        let head = self.kick(request);
        self.vcpu_finish();
        assert_eq!(self.used_idx(), self.next, "completed");
        self.status(head)
    }

    fn status(&self, head: u16) -> u32 {
        read_le_u32(&self.mem.read(RESP + u64::from(head / 2) * 0x40, 4), 0).unwrap()
    }

    fn used_idx(&self) -> u16 {
        u16::from_le_bytes(self.mem.read(USED + 2, 2).try_into().unwrap())
    }

    /// The heads in the used ring, oldest first.
    fn used_heads(&self) -> Vec<u32> {
        (0..self.used_idx())
            .map(|i| read_le_u32(&self.mem.read(USED + 4 + u64::from(i % 16) * 8, 4), 0).unwrap())
            .collect()
    }

    /// The guest draws `pixels` (a whole `W`x`H` frame) into the backing.
    fn draw(&mut self, pixels: &[u8]) {
        self.mem.write(BACKING, pixels);
    }

    /// viogpudo's present: TRANSFER_TO_HOST_2D of `area` (its offset in the
    /// whole-frame backing), then RESOURCE_FLUSH of the same rectangle.
    fn present(&mut self, area: Rect) {
        let offset = u64::from(area.y * W + area.x) * 4;
        assert_eq!(
            self.command(&transfer_req(1, area, offset)),
            VIRTIO_GPU_RESP_OK_NODATA
        );
        assert_eq!(self.command(&flush_req(1, area)), VIRTIO_GPU_RESP_OK_NODATA);
    }

    /// The display file without its publish time (bytes 48..56).
    fn display_file(&self) -> Vec<u8> {
        let mut bytes = std::fs::read(&self.path).unwrap();
        bytes[48..56].fill(0);
        bytes
    }
}

impl Drop for Guest {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// A `W`x`H` frame whose every byte differs, alpha bytes included.
fn frame(seed: u8) -> Vec<u8> {
    (0..W * H * 4)
        .map(|i| seed.wrapping_add(i as u8).wrapping_mul(7))
        .collect()
}

#[test]
fn display_fb_is_byte_identical_to_the_inline_copy() {
    for format in [FORMAT_B8G8R8A8_UNORM, FORMAT_R8G8B8X8_UNORM] {
        let mut inline = Guest::new(format, false);
        let mut unlocked = Guest::new(format, true);
        // Another writer of each file, as the ramfb exporter is.
        let mut others =
            [&inline, &unlocked].map(|guest| guest.dev.gpu.fb_sink.as_ref().unwrap().share());
        let steps: [(&str, u8, Rect); 4] = [
            ("full", 1, rect(0, 0, W, H)),
            ("partial", 2, rect(1, 1, 5, 2)),
            ("clipped", 3, rect(6, 3, 9, 9)),
            ("after another writer", 4, rect(2, 0, 3, 1)),
        ];
        for (step, seed, area) in steps {
            if step == "after another writer" {
                for other in &mut others {
                    other.write(
                        W,
                        H,
                        W * 4,
                        crate::ramfb::DRM_FORMAT_XRGB8888,
                        &[0x5a; (W * H * 4) as usize],
                    );
                }
            }
            for guest in [&mut inline, &mut unlocked] {
                guest.draw(&frame(seed));
                guest.present(area);
            }
            assert_eq!(
                unlocked.display_file(),
                inline.display_file(),
                "{format:#x} {step}"
            );
            assert_eq!(
                unlocked.dev.gpu.scanout, inline.dev.gpu.scanout,
                "{format:#x} {step}"
            );
        }
        // The damage after the other writer's frame was published whole.
        assert_eq!(unlocked.display_file()[64..], unlocked.dev.gpu.scanout);
        assert_eq!((inline.copies, unlocked.copies), (0, 8), "{format:#x}");
        assert_eq!(unlocked.used_heads(), inline.used_heads());
    }
}

/// A vCPU kicks a full-frame flush and runs its exit; meanwhile the test
/// holds the display file's lock, which the copy's publish waits for, and
/// another thread tries the platform lock. Returns whether that thread got
/// it during the copy, and the used index it saw then.
fn flush_held_by_a_vcpu(unlocked: bool) -> (Option<u16>, Guest) {
    let mut guest = Guest::new(FORMAT_B8G8R8A8_UNORM, unlocked);
    guest.draw(&frame(9));
    let area = rect(0, 0, W, H);
    assert_eq!(
        guest.command(&transfer_req(1, area, 0)),
        VIRTIO_GPU_RESP_OK_NODATA
    );
    let used_before = guest.used_idx();
    let other_writer = guest.dev.gpu.fb_sink.as_ref().unwrap().share();
    let display_file_lock = other_writer.lock_shared();
    let kicked = AtomicBool::new(false);
    let platform = Mutex::new(guest);
    let seen = std::thread::scope(|s| {
        let vcpu = s.spawn(|| {
            let copy = {
                let mut guest = platform.lock().unwrap();
                kicked.store(true, Ordering::SeqCst);
                guest.kick(&flush_req(1, area));
                guest.dev.take_copy()
            };
            if let Some(mut copy) = copy {
                copy.run(&TestMem::new(0, 0));
                let mut guest = platform.lock().unwrap();
                let guest = &mut *guest;
                guest.dev.finish_copy(copy, &mut guest.mem);
            }
        });
        while !kicked.load(Ordering::SeqCst) {
            std::thread::yield_now();
        }
        let seen = if unlocked {
            // The copy waits on the display file with the platform unlocked.
            let give_up = Instant::now() + Duration::from_secs(5);
            loop {
                match platform.try_lock() {
                    Ok(guest) if guest.dev.gpu.copy_pending() => break Some(guest.used_idx()),
                    _ if Instant::now() > give_up => panic!("the copy never ran unlocked"),
                    _ => std::thread::sleep(Duration::from_millis(1)),
                }
            }
        } else {
            // Time for the inline copy to reach the display file's lock.
            std::thread::sleep(Duration::from_millis(50));
            match platform.try_lock() {
                Ok(guest) => Some(guest.used_idx()),
                Err(TryLockError::WouldBlock) => None,
                Err(TryLockError::Poisoned(_)) => panic!("the vCPU panicked"),
            }
        };
        drop(display_file_lock);
        vcpu.join().unwrap();
        seen.map(|used| used - used_before)
    });
    (seen, platform.into_inner().unwrap())
}

#[test]
fn the_platform_lock_is_free_while_a_frame_copies() {
    let (seen, guest) = flush_held_by_a_vcpu(true);
    assert_eq!(
        seen,
        Some(0),
        "another vCPU gets the lock and sees no completion yet"
    );
    assert_eq!(
        guest.used_idx(),
        guest.next,
        "the flush completed after the copy"
    );
    let pixels = &guest.display_file()[64..];
    assert_eq!(
        pixels[..4],
        [frame(9)[0], frame(9)[1], frame(9)[2], 0],
        "published"
    );
}

#[test]
fn with_unlocked_copies_off_the_copy_holds_the_lock_as_before() {
    let (seen, guest) = flush_held_by_a_vcpu(false);
    assert_eq!(seen, None, "the copy ran under the kick's hold");
    assert_eq!(guest.used_idx(), guest.next);
}

#[test]
fn an_unref_kicked_during_a_transfer_waits_and_each_completes_once() {
    let mut guest = Guest::new(FORMAT_B8G8R8A8_UNORM, true);
    guest.draw(&frame(5));
    let before = guest.used_idx();
    let transfer = guest.kick(&transfer_req(1, rect(0, 0, W, H), 0));
    let mut copy = guest.dev.take_copy().expect("the transfer queued its copy");
    // Another vCPU unrefs the resource the copy is filling.
    let unref = guest.kick(&request(VIRTIO_GPU_CMD_RESOURCE_UNREF, &[1, 0]));
    assert_eq!(
        guest.used_idx(),
        before,
        "neither completes during the copy"
    );
    assert_eq!(
        guest.dev.stats().queues[0].last_avail_idx,
        before + 1,
        "the unref waits in the ring"
    );
    copy.run(&guest.mem);
    guest.dev.finish_copy(copy, &mut guest.mem);
    assert!(guest.dev.take_copy().is_none());
    assert_eq!(guest.used_idx(), before + 2);
    assert_eq!(
        guest.used_heads()[usize::from(before)..],
        [u32::from(transfer), u32::from(unref)]
    );
    assert_eq!(
        [transfer, unref].map(|head| guest.status(head)),
        [VIRTIO_GPU_RESP_OK_NODATA; 2]
    );
    assert!(
        !guest.dev.gpu.resources.contains_key(&1),
        "unreffed after the copy"
    );
    assert!(guest.dev.gpu.interrupt_status & 1 != 0);
}

#[test]
fn a_device_reset_during_a_flush_posts_nothing_to_the_new_queue() {
    let mut guest = Guest::new(FORMAT_B8G8R8A8_UNORM, true);
    guest.draw(&frame(6));
    guest.present(rect(0, 0, W, H));
    guest.kick(&flush_req(1, rect(0, 0, W, H)));
    let mut copy = guest.dev.take_copy().expect("the flush queued its copy");
    // Another vCPU resets the device, and the driver starts again on the
    // same rings, which it clears, before the old copy is back.
    pci_write(&mut guest.dev, COMMON_DEVICE_STATUS, 1, 0, &mut guest.mem);
    for ring in [AVAIL, USED] {
        guest.mem.write(ring, &[0; 0x100]);
    }
    guest.next = 0;
    guest.setup_queues();
    let info = guest.kick(&ctrl_req(VIRTIO_GPU_CMD_GET_DISPLAY_INFO));
    assert_eq!(guest.used_idx(), 0, "the new queue waits for the copy");
    copy.run(&guest.mem);
    guest.dev.finish_copy(copy, &mut guest.mem);
    assert_eq!(
        guest.used_heads(),
        [u32::from(info)],
        "only the new command, once"
    );
    assert!(
        guest.dev.gpu.fb_sink.is_some(),
        "the display file's writer is back"
    );
    assert!(guest.dev.gpu.resources.is_empty());
    assert_eq!(guest.dev.gpu.scanout.len(), (W * H * 4) as usize);
    // The new driver presents as before.
    for command in [
        request(
            VIRTIO_GPU_CMD_RESOURCE_CREATE_2D,
            &[1, FORMAT_B8G8R8A8_UNORM, W, H],
        ),
        attach_req(1, BACKING, W * H * 4),
        request(VIRTIO_GPU_CMD_SET_SCANOUT, &[0, 0, W, H, 0, 1]),
    ] {
        assert_eq!(guest.command(&command), VIRTIO_GPU_RESP_OK_NODATA);
    }
    guest.draw(&frame(7));
    guest.present(rect(0, 0, W, H));
    assert_eq!(
        guest.display_file()[64..68],
        [frame(7)[0], frame(7)[1], frame(7)[2], 0]
    );
}

#[test]
fn a_cursor_shape_kicked_during_its_transfer_gets_the_transferred_pixels() {
    let cursor_path = temp_cursor_path("unlocked-copy");
    let mut guest = Guest::new(FORMAT_B8G8R8A8_UNORM, true);
    guest.dev.gpu.cursor = GuestCursor::at_path(&cursor_path);
    // viogpudo's CreateCursor: a 64x64 resource with its backing.
    for command in [
        request(
            VIRTIO_GPU_CMD_RESOURCE_CREATE_2D,
            &[9, FORMAT_B8G8R8A8_UNORM, 64, 64],
        ),
        attach_req(9, CURSOR_BACKING, 64 * 64 * 4),
    ] {
        assert_eq!(guest.command(&command), VIRTIO_GPU_RESP_OK_NODATA);
    }
    let shape: Vec<u8> = (0..64 * 64 * 4).map(|i| (i % 251) as u8).collect();
    guest.mem.write(CURSOR_BACKING, &shape);
    // UpdateCursor: the transfer, then the cursor queue's UPDATE_CURSOR,
    // here from another vCPU while the transfer's copy is out.
    let before = guest.used_idx();
    guest.kick(&transfer_req(9, rect(0, 0, 64, 64), 0));
    let mut copy = guest.dev.take_copy().expect("the transfer queued its copy");
    let update = request(VIRTIO_GPU_CMD_UPDATE_CURSOR, &[0, 0, 0, 0, 9, 3, 5, 0]);
    guest.mem.write(CURSOR_REQ, &update);
    write_desc(
        &mut guest.mem,
        CURSOR_DESC,
        0,
        CURSOR_REQ,
        update.len() as u32,
        0,
        0,
    );
    guest.mem.write(CURSOR_AVAIL + 4, &0u16.to_le_bytes());
    guest.mem.write(CURSOR_AVAIL + 2, &1u16.to_le_bytes());
    pci_write(
        &mut guest.dev,
        PCI_NOTIFY_CFG_OFFSET,
        4,
        QUEUE_CURSOR as u64,
        &mut guest.mem,
    );
    let cursor_used =
        |guest: &Guest| u16::from_le_bytes(guest.mem.read(CURSOR_USED + 2, 2).try_into().unwrap());
    assert_eq!(
        cursor_used(&guest),
        0,
        "the cursor queue waits for the copy"
    );
    assert!(!read_cursor_file(&cursor_path).1.visible);
    copy.run(&guest.mem);
    guest.dev.finish_copy(copy, &mut guest.mem);
    assert_eq!(
        (guest.used_idx(), cursor_used(&guest)),
        (before + 1, 1),
        "each once"
    );
    let (_, state, _, image) = read_cursor_file(&cursor_path);
    assert_eq!(
        (state.visible, state.serial, state.hot_x, state.hot_y),
        (true, 1, 3, 5)
    );
    assert!(image == shape, "the shape is the transferred one");
    std::fs::remove_file(cursor_path).unwrap();
}
