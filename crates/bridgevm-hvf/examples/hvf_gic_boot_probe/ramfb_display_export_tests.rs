use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use super::{has_virtio_gpu, interval_from, RamfbDisplayExporter};
use bridgevm_hvf::display_fb::DisplayFramebuffer;
use bridgevm_hvf::dtb::VirtFdtConfig;
use bridgevm_hvf::fwcfg::GuestMemoryMut;
use bridgevm_hvf::platform_virt::{VirtPlatform, VirtPlatformConfig, VirtPlatformDeviceConfig};
use bridgevm_hvf::ramfb::{RamfbConfig, DRM_FORMAT_XRGB8888};

const RAM_BASE: u64 = 0x4000_0000;
const FB_ADDR: u64 = RAM_BASE + 0x100;
const TICK: Duration = Duration::from_millis(100);

struct TestRam {
    bytes: Vec<u8>,
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
}

/// 4x2 XRGB8888 with 16-byte rows: the stride equals the visible row.
fn config() -> RamfbConfig {
    RamfbConfig {
        addr: FB_ADDR,
        fourcc: DRM_FORMAT_XRGB8888,
        flags: 0,
        width: 4,
        height: 2,
        stride: 16,
    }
}

fn frame(seed: u8) -> Vec<u8> {
    (0..32).map(|byte| seed.wrapping_add(byte)).collect()
}

fn ram_with(pixels: &[u8]) -> TestRam {
    let mut ram = TestRam {
        bytes: vec![0; 0x1000],
    };
    assert!(ram.write_bytes(FB_ADDR, pixels));
    ram
}

fn scratch_path() -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    std::env::temp_dir()
        .join(format!(
            "bridgevm-ramfb-display-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
        .join("display.fb")
}

fn exporter(path: &Path) -> RamfbDisplayExporter {
    RamfbDisplayExporter::new(Some(DisplayFramebuffer::at_path(path)), TICK)
}

/// (header u32 words, sequence, pixels) as the app reads them.
fn read_fb(path: &Path) -> (Vec<u32>, u64, Vec<u8>) {
    let file = std::fs::read(path).unwrap();
    let word = |at: usize| u32::from_le_bytes(file[at..at + 4].try_into().unwrap());
    let header = (0..24).step_by(4).map(word).collect();
    let sequence = u64::from_le_bytes(file[24..32].try_into().unwrap());
    (header, sequence, file[64..].to_vec())
}

fn cleanup(path: &Path) {
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}

#[test]
fn publishes_an_xrgb8888_ramfb_frame_with_the_bvfb_header_and_pixels() {
    let path = scratch_path();
    let mut exporter = exporter(&path);
    let t0 = Instant::now();
    assert!(exporter.due(t0));
    exporter.export_frame_due(false, Some(config()), &ram_with(&frame(1)), t0);
    let (header, sequence, pixels) = read_fb(&path);
    assert_eq!(header, [0x4256_4642, 1, 4, 2, 16, DRM_FORMAT_XRGB8888]);
    assert_eq!(sequence, 2);
    assert_eq!(pixels, frame(1));
    assert!(!exporter.due(t0 + TICK / 2), "cadence holds the next export");
    cleanup(&path);
}

#[test]
fn inert_for_the_process_when_the_machine_has_a_virtio_gpu_device() {
    let path = scratch_path();
    let mut exporter = exporter(&path);
    let ram = ram_with(&frame(1));
    let t0 = Instant::now();
    exporter.export_frame_due(true, Some(config()), &ram, t0);
    assert!(!path.exists(), "the virtio-gpu device owns the display file");
    assert!(!exporter.due(t0 + 10 * TICK), "retired, not merely skipped");
    exporter.export_frame_due(false, Some(config()), &ram, t0 + 10 * TICK);
    assert!(!path.exists());
    cleanup(&path);
}

#[test]
fn device_presence_not_scanout_presence_decides_ownership() {
    let plain = VirtPlatform::new(VirtFdtConfig::default());
    assert!(!has_virtio_gpu(&plain));
    let gpu = VirtPlatform::new_with_config(VirtPlatformConfig {
        fdt: VirtFdtConfig::default(),
        devices: VirtPlatformDeviceConfig {
            virtio_gpu_present: true,
            ramfb_present: true,
            ..VirtPlatformDeviceConfig::default()
        },
    });
    assert!(gpu.virtio_gpu_scanout().is_none(), "no scanout set yet");
    assert!(has_virtio_gpu(&gpu));
}

#[test]
fn unchanged_frames_do_not_advance_the_sequence() {
    let path = scratch_path();
    let mut exporter = exporter(&path);
    let t0 = Instant::now();
    let mut ram = ram_with(&frame(1));
    exporter.export_frame_due(false, Some(config()), &ram, t0);
    exporter.export_frame_due(false, Some(config()), &ram, t0 + TICK);
    exporter.export_frame_due(false, Some(config()), &ram, t0 + 2 * TICK);
    assert_eq!(read_fb(&path).1, 2);
    assert!(ram.write_bytes(FB_ADDR + 5, &[0xee]));
    exporter.export_frame_due(false, Some(config()), &ram, t0 + 3 * TICK);
    let (_, sequence, pixels) = read_fb(&path);
    assert_eq!(sequence, 4);
    assert_eq!(pixels[5], 0xee);
    let narrower = RamfbConfig {
        width: 2,
        ..config()
    };
    exporter.export_frame_due(false, Some(narrower), &ram, t0 + 4 * TICK);
    let (header, sequence, _) = read_fb(&path);
    assert_eq!((header[2], sequence), (2, 6), "same bytes, new geometry");
    cleanup(&path);
}

#[test]
fn a_change_before_the_cadence_waits_for_the_next_tick() {
    let path = scratch_path();
    let mut exporter = exporter(&path);
    let t0 = Instant::now();
    exporter.export_frame_due(false, Some(config()), &ram_with(&frame(1)), t0);
    let changed = ram_with(&frame(9));
    exporter.export_frame_due(false, Some(config()), &changed, t0 + TICK / 2);
    assert_eq!(read_fb(&path).1, 2);
    exporter.export_frame_due(false, Some(config()), &changed, t0 + TICK);
    let (_, sequence, pixels) = read_fb(&path);
    assert_eq!((sequence, pixels), (4, frame(9)));
    cleanup(&path);
}

#[test]
fn inactive_unsupported_or_unbacked_ramfb_publishes_nothing() {
    let path = scratch_path();
    let mut exporter = exporter(&path);
    let ram = ram_with(&frame(1));
    let t0 = Instant::now();
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
    for (step, case) in cases.into_iter().enumerate() {
        exporter.export_frame_due(false, case, &ram, t0 + TICK * step as u32);
    }
    assert!(!path.exists());
    assert!(exporter.due(t0 + TICK * 6), "still armed for a later frame");
    cleanup(&path);
}

#[test]
fn inert_without_a_display_path() {
    let mut exporter = RamfbDisplayExporter::new(None, TICK);
    let t0 = Instant::now();
    assert!(!exporter.due(t0));
    exporter.export_frame_due(false, Some(config()), &ram_with(&frame(1)), t0);
    assert!(!exporter.due(t0 + TICK));
}

#[test]
fn an_export_failure_is_logged_once_and_retried_on_the_next_tick() {
    let path = scratch_path();
    let dir = path.parent().unwrap().to_path_buf();
    std::fs::create_dir_all(dir.parent().unwrap()).unwrap();
    std::fs::write(&dir, b"a file where the directory belongs").unwrap();
    let mut exporter = exporter(&path);
    let ram = ram_with(&frame(1));
    let t0 = Instant::now();
    exporter.export_frame_due(false, Some(config()), &ram, t0);
    exporter.export_frame_due(false, Some(config()), &ram, t0 + TICK);
    assert!(exporter.failure_logged);
    assert!(exporter.due(t0 + 2 * TICK));
    std::fs::remove_file(&dir).unwrap();
    exporter.export_frame_due(false, Some(config()), &ram, t0 + 2 * TICK);
    assert_eq!(read_fb(&path).1, 2);
    cleanup(&path);
}

#[test]
fn a_recreated_process_republishes_into_the_same_file() {
    let path = scratch_path();
    let ram = ram_with(&frame(3));
    let mut first = exporter(&path);
    let t0 = Instant::now();
    first.export_frame_due(false, Some(config()), &ram, t0);
    let changed = ram_with(&frame(4));
    first.export_frame_due(false, Some(config()), &changed, t0 + TICK);
    assert_eq!(read_fb(&path).1, 4);
    let inode = std::fs::metadata(&path).unwrap().ino();
    drop(first);
    let mut second = exporter(&path);
    second.export_frame_due(false, Some(config()), &changed, t0 + 2 * TICK);
    assert_eq!(std::fs::metadata(&path).unwrap().ino(), inode);
    assert_eq!(read_fb(&path).1, 2, "a fresh writer restarts its sequence");
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
