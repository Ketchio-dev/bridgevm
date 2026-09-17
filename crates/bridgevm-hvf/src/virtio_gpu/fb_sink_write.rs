//! Framebuffer mmap allocation and full or damaged-row publication.

use super::{FbSink, Rect};
use std::fs::OpenOptions;
use std::os::unix::io::AsRawFd;
use std::sync::atomic::Ordering;

const HEADER_LEN: usize = 64;
const PIXEL_BYTES: usize = 4;

impl FbSink {
    pub(crate) fn write(
        &mut self,
        width: u32,
        height: u32,
        stride: u32,
        fourcc: u32,
        bytes: &[u8],
    ) {
        self.write_inner(width, height, stride, fourcc, bytes, None);
    }

    pub(crate) fn write_damage(
        &mut self,
        width: u32,
        height: u32,
        stride: u32,
        fourcc: u32,
        bytes: &[u8],
        damage: Rect,
    ) {
        self.write_inner(width, height, stride, fourcc, bytes, Some(damage));
    }

    fn write_inner(
        &mut self,
        width: u32,
        height: u32,
        stride: u32,
        fourcc: u32,
        bytes: &[u8],
        damage: Option<Rect>,
    ) {
        let Some(frame_len) = (height as usize).checked_mul(stride as usize) else {
            return;
        };
        let Some(needed) = HEADER_LEN.checked_add(frame_len) else {
            return;
        };
        if bytes.len() < frame_len {
            return;
        }
        let Some(new_mapping) = self.ensure_mapping(needed) else {
            return;
        };
        let same_layout = !new_mapping && self.layout_matches(width, height, stride, fourcc);
        self.begin_frame(width, height, stride, fourcc);
        if same_layout {
            if let Some(rect) = damage {
                self.copy_damage(bytes, width, height, stride, rect);
            } else {
                self.copy_full(bytes, frame_len);
            }
        } else {
            self.copy_full(bytes, frame_len);
        }
        self.finish_frame();
    }

    fn ensure_mapping(&mut self, needed: usize) -> Option<bool> {
        if !self.map.is_null() && self.capacity >= needed {
            return Some(false);
        }
        self.reset_mapping();
        if let Some(parent) = self
            .path
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
        {
            if let Err(err) = std::fs::create_dir_all(parent) {
                eprintln!("virtio-gpu fb export failed: {err}");
                return None;
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
                return None;
            }
        };
        if let Err(err) = file.set_len(needed as u64) {
            eprintln!("virtio-gpu fb export failed: {err}");
            return None;
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
            return None;
        }
        self.file = Some(file);
        self.map = map.cast();
        self.map_len = needed;
        self.capacity = needed;
        Some(true)
    }

    fn reset_mapping(&mut self) {
        if !self.map.is_null() {
            unsafe { libc::munmap(self.map.cast(), self.map_len) };
        }
        self.map = std::ptr::null_mut();
        self.map_len = 0;
        self.capacity = 0;
        self.file = None;
    }

    fn layout_matches(&self, width: u32, height: u32, stride: u32, fourcc: u32) -> bool {
        unsafe {
            read_u32(self.map, 0) == 0x4256_4642
                && read_u32(self.map, 4) == 1
                && read_u32(self.map, 8) == width
                && read_u32(self.map, 12) == height
                && read_u32(self.map, 16) == stride
                && read_u32(self.map, 20) == fourcc
        }
    }

    fn begin_frame(&mut self, width: u32, height: u32, stride: u32, fourcc: u32) {
        self.seq = self.seq.wrapping_add(1);
        self.sequence().store(self.seq, Ordering::Release);
        std::sync::atomic::fence(Ordering::Release);
        let mut header = [0u8; 24];
        for (slot, value) in
            header
                .chunks_exact_mut(4)
                .zip([0x4256_4642u32, 1, width, height, stride, fourcc])
        {
            slot.copy_from_slice(&value.to_le_bytes());
        }
        unsafe {
            std::ptr::copy_nonoverlapping(header.as_ptr(), self.map, header.len());
        }
    }

    fn copy_full(&mut self, bytes: &[u8], frame_len: usize) {
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), self.map.add(HEADER_LEN), frame_len)
        };
    }

    fn copy_damage(&mut self, bytes: &[u8], width: u32, height: u32, stride: u32, rect: Rect) {
        let x0 = rect.x.min(width) as usize;
        let x1 = rect.x.saturating_add(rect.width).min(width) as usize;
        let y0 = rect.y.min(height) as usize;
        let y1 = rect.y.saturating_add(rect.height).min(height) as usize;
        if x1 <= x0 || y1 <= y0 {
            return;
        }
        let row_len = (x1 - x0) * PIXEL_BYTES;
        for y in y0..y1 {
            let offset = y * stride as usize + x0 * PIXEL_BYTES;
            unsafe {
                std::ptr::copy_nonoverlapping(
                    bytes.as_ptr().add(offset),
                    self.map.add(HEADER_LEN + offset),
                    row_len,
                )
            };
        }
    }

    fn finish_frame(&mut self) {
        std::sync::atomic::fence(Ordering::Release);
        self.seq = self.seq.wrapping_add(1);
        self.sequence().store(self.seq, Ordering::Release);
    }

    fn sequence(&self) -> &std::sync::atomic::AtomicU64 {
        unsafe { &*(self.map.add(24) as *const std::sync::atomic::AtomicU64) }
    }
}

unsafe fn read_u32(base: *mut u8, offset: usize) -> u32 {
    unsafe { u32::from_le(std::ptr::read_unaligned(base.add(offset).cast::<u32>())) }
}

#[cfg(test)]
#[path = "fb_sink_write_tests.rs"]
mod tests;
