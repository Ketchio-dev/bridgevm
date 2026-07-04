use std::collections::BTreeSet;

use crate::fwcfg::GuestMemoryMut;

use super::RamfbConfig;

const XRGB8888_BYTES_PER_PIXEL: u64 = 4;
const RGB888_BYTES_PER_PIXEL: u64 = 3;
const FNV64_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV64_PRIME: u64 = 0x0000_0100_0000_01b3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RamfbSnapshotError {
    Inactive,
    UnsupportedFormat { fourcc: u32 },
    StrideTooSmall { stride: u32, min_stride: u64 },
    SizeOverflow,
    GuestMemoryOutOfRange { addr: u64, len: usize },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RamfbSnapshotSummary {
    pub byte_len: usize,
    pub pixel_count: u64,
    pub nonzero_bytes: usize,
    pub nonzero_pixels: u64,
    pub zero_pixels: u64,
    pub unique_colors: usize,
    pub first_nonzero_pixel: Option<u64>,
    pub checksum64: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RamfbSnapshot {
    pub config: RamfbConfig,
    pub bytes: Vec<u8>,
    pub summary: RamfbSnapshotSummary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FramebufferGeometry {
    byte_len: usize,
    stride: usize,
    height: usize,
    row_bytes: usize,
}

pub(super) fn framebuffer_len(config: RamfbConfig) -> Result<usize, RamfbSnapshotError> {
    ensure_xrgb8888(config)?;
    framebuffer_geometry(config).map(|geometry| geometry.byte_len)
}

impl RamfbSnapshot {
    pub fn from_xrgb8888_bytes(
        config: RamfbConfig,
        bytes: Vec<u8>,
    ) -> Result<Self, RamfbSnapshotError> {
        ensure_xrgb8888(config)?;
        let geometry = framebuffer_geometry(config)?;
        if bytes.len() != geometry.byte_len {
            return Err(RamfbSnapshotError::GuestMemoryOutOfRange {
                addr: config.addr,
                len: geometry.byte_len,
            });
        }
        let summary = summarize_xrgb8888(config, geometry, &bytes)?;
        Ok(Self {
            config,
            bytes,
            summary,
        })
    }

    pub fn read_from(
        mem: &dyn GuestMemoryMut,
        config: RamfbConfig,
    ) -> Result<Self, RamfbSnapshotError> {
        ensure_xrgb8888(config)?;
        let geometry = framebuffer_geometry(config)?;
        let len = geometry.byte_len;
        let Some(bytes) = mem.read_bytes(config.addr, len) else {
            return Err(RamfbSnapshotError::GuestMemoryOutOfRange {
                addr: config.addr,
                len,
            });
        };
        let summary = summarize_xrgb8888(config, geometry, &bytes)?;
        Ok(Self {
            config,
            bytes,
            summary,
        })
    }

    pub fn ppm_bytes(&self) -> Result<Vec<u8>, RamfbSnapshotError> {
        ensure_xrgb8888(self.config)?;
        let geometry = framebuffer_geometry(self.config)?;
        let rgb_len = u64::from(self.config.width)
            .checked_mul(u64::from(self.config.height))
            .and_then(|pixels| pixels.checked_mul(RGB888_BYTES_PER_PIXEL))
            .ok_or(RamfbSnapshotError::SizeOverflow)?;
        let rgb_len = usize::try_from(rgb_len).map_err(|_| RamfbSnapshotError::SizeOverflow)?;
        let header = format!("P6\n{} {}\n255\n", self.config.width, self.config.height);
        let mut out = Vec::with_capacity(header.len() + rgb_len);
        out.extend_from_slice(header.as_bytes());
        for row in 0..geometry.height {
            let row_start = row * geometry.stride;
            let row_end = row_start + geometry.row_bytes;
            let row_bytes = self.bytes.get(row_start..row_end).ok_or(
                RamfbSnapshotError::GuestMemoryOutOfRange {
                    addr: self.config.addr,
                    len: geometry.byte_len,
                },
            )?;
            for pixel in row_bytes.chunks_exact(4) {
                out.extend_from_slice(&[pixel[2], pixel[1], pixel[0]]);
            }
        }
        Ok(out)
    }
}

fn ensure_xrgb8888(config: RamfbConfig) -> Result<(), RamfbSnapshotError> {
    if !config.is_active() {
        return Err(RamfbSnapshotError::Inactive);
    }
    if !config.is_xrgb8888() {
        return Err(RamfbSnapshotError::UnsupportedFormat {
            fourcc: config.fourcc,
        });
    }
    Ok(())
}

fn framebuffer_geometry(config: RamfbConfig) -> Result<FramebufferGeometry, RamfbSnapshotError> {
    if !config.is_active() {
        return Err(RamfbSnapshotError::Inactive);
    }
    let min_stride = u64::from(config.width)
        .checked_mul(XRGB8888_BYTES_PER_PIXEL)
        .ok_or(RamfbSnapshotError::SizeOverflow)?;
    if u64::from(config.stride) < min_stride {
        return Err(RamfbSnapshotError::StrideTooSmall {
            stride: config.stride,
            min_stride,
        });
    }
    let byte_len = u64::from(config.stride)
        .checked_mul(u64::from(config.height))
        .ok_or(RamfbSnapshotError::SizeOverflow)?;
    Ok(FramebufferGeometry {
        byte_len: usize::try_from(byte_len).map_err(|_| RamfbSnapshotError::SizeOverflow)?,
        stride: usize::try_from(config.stride).map_err(|_| RamfbSnapshotError::SizeOverflow)?,
        height: usize::try_from(config.height).map_err(|_| RamfbSnapshotError::SizeOverflow)?,
        row_bytes: usize::try_from(min_stride).map_err(|_| RamfbSnapshotError::SizeOverflow)?,
    })
}

fn summarize_xrgb8888(
    config: RamfbConfig,
    geometry: FramebufferGeometry,
    bytes: &[u8],
) -> Result<RamfbSnapshotSummary, RamfbSnapshotError> {
    let mut checksum64 = FNV64_OFFSET;
    let mut nonzero_bytes = 0usize;
    for byte in bytes {
        if *byte != 0 {
            nonzero_bytes += 1;
        }
        checksum64 ^= u64::from(*byte);
        checksum64 = checksum64.wrapping_mul(FNV64_PRIME);
    }

    let mut unique_colors = BTreeSet::new();
    let mut nonzero_pixels = 0u64;
    let mut first_nonzero_pixel = None;
    for row in 0..geometry.height {
        let row_start = row * geometry.stride;
        let row_end = row_start + geometry.row_bytes;
        for (col, pixel) in bytes[row_start..row_end].chunks_exact(4).enumerate() {
            if pixel[0..3] != [0, 0, 0] {
                let row_index = u64::try_from(row).map_err(|_| RamfbSnapshotError::SizeOverflow)?;
                let col_index = u64::try_from(col).map_err(|_| RamfbSnapshotError::SizeOverflow)?;
                let pixel_index = row_index * u64::from(config.width) + col_index;
                nonzero_pixels += 1;
                first_nonzero_pixel.get_or_insert(pixel_index);
            }
            unique_colors.insert((pixel[2], pixel[1], pixel[0]));
        }
    }
    let pixel_count = u64::from(config.width) * u64::from(config.height);
    Ok(RamfbSnapshotSummary {
        byte_len: bytes.len(),
        pixel_count,
        nonzero_bytes,
        nonzero_pixels,
        zero_pixels: pixel_count.saturating_sub(nonzero_pixels),
        unique_colors: unique_colors.len(),
        first_nonzero_pixel,
        checksum64,
    })
}
