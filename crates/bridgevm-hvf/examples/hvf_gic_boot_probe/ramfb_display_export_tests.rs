use std::cell::Cell;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use super::{has_virtio_gpu, interval_from, ramfb_interval_from, FramePublisher};
use bridgevm_hvf::display_fb::DisplayFramebuffer;
use bridgevm_hvf::dtb::VirtFdtConfig;
use bridgevm_hvf::fwcfg::GuestMemoryMut;
use bridgevm_hvf::platform_virt::{VirtPlatform, VirtPlatformConfig, VirtPlatformDeviceConfig};
use bridgevm_hvf::ramfb::{RamfbConfig, DRM_FORMAT_XRGB8888};

pub(super) const RAM_BASE: u64 = 0x4000_0000;
pub(super) const FB_ADDR: u64 = RAM_BASE + 0x100;

pub(super) struct TestRam {
    pub(super) bytes: Vec<u8>,
}

impl GuestMemoryMut for TestRam {
    fn write_bytes(&mut self, gpa: u64, data: &[u8]) -> bool {
        let start = (gpa - RAM_BASE) as usize;
        match self.bytes.get_mut(start..start + data.len()) {
            Some(dst) => {
                dst.copy_from_slice(data);
                true
            }
            None => false,
        }
    }

    fn read_bytes(&self, gpa: u64, len: usize) -> Option<Vec<u8>> {
        let start = usize::try_from(gpa.checked_sub(RAM_BASE)?).ok()?;
        self.bytes.get(start..start.checked_add(len)?).map(<[u8]>::to_vec)
    }

    fn host_ptr(&self, gpa: u64, len: usize) -> Option<*mut u8> {
        let start = usize::try_from(gpa.checked_sub(RAM_BASE)?).ok()?;
        let bytes = self.bytes.get(start..start.checked_add(len)?)?;
        Some(bytes.as_ptr().cast_mut())
    }
}

/// Guest memory without a direct host view: frames are copied to compare.
struct CopyOnly(TestRam);

impl GuestMemoryMut for CopyOnly {
    fn write_bytes(&mut self, gpa: u64, data: &[u8]) -> bool {
        self.0.write_bytes(gpa, data)
    }

    fn read_bytes(&self, gpa: u64, len: usize) -> Option<Vec<u8>> {
        self.0.read_bytes(gpa, len)
    }
}

/// 4x2 XRGB8888 with 16-byte rows: the stride equals the visible row.
pub(super) fn config() -> RamfbConfig {
    RamfbConfig {
        addr: FB_ADDR,
        fourcc: DRM_FORMAT_XRGB8888,
        flags: 0,
        width: 4,
        height: 2,
        stride: 16,
    }
}

pub(super) fn frame(seed: u8) -> Vec<u8> {
    (0..32).map(|byte| seed.wrapping_add(byte)).collect()
}

/// One 16 KiB stage-2 page of guest RAM at `RAM_BASE`.
pub(super) fn ram_with(pixels: &[u8]) -> TestRam {
    let mut ram = TestRam {
        bytes: vec![0; 0x4000],
    };
    assert!(ram.write_bytes(FB_ADDR, pixels));
    ram
}

pub(super) fn scratch_path() -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    std::env::temp_dir()
        .join(format!(
            "bridgevm-ramfb-display-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
        .join("display.fb")
}

fn publisher(path: &Path) -> FramePublisher {
    FramePublisher::new(DisplayFramebuffer::at_path(path))
}

/// (header u32 words, sequence, pixels) as the app reads them.
pub(super) fn read_fb(path: &Path) -> (Vec<u32>, u64, Vec<u8>) {
    let file = std::fs::read(path).unwrap();
    let word = |at: usize| u32::from_le_bytes(file[at..at + 4].try_into().unwrap());
    let header = (0..24).step_by(4).map(word).collect();
    let sequence = u64::from_le_bytes(file[24..32].try_into().unwrap());
    (header, sequence, file[64..].to_vec())
}

pub(super) fn cleanup(path: &Path) {
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}

pub(super) fn gpu_platform() -> VirtPlatform {
    VirtPlatform::new_with_config(VirtPlatformConfig {
        fdt: VirtFdtConfig::default(),
        devices: VirtPlatformDeviceConfig {
            virtio_gpu_present: true,
            ramfb_present: true,
            ..VirtPlatformDeviceConfig::default()
        },
    })
}

#[test]
fn publishes_an_xrgb8888_ramfb_frame_with_the_bvfb_header_and_pixels() {
    let path = scratch_path();
    let mut publisher = publisher(&path);
    assert!(publisher.publish_changed(Some(config()), &ram_with(&frame(1))));
    let (header, sequence, pixels) = read_fb(&path);
    assert_eq!(header, [0x4256_4642, 1, 4, 2, 16, DRM_FORMAT_XRGB8888]);
    assert_eq!(sequence, 2);
    assert_eq!(pixels, frame(1));
    cleanup(&path);
}

#[test]
fn device_presence_not_scanout_presence_decides_ownership() {
    let plain = VirtPlatform::new(VirtFdtConfig::default());
    assert!(!has_virtio_gpu(&plain));
    let gpu = gpu_platform();
    assert!(gpu.virtio_gpu_scanout().is_none(), "no scanout set yet");
    assert!(has_virtio_gpu(&gpu));
}

#[test]
fn unchanged_frames_do_not_advance_the_sequence() {
    let path = scratch_path();
    let mut publisher = publisher(&path);
    let mut ram = ram_with(&frame(1));
    assert!(publisher.publish_changed(Some(config()), &ram));
    assert!(!publisher.publish_changed(Some(config()), &ram));
    assert!(!publisher.publish_changed(Some(config()), &ram));
    assert_eq!(read_fb(&path).1, 2);
    assert!(ram.write_bytes(FB_ADDR + 5, &[0xee]));
    assert!(publisher.publish_changed(Some(config()), &ram));
    let (_, sequence, pixels) = read_fb(&path);
    assert_eq!(sequence, 4);
    assert_eq!(pixels[5], 0xee);
    assert!(ram.write_bytes(FB_ADDR + 31, &[0x77]));
    assert!(publisher.publish_changed(Some(config()), &ram), "the last byte counts");
    assert_eq!(read_fb(&path).1, 6);
    let narrower = RamfbConfig {
        width: 2,
        ..config()
    };
    assert!(publisher.publish_changed(Some(narrower), &ram));
    let (header, sequence, _) = read_fb(&path);
    assert_eq!((header[2], sequence), (2, 8), "same bytes, new geometry");
    cleanup(&path);
}

fn allow(_: u64, _: u64, _: u64) -> bool {
    true
}

static WATCH: crate::ramfb_dirty_watch::FbWatch = crate::ramfb_dirty_watch::FbWatch::new(allow);

/// Guest memory that counts how often the exporter reads it.
struct CountingRam {
    ram: TestRam,
    reads: Cell<usize>,
}

impl GuestMemoryMut for CountingRam {
    fn write_bytes(&mut self, gpa: u64, data: &[u8]) -> bool {
        self.ram.write_bytes(gpa, data)
    }

    fn read_bytes(&self, gpa: u64, len: usize) -> Option<Vec<u8>> {
        self.reads.set(self.reads.get() + 1);
        self.ram.read_bytes(gpa, len)
    }

    fn host_ptr(&self, gpa: u64, len: usize) -> Option<*mut u8> {
        self.reads.set(self.reads.get() + 1);
        self.ram.host_ptr(gpa, len)
    }
}

#[test]
fn a_watched_frame_is_read_only_after_a_guest_write() {
    let path = scratch_path();
    let mut publisher = publisher(&path).with_watch(&WATCH);
    let mut ram = CountingRam {
        ram: ram_with(&frame(1)),
        reads: Cell::new(0),
    };
    assert!(publisher.publish_changed(Some(config()), &ram));
    let reads = ram.reads.get();
    assert!(!publisher.publish_changed(Some(config()), &ram));
    assert_eq!(ram.reads.get(), reads, "no guest write, no read");
    assert!(ram.write_bytes(FB_ADDR + 3, &[0xab]));
    assert!(WATCH.take_write_fault(FB_ADDR + 3, 0x9200_004f));
    assert!(publisher.publish_changed(Some(config()), &ram));
    assert_eq!(read_fb(&path).2[3], 0xab);
    assert!(!publisher.publish_changed(Some(config()), &ram));
    WATCH.release();
    cleanup(&path);
}

fn never(ipa: u64, len: u64, _: u64) -> bool {
    panic!("protected {ipa:#x}+{len:#x}, outside guest RAM")
}

static OUTSIDE_RAM: crate::ramfb_dirty_watch::FbWatch =
    crate::ramfb_dirty_watch::FbWatch::new(never);

#[test]
fn a_frame_beyond_the_guest_ram_the_exporter_reads_is_never_write_protected() {
    let path = scratch_path();
    let mut publisher = publisher(&path).with_watch(&OUTSIDE_RAM);
    let (ram, changed) = (ram_with(&frame(1)), ram_with(&frame(2)));
    let beyond = RamfbConfig {
        addr: RAM_BASE + 0x4000,
        ..config()
    };
    assert!(!publisher.publish_changed(Some(beyond), &ram), "nothing to read there");
    assert!(publisher.publish_changed(Some(config()), &ram), "frames in RAM still publish");
    assert!(publisher.publish_changed(Some(config()), &changed), "compared every tick");
    cleanup(&path);
}

#[test]
fn memory_without_a_direct_view_still_skips_unchanged_frames() {
    let path = scratch_path();
    let mut publisher = publisher(&path);
    let mut ram = CopyOnly(ram_with(&frame(1)));
    assert!(publisher.publish_changed(Some(config()), &ram));
    assert!(!publisher.publish_changed(Some(config()), &ram));
    assert!(ram.write_bytes(FB_ADDR + 31, &[0x55]));
    assert!(publisher.publish_changed(Some(config()), &ram), "the last byte counts");
    assert_eq!(read_fb(&path).2[31], 0x55);
    cleanup(&path);
}

#[test]
fn inactive_unsupported_or_unbacked_ramfb_publishes_nothing() {
    let path = scratch_path();
    let mut publisher = publisher(&path);
    let ram = ram_with(&frame(1));
    let cases = [
        None,
        Some(RamfbConfig { addr: 0, ..config() }),
        Some(RamfbConfig {
            fourcc: 0x3432_4258, // XBGR8888
            ..config()
        }),
        Some(RamfbConfig { stride: 8, ..config() }),
        Some(RamfbConfig {
            addr: RAM_BASE + 0x10_0000,
            ..config()
        }),
        Some(RamfbConfig {
            height: u32::MAX,
            stride: u32::MAX,
            ..config()
        }),
    ];
    for case in cases {
        assert!(!publisher.publish_changed(case, &ram));
    }
    assert!(!path.exists());
    assert!(publisher.publish_changed(Some(config()), &ram), "a later frame still publishes");
    cleanup(&path);
}

#[test]
fn an_export_failure_is_logged_once_and_retried_on_the_next_tick() {
    let path = scratch_path();
    let dir = path.parent().unwrap().to_path_buf();
    std::fs::create_dir_all(dir.parent().unwrap()).unwrap();
    std::fs::write(&dir, b"a file where the directory belongs").unwrap();
    let mut publisher = publisher(&path);
    let ram = ram_with(&frame(1));
    assert!(!publisher.publish_changed(Some(config()), &ram));
    assert!(!publisher.publish_changed(Some(config()), &ram));
    assert!(publisher.failure_logged);
    std::fs::remove_file(&dir).unwrap();
    assert!(publisher.publish_changed(Some(config()), &ram));
    assert_eq!(read_fb(&path).1, 2);
    cleanup(&path);
}

#[test]
fn a_recreated_process_republishes_into_the_same_file() {
    let path = scratch_path();
    let mut first = publisher(&path);
    assert!(first.publish_changed(Some(config()), &ram_with(&frame(3))));
    let changed = ram_with(&frame(4));
    assert!(first.publish_changed(Some(config()), &changed));
    assert_eq!(read_fb(&path).1, 4);
    let inode = std::fs::metadata(&path).unwrap().ino();
    drop(first);
    let mut second = publisher(&path);
    assert!(second.publish_changed(Some(config()), &changed));
    assert_eq!(std::fs::metadata(&path).unwrap().ino(), inode);
    assert_eq!(read_fb(&path).1, 6, "a fresh writer continues the sequence");
    assert_eq!(read_fb(&path).2, frame(4));
    cleanup(&path);
}

#[test]
fn cadence_matches_the_ppm_exporter_bounds() {
    let ms = |value: Option<&str>| interval_from(value).as_millis();
    assert_eq!(ms(None), 500);
    assert_eq!(ms(Some("100")), 100);
    assert_eq!(ms(Some("60000")), 60_000);
    assert_eq!(ms(Some("99")), 500);
    assert_eq!(ms(Some("60001")), 500);
    assert_eq!(ms(Some("fast")), 500);
}

#[test]
fn the_ramfb_thread_has_its_own_bounded_period_and_otherwise_the_shared_one() {
    let ms = |own, shared| ramfb_interval_from(own, shared).as_millis();
    assert_eq!(ms(Some("33"), Some("100")), 33);
    assert_eq!(ms(Some("16"), None), 16);
    assert_eq!(ms(Some("60000"), None), 60_000);
    // Out of range or unparsable: the shared cadence, bounded as before.
    assert_eq!(ms(Some("15"), Some("100")), 100);
    assert_eq!(ms(Some("60001"), Some("250")), 250);
    assert_eq!(ms(Some("fast"), None), 500);
    assert_eq!(ms(None, Some("100")), 100);
    assert_eq!(ms(None, Some("33")), 500, "the shared floor stays 100 ms");
    assert_eq!(ms(None, None), 500);
}
