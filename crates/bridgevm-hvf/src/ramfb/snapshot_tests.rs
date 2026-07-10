use crate::fwcfg::GuestMemoryMut;

use super::{RamfbConfig, RamfbSnapshot, RamfbSnapshotError, DRM_FORMAT_XRGB8888};

struct TestRam {
    base: u64,
    bytes: Vec<u8>,
}

impl TestRam {
    fn new(base: u64, bytes: Vec<u8>) -> Self {
        Self { base, bytes }
    }
}

impl GuestMemoryMut for TestRam {
    fn write_bytes(&mut self, gpa: u64, data: &[u8]) -> bool {
        let Some(start) = gpa
            .checked_sub(self.base)
            .and_then(|value| usize::try_from(value).ok())
        else {
            return false;
        };
        let Some(end) = start.checked_add(data.len()) else {
            return false;
        };
        if end > self.bytes.len() {
            return false;
        }
        self.bytes[start..end].copy_from_slice(data);
        true
    }

    fn read_bytes(&self, gpa: u64, len: usize) -> Option<Vec<u8>> {
        let start = gpa
            .checked_sub(self.base)
            .and_then(|value| usize::try_from(value).ok())?;
        let end = start.checked_add(len)?;
        self.bytes.get(start..end).map(<[u8]>::to_vec)
    }
}

#[test]
fn snapshot_summarizes_xrgb8888_framebuffer() {
    let config = RamfbConfig {
        addr: 0x4008_0000,
        fourcc: DRM_FORMAT_XRGB8888,
        flags: 0,
        width: 2,
        height: 1,
        stride: 8,
    };
    let ram = TestRam::new(
        0x4008_0000,
        vec![0x03, 0x02, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00],
    );

    let snapshot = RamfbSnapshot::read_from(&ram, config).unwrap();

    assert_eq!(snapshot.summary.byte_len, 8);
    assert_eq!(snapshot.summary.pixel_count, 2);
    assert_eq!(snapshot.summary.nonzero_bytes, 3);
    assert_eq!(snapshot.summary.nonzero_pixels, 1);
    assert_eq!(snapshot.summary.first_nonzero_pixel, Some(0));
    assert_eq!(
        snapshot.ppm_bytes().unwrap(),
        b"P6\n2 1\n255\n\x01\x02\x03\x00\x00\x00"
    );
}

#[test]
fn borrowed_summary_matches_owned_snapshot_without_requiring_snapshot_bytes() {
    let config = RamfbConfig {
        addr: 0x4008_0000,
        fourcc: DRM_FORMAT_XRGB8888,
        flags: 0,
        width: 2,
        height: 1,
        stride: 8,
    };
    let bytes = [0x03, 0x02, 0x01, 0x00, 0x06, 0x05, 0x04, 0x00];

    let borrowed = RamfbSnapshot::summarize_xrgb8888_bytes(config, &bytes).unwrap();
    let owned = RamfbSnapshot::from_xrgb8888_bytes(config, bytes.to_vec())
        .unwrap()
        .summary;

    assert_eq!(borrowed, owned);
    assert_eq!(borrowed.nonzero_pixels, 2);
    assert_eq!(borrowed.unique_colors, 2);
}

#[test]
fn borrowed_ppm_matches_owned_snapshot_without_requiring_snapshot_bytes() {
    let config = RamfbConfig {
        addr: 0x4008_0000,
        fourcc: DRM_FORMAT_XRGB8888,
        flags: 0,
        width: 2,
        height: 1,
        stride: 8,
    };
    let bytes = [0x03, 0x02, 0x01, 0x00, 0x06, 0x05, 0x04, 0x00];
    let owned = RamfbSnapshot::from_xrgb8888_bytes(config, bytes.to_vec()).unwrap();

    assert_eq!(
        RamfbSnapshot::ppm_bytes_from_xrgb8888(config, &bytes).unwrap(),
        owned.ppm_bytes().unwrap()
    );
}

#[test]
fn borrowed_ppm_requires_the_exact_framebuffer_length() {
    let config = RamfbConfig {
        addr: 0x4008_0000,
        fourcc: DRM_FORMAT_XRGB8888,
        flags: 0,
        width: 1,
        height: 2,
        stride: 8,
    };
    let expected = Err(RamfbSnapshotError::GuestMemoryOutOfRange {
        addr: config.addr,
        len: 16,
    });

    assert_eq!(
        RamfbSnapshot::ppm_bytes_from_xrgb8888(config, &[0; 15]),
        expected
    );
    assert_eq!(
        RamfbSnapshot::ppm_bytes_from_xrgb8888(config, &[0; 17]),
        expected
    );
}

#[test]
fn snapshot_ignores_x_byte_when_counting_nonzero_pixels() {
    let config = RamfbConfig {
        addr: 0x4008_0000,
        fourcc: DRM_FORMAT_XRGB8888,
        flags: 0,
        width: 1,
        height: 1,
        stride: 4,
    };
    let ram = TestRam::new(0x4008_0000, vec![0x00, 0x00, 0x00, 0xff]);

    let snapshot = RamfbSnapshot::read_from(&ram, config).unwrap();

    assert_eq!(snapshot.summary.nonzero_bytes, 1);
    assert_eq!(snapshot.summary.nonzero_pixels, 0);
    assert_eq!(snapshot.summary.zero_pixels, 1);
    assert_eq!(snapshot.summary.first_nonzero_pixel, None);
    assert_eq!(snapshot.ppm_bytes().unwrap(), b"P6\n1 1\n255\n\x00\x00\x00");
}

#[test]
fn snapshot_preserves_stride_padding_in_byte_len() {
    let config = RamfbConfig {
        addr: 0x4008_0000,
        fourcc: DRM_FORMAT_XRGB8888,
        flags: 0,
        width: 1,
        height: 2,
        stride: 8,
    };
    let ram = TestRam::new(
        0x4008_0000,
        vec![
            0x03, 0x02, 0x01, 0x00, 0xaa, 0xbb, 0xcc, 0xdd, 0x06, 0x05, 0x04, 0x00, 0x11, 0x22,
            0x33, 0x44,
        ],
    );

    let snapshot = RamfbSnapshot::read_from(&ram, config).unwrap();

    assert_eq!(config.framebuffer_len().unwrap(), 16);
    assert_eq!(snapshot.summary.byte_len, 16);
    assert_eq!(snapshot.summary.pixel_count, 2);
    assert_eq!(
        snapshot.ppm_bytes().unwrap(),
        b"P6\n1 2\n255\n\x01\x02\x03\x04\x05\x06"
    );
}

#[test]
fn snapshot_rejects_inactive_config() {
    let ram = TestRam::new(0x4000_0000, vec![0; 16]);
    let config = RamfbConfig {
        addr: 0,
        fourcc: DRM_FORMAT_XRGB8888,
        flags: 0,
        width: 1,
        height: 1,
        stride: 4,
    };

    assert_eq!(
        RamfbSnapshot::read_from(&ram, config),
        Err(RamfbSnapshotError::Inactive)
    );
}

#[test]
fn snapshot_rejects_unsupported_format_before_sizing() {
    let ram = TestRam::new(0x4000_0000, vec![0; 16]);
    let config = RamfbConfig {
        addr: 0x4000_0000,
        fourcc: 0x3432_4241,
        flags: 0,
        width: 1,
        height: 1,
        stride: 4,
    };

    assert_eq!(
        config.framebuffer_len(),
        Err(RamfbSnapshotError::UnsupportedFormat {
            fourcc: config.fourcc
        })
    );
    assert_eq!(
        RamfbSnapshot::read_from(&ram, config),
        Err(RamfbSnapshotError::UnsupportedFormat {
            fourcc: config.fourcc
        })
    );
}

#[test]
fn snapshot_rejects_out_of_range_guest_memory() {
    let ram = TestRam::new(0x4000_0000, vec![0; 16]);
    let config = RamfbConfig {
        addr: 0x5000_0000,
        fourcc: DRM_FORMAT_XRGB8888,
        flags: 0,
        width: 2,
        height: 1,
        stride: 8,
    };

    assert_eq!(
        RamfbSnapshot::read_from(&ram, config),
        Err(RamfbSnapshotError::GuestMemoryOutOfRange {
            addr: config.addr,
            len: 8
        })
    );
}

#[test]
fn snapshot_rejects_too_small_stride() {
    let ram = TestRam::new(0x4000_0000, vec![0; 16]);
    let config = RamfbConfig {
        addr: 0x4000_0000,
        fourcc: DRM_FORMAT_XRGB8888,
        flags: 0,
        width: 2,
        height: 1,
        stride: 4,
    };

    assert_eq!(
        RamfbSnapshot::read_from(&ram, config),
        Err(RamfbSnapshotError::StrideTooSmall {
            stride: 4,
            min_stride: 8
        })
    );
}
