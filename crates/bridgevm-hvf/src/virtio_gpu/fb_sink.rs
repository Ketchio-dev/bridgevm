//! Shared-memory framebuffer export sink: mmap file, header/seq protocol, teardown.

use std::fs::File;
use std::fs::OpenOptions;
use std::os::unix::io::AsRawFd;
use std::path::PathBuf;
use std::sync::atomic::Ordering;

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

impl Drop for FbSink {
    fn drop(&mut self) {
        if !self.map.is_null() {
            unsafe {
                libc::munmap(self.map.cast(), self.map_len);
            }
            self.map = std::ptr::null_mut();
            self.map_len = 0;
            self.capacity = 0;
        }
    }
}

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
