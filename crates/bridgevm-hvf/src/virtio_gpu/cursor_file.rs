//! `display.cursor`: the guest's hardware cursor for the app, a fixed-size
//! file next to the BVFB display file (`display.fb` -> `display.cursor`).
//! BVFB itself is unchanged.
//!
//! Little-endian, laid out like BVFB (the app's reader is
//! `apps/macos/Sources/BridgeVMControl/HvfEngine/HvfGuestCursorFile.swift`):
//! - 0 magic `0x42564352` ("BVCR"), 4 version 1, 8 width 64, 12 height 64,
//!   16 stride 256, 20 fourcc AR24 (BGRA bytes, straight alpha);
//! - 24 u64 seqlock: odd while a state is being written, +2 per publish;
//! - 32 u32 shape serial: +1 per accepted image and continued by every writer
//!   of the file, so a reader reloads the image whenever it changes;
//! - 36 u32 flags: bit 0 the guest shows its sprite, bit 1 this writer (this
//!   boot's device) has accepted an image;
//! - 40/44 i32 x/y: the hot spot's position on scanout 0, in guest pixels;
//! - 48 u64 publish time, `CLOCK_UPTIME_RAW` ns (the clock of BVFB byte 48);
//! - 56/60 u32 hot spot inside the image; 64.. the 64x64 image.
//!
//! One writer, the device, under the platform lock. The file is created at
//! its fixed size and never truncated, so a reader's mapping stays valid.

use super::fb_sink_header::uptime_ns;
use super::CursorState;
use std::fs::OpenOptions;
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::io::AsRawFd;
use std::path::Path;
use std::sync::atomic::{fence, AtomicU64, Ordering};

pub(crate) const CURSOR_FILE_MAGIC: u32 = 0x4256_4352;
pub(crate) const CURSOR_SIZE: u32 = 64;
pub(crate) const CURSOR_IMAGE_BYTES: usize = 64 * 64 * 4;
pub(crate) const CURSOR_HEADER_LEN: usize = 64;
pub(crate) const CURSOR_FILE_LEN: usize = CURSOR_HEADER_LEN + CURSOR_IMAGE_BYTES;
/// DRM_FORMAT_ARGB8888 ('AR24'): B, G, R, A bytes.
pub(crate) const CURSOR_FOURCC: u32 = 0x3432_5241;
pub(crate) const FLAG_VISIBLE: u32 = 1;
pub(crate) const FLAG_SHAPE_THIS_BOOT: u32 = 2;

#[derive(Debug)]
pub(crate) struct CursorFile {
    map: *mut u8,
    seq: u64,
}

// The device owns the file and writes it only under the platform lock; the
// raw mapping pointer never leaves it.
unsafe impl Send for CursorFile {}

impl Drop for CursorFile {
    fn drop(&mut self) {
        unsafe { libc::munmap(self.map.cast(), CURSOR_FILE_LEN) };
    }
}

impl CursorFile {
    /// Opens or creates `path` at its fixed size. A file an earlier writer
    /// left keeps counting: its sequence and shape serial (returned) go on,
    /// so a reader never mistakes this writer's first state or image for one
    /// it has already seen.
    pub(crate) fn open(path: &Path) -> std::io::Result<(CursorFile, u32)> {
        if let Some(parent) = path.parent().filter(|path| !path.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)?;
        }
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .custom_flags(libc::O_NOFOLLOW)
            .open(path)?;
        if file.metadata()?.len() < CURSOR_FILE_LEN as u64 {
            file.set_len(CURSOR_FILE_LEN as u64)?;
        }
        let map = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                CURSOR_FILE_LEN,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                file.as_raw_fd(),
                0,
            )
        };
        if map == libc::MAP_FAILED {
            return Err(std::io::Error::last_os_error());
        }
        let mut cursor = CursorFile {
            map: map.cast(),
            seq: 0,
        };
        let mut serial = 0;
        if cursor.read_u32(0) == CURSOR_FILE_MAGIC {
            cursor.seq = cursor.sequence().load(Ordering::Acquire) & !1;
            serial = cursor.read_u32(32);
        }
        Ok((cursor, serial))
    }

    /// Publish `state`, and `image` (64x64 BGRA) when the shape changed, under the seqlock.
    pub(crate) fn publish(&mut self, state: &CursorState, image: Option<&[u8]>) {
        self.seq = self.seq.wrapping_add(1);
        self.sequence().store(self.seq, Ordering::Release);
        fence(Ordering::Release);
        let flags = if state.visible { FLAG_VISIBLE } else { 0 }
            | if state.shape_this_boot {
                FLAG_SHAPE_THIS_BOOT
            } else {
                0
            };
        for (offset, value) in [
            (0, CURSOR_FILE_MAGIC),
            (4, 1),
            (8, CURSOR_SIZE),
            (12, CURSOR_SIZE),
            (16, CURSOR_SIZE * 4),
            (20, CURSOR_FOURCC),
            (32, state.serial),
            (36, flags),
            (40, state.x as u32),
            (44, state.y as u32),
            (56, state.hot_x),
            (60, state.hot_y),
        ] {
            self.write_u32(offset, value);
        }
        if let Some(image) = image {
            let len = image.len().min(CURSOR_IMAGE_BYTES);
            unsafe {
                std::ptr::copy_nonoverlapping(image.as_ptr(), self.map.add(CURSOR_HEADER_LEN), len)
            };
        }
        // SAFETY: the mapping is page-aligned, so byte 48 is 8-byte aligned.
        let published = unsafe { &*(self.map.add(48) as *const AtomicU64) };
        published.store(uptime_ns().to_le(), Ordering::Relaxed);
        fence(Ordering::Release);
        self.seq = self.seq.wrapping_add(1);
        self.sequence().store(self.seq, Ordering::Release);
    }

    fn sequence(&self) -> &AtomicU64 {
        // SAFETY: the mapping is page-aligned and CURSOR_FILE_LEN long.
        unsafe { &*(self.map.add(24) as *const AtomicU64) }
    }

    fn read_u32(&self, offset: usize) -> u32 {
        u32::from_le(unsafe { std::ptr::read_unaligned(self.map.add(offset).cast::<u32>()) })
    }

    fn write_u32(&mut self, offset: usize, value: u32) {
        unsafe { std::ptr::write_unaligned(self.map.add(offset).cast::<u32>(), value.to_le()) };
    }
}

#[cfg(test)]
#[path = "cursor_file_tests.rs"]
pub(crate) mod tests;
