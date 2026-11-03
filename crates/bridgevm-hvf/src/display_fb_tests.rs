use super::DisplayFramebuffer;
use crate::ramfb::DRM_FORMAT_XRGB8888;
use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

fn test_dir() -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "bridgevm-display-fb-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn le_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

fn le_u64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap())
}

#[test]
fn publish_writes_the_bvfb_header_even_sequence_and_pixels() {
    let dir = test_dir();
    let path = dir.join("nested").join("display.fb");
    let mut fb = DisplayFramebuffer::at_path(&path);
    assert!(
        !path.exists(),
        "nothing is created before the first publish"
    );
    let pixels: Vec<u8> = (0..24).collect();
    fb.publish(2, 2, 12, DRM_FORMAT_XRGB8888, &pixels).unwrap();
    let file = std::fs::read(&path).unwrap();
    assert_eq!(file.len(), 64 + 24);
    let header: Vec<u32> = (0..24).step_by(4).map(|at| le_u32(&file, at)).collect();
    assert_eq!(header, [0x4256_4642, 1, 2, 2, 12, DRM_FORMAT_XRGB8888]);
    assert_eq!(le_u64(&file, 24), 2);
    assert_eq!(fb.sequence(), 2);
    assert_eq!(&file[64..], &pixels[..]);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn short_or_oversized_frames_are_rejected_without_creating_the_file() {
    let dir = test_dir();
    let path = dir.join("display.fb");
    let mut fb = DisplayFramebuffer::at_path(&path);
    let short = fb
        .publish(2, 2, 8, DRM_FORMAT_XRGB8888, &[0; 15])
        .unwrap_err();
    assert_eq!(short.kind(), std::io::ErrorKind::InvalidInput);
    let huge = fb.publish(1, u32::MAX, u32::MAX, DRM_FORMAT_XRGB8888, &[0; 16]);
    assert_eq!(huge.unwrap_err().kind(), std::io::ErrorKind::InvalidInput);
    assert!(!path.exists());
    assert_eq!(fb.sequence(), 0);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn an_unusable_path_is_an_error_not_a_panic() {
    let dir = test_dir();
    let blocker = dir.join("not-a-directory");
    std::fs::write(&blocker, b"file").unwrap();
    let mut fb = DisplayFramebuffer::at_path(blocker.join("display.fb"));
    let error = fb
        .publish(1, 1, 4, DRM_FORMAT_XRGB8888, &[1; 4])
        .unwrap_err();
    assert_ne!(error.kind(), std::io::ErrorKind::InvalidInput);
    assert_eq!(fb.sequence(), 0);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_later_writer_restarts_the_same_file_in_place() {
    // Process recreation: the old writer is gone, the reader may still map the
    // file, and the new process's first publish reuses the path and inode.
    let dir = test_dir();
    let path = dir.join("display.fb");
    let mut first = DisplayFramebuffer::at_path(&path);
    first
        .publish(2, 1, 8, DRM_FORMAT_XRGB8888, &[1; 8])
        .unwrap();
    first
        .publish(2, 1, 8, DRM_FORMAT_XRGB8888, &[2; 8])
        .unwrap();
    assert_eq!(first.sequence(), 4);
    let inode = std::fs::metadata(&path).unwrap().ino();
    drop(first);
    let mut second = DisplayFramebuffer::at_path(&path);
    second
        .publish(2, 1, 8, DRM_FORMAT_XRGB8888, &[7; 8])
        .unwrap();
    let file = std::fs::read(&path).unwrap();
    assert_eq!(std::fs::metadata(&path).unwrap().ino(), inode);
    assert_eq!(file.len(), 64 + 8);
    assert_eq!(le_u64(&file, 24), 6);
    assert_eq!(&file[64..], &[7; 8]);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_later_writer_never_shrinks_a_file_a_reader_may_map() {
    let dir = test_dir();
    let path = dir.join("display.fb");
    let mut first = DisplayFramebuffer::at_path(&path);
    first
        .publish(2, 2, 8, DRM_FORMAT_XRGB8888, &[1; 16])
        .unwrap();
    drop(first);
    let mut second = DisplayFramebuffer::at_path(&path);
    second
        .publish(2, 1, 8, DRM_FORMAT_XRGB8888, &[3; 8])
        .unwrap();
    let file = std::fs::read(&path).unwrap();
    assert_eq!(file.len(), 64 + 16);
    assert_eq!(le_u32(&file, 12), 1);
    assert_eq!(le_u64(&file, 24), 4);
    assert_eq!(&file[64..72], &[3; 8]);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_shared_writer_knows_whose_frame_is_newest_and_how_old_it_is() {
    let dir = test_dir();
    let mut device = DisplayFramebuffer::at_path(dir.join("display.fb"));
    let mut ramfb = device.share();
    assert_eq!(ramfb.other_frame_age(), None, "nothing published yet");
    device
        .publish(1, 1, 4, DRM_FORMAT_XRGB8888, &[1; 4])
        .unwrap();
    let age = ramfb
        .other_frame_age()
        .expect("the device's frame is newest");
    assert!(age < Duration::from_secs(5));
    assert_eq!(device.other_frame_age(), None, "its own frame");
    ramfb
        .publish(1, 1, 4, DRM_FORMAT_XRGB8888, &[2; 4])
        .unwrap();
    assert_eq!(ramfb.other_frame_age(), None);
    assert!(device.other_frame_age().is_some());
    assert_eq!((device.sequence(), ramfb.sequence()), (4, 4));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn the_platform_hands_out_a_writer_sharing_the_virtio_gpu_devices() {
    use crate::dtb::VirtFdtConfig;
    use crate::platform_virt::{VirtPlatform, VirtPlatformConfig, VirtPlatformDeviceConfig};
    let plain = VirtPlatform::new(VirtFdtConfig::default());
    assert!(plain.display_framebuffer().is_none(), "no device");
    let mut gpu = VirtPlatform::new_with_config(VirtPlatformConfig {
        fdt: VirtFdtConfig::default(),
        devices: VirtPlatformDeviceConfig {
            virtio_gpu_present: true,
            ..VirtPlatformDeviceConfig::default()
        },
    });
    let dir = test_dir();
    let device = &mut gpu.virtio_gpu.as_mut().unwrap().gpu.fb_sink;
    *device = Some(crate::virtio_gpu::FbSink::at_path(dir.join("display.fb")));
    let mut ramfb = gpu.display_framebuffer().expect("the device's file");
    ramfb
        .publish(1, 1, 4, DRM_FORMAT_XRGB8888, &[1; 4])
        .unwrap();
    let device = gpu.virtio_gpu.as_mut().unwrap().gpu.fb_sink.as_mut();
    device.unwrap().write(1, 1, 4, DRM_FORMAT_XRGB8888, &[2; 4]);
    assert_eq!(ramfb.sequence(), 4, "one sequence for both producers");
    assert!(
        ramfb.other_frame_age().is_some(),
        "the device's frame is newest"
    );
    std::fs::remove_dir_all(dir).unwrap();
}
