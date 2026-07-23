//! Guest-backed pre-context RESOURCE_COPY_REGION emulation and its scatter-gather pixel copy.

use super::*;
use crate::fwcfg::GuestMemoryMut;

pub(crate) fn parse_local_resource_copy_region(command: &[u8]) -> Option<LocalResourceCopyRegion> {
    if command.len() != VIRGL_RESOURCE_COPY_REGION_BYTES {
        return None;
    }
    let header = read_le_u32(command, 0)?;
    if header & 0xff != VIRGL_CCMD_RESOURCE_COPY_REGION
        || (header >> 8) & 0xff != 0
        || header >> 16 != VIRGL_RESOURCE_COPY_REGION_PAYLOAD_DWORDS
        || read_le_u32(command, 8)? != 0
        || read_le_u32(command, 20)? != 0
        || read_le_u32(command, 28)? != 0
        || read_le_u32(command, 40)? != 0
        || read_le_u32(command, 52)? != 1
    {
        return None;
    }
    Some(LocalResourceCopyRegion {
        dst_resource_id: read_le_u32(command, 4)?,
        dst_x: read_le_u32(command, 12)?,
        dst_y: read_le_u32(command, 16)?,
        src_resource_id: read_le_u32(command, 24)?,
        src_x: read_le_u32(command, 32)?,
        src_y: read_le_u32(command, 36)?,
        width: read_le_u32(command, 44)?,
        height: read_le_u32(command, 48)?,
    })
}

pub(crate) fn backing_covers_32bpp_resource(backing: &[BlobMemEntry], info: Create3dArgs) -> bool {
    let Some(required) = u64::from(info.width)
        .checked_mul(u64::from(info.height))
        .and_then(|pixels| pixels.checked_mul(4))
    else {
        return false;
    };
    let available = backing.iter().fold(0u64, |total, entry| {
        total.saturating_add(u64::from(entry.len))
    });
    available >= required
}

pub(crate) fn resource_32bpp_offset(width: u32, x: u32, y: u32) -> Option<u64> {
    u64::from(y)
        .checked_mul(u64::from(width))
        .and_then(|row| row.checked_add(u64::from(x)))
        .and_then(|pixels| pixels.checked_mul(4))
}

pub(crate) fn read_scattered_backing_into(
    mem: &dyn GuestMemoryMut,
    backing: &[BlobMemEntry],
    mut offset: u64,
    dst: &mut [u8],
) -> bool {
    let mut copied = 0usize;
    for entry in backing {
        let entry_len = u64::from(entry.len);
        if offset >= entry_len {
            offset -= entry_len;
            continue;
        }
        let available = usize::try_from(entry_len - offset).unwrap_or(usize::MAX);
        let count = available.min(dst.len().saturating_sub(copied));
        let Some(gpa) = entry.addr.checked_add(offset) else {
            return false;
        };
        if !mem.read_into(gpa, &mut dst[copied..copied + count]) {
            return false;
        }
        copied += count;
        if copied == dst.len() {
            return true;
        }
        offset = 0;
    }
    copied == dst.len()
}

pub(crate) fn write_scattered_backing(
    mem: &dyn GuestMemoryMut,
    backing: &[BlobMemEntry],
    mut offset: u64,
    src: &[u8],
) -> bool {
    let mut copied = 0usize;
    for entry in backing {
        let entry_len = u64::from(entry.len);
        if offset >= entry_len {
            offset -= entry_len;
            continue;
        }
        let available = usize::try_from(entry_len - offset).unwrap_or(usize::MAX);
        let count = available.min(src.len().saturating_sub(copied));
        let Some(gpa) = entry.addr.checked_add(offset) else {
            return false;
        };
        let Some(host_ptr) = mem.host_ptr(gpa, count) else {
            return false;
        };
        if host_ptr.is_null() {
            return false;
        }
        // `host_ptr` is the GuestMemoryMut contract for a stable writable view
        // of this exact guest-RAM span. The source is our private scratch row,
        // so it cannot alias the destination mapping.
        unsafe {
            std::ptr::copy_nonoverlapping(src[copied..].as_ptr(), host_ptr, count);
        }
        copied += count;
        if copied == src.len() {
            return true;
        }
        offset = 0;
    }
    copied == src.len()
}

pub(crate) const VIRGL_CCMD_RESOURCE_COPY_REGION: u32 = 17;

pub(crate) const VIRGL_RESOURCE_COPY_REGION_PAYLOAD_DWORDS: u32 = 13;

pub(crate) const VIRGL_RESOURCE_COPY_REGION_DWORDS: usize = 14;

pub(crate) const VIRGL_RESOURCE_COPY_REGION_BYTES: usize = VIRGL_RESOURCE_COPY_REGION_DWORDS * 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LocalResourceCopyRegion {
    pub(crate) dst_resource_id: u32,
    pub(crate) dst_x: u32,
    pub(crate) dst_y: u32,
    pub(crate) src_resource_id: u32,
    pub(crate) src_x: u32,
    pub(crate) src_y: u32,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LocalResourceCopyResult {
    NotApplicable,
    Invalid,
    Copied { regions: usize },
}

impl VirtioGpu3d {
    pub(crate) fn try_local_resource_copies(
        &mut self,
        mem: &dyn GuestMemoryMut,
        cmdbuf: &[u8],
    ) -> LocalResourceCopyResult {
        if cmdbuf.is_empty() || cmdbuf.len() % VIRGL_RESOURCE_COPY_REGION_BYTES != 0 {
            return LocalResourceCopyResult::NotApplicable;
        }

        let mut regions = 0usize;
        for command in cmdbuf.chunks_exact(VIRGL_RESOURCE_COPY_REGION_BYTES) {
            let Some(region) = parse_local_resource_copy_region(command) else {
                return LocalResourceCopyResult::NotApplicable;
            };
            let Some(dst_info) = self.resource_3d_info.get(&region.dst_resource_id) else {
                return LocalResourceCopyResult::NotApplicable;
            };
            let Some(src_info) = self.resource_3d_info.get(&region.src_resource_id) else {
                return LocalResourceCopyResult::NotApplicable;
            };
            let Some(dst_backing) = self.local_3d_backing.get(&region.dst_resource_id) else {
                return LocalResourceCopyResult::NotApplicable;
            };
            let Some(src_backing) = self.local_3d_backing.get(&region.src_resource_id) else {
                return LocalResourceCopyResult::NotApplicable;
            };

            let compatible = region.dst_resource_id != region.src_resource_id
                && dst_info.format == src_info.format
                && matches!(dst_info.format, 1..=4)
                && dst_info.depth == 1
                && src_info.depth == 1
                && region.width != 0
                && region.height != 0
                && region
                    .dst_x
                    .checked_add(region.width)
                    .is_some_and(|right| right <= dst_info.width)
                && region
                    .dst_y
                    .checked_add(region.height)
                    .is_some_and(|bottom| bottom <= dst_info.height)
                && region
                    .src_x
                    .checked_add(region.width)
                    .is_some_and(|right| right <= src_info.width)
                && region
                    .src_y
                    .checked_add(region.height)
                    .is_some_and(|bottom| bottom <= src_info.height)
                && backing_covers_32bpp_resource(dst_backing, *dst_info)
                && backing_covers_32bpp_resource(src_backing, *src_info);
            if !compatible {
                return LocalResourceCopyResult::Invalid;
            }
            regions = regions.saturating_add(1);
        }

        for command in cmdbuf.chunks_exact(VIRGL_RESOURCE_COPY_REGION_BYTES) {
            let region = parse_local_resource_copy_region(command)
                .expect("local copy command was validated in the first pass");
            if !self.copy_local_resource_region(mem, region) {
                return LocalResourceCopyResult::Invalid;
            }
        }
        LocalResourceCopyResult::Copied { regions }
    }

    pub(crate) fn copy_local_resource_region(
        &mut self,
        mem: &dyn GuestMemoryMut,
        region: LocalResourceCopyRegion,
    ) -> bool {
        let Some(dst_info) = self.resource_3d_info.get(&region.dst_resource_id).copied() else {
            return false;
        };
        let Some(src_info) = self.resource_3d_info.get(&region.src_resource_id).copied() else {
            return false;
        };
        let Some(dst_backing) = self.local_3d_backing.get(&region.dst_resource_id) else {
            return false;
        };
        let Some(src_backing) = self.local_3d_backing.get(&region.src_resource_id) else {
            return false;
        };
        let Some(row_bytes) = usize::try_from(region.width)
            .ok()
            .and_then(|width| width.checked_mul(4))
        else {
            return false;
        };
        self.local_copy_scratch.resize(row_bytes, 0);

        for row in 0..region.height {
            let Some(src_offset) = resource_32bpp_offset(
                src_info.width,
                region.src_x,
                region.src_y.saturating_add(row),
            ) else {
                return false;
            };
            let Some(dst_offset) = resource_32bpp_offset(
                dst_info.width,
                region.dst_x,
                region.dst_y.saturating_add(row),
            ) else {
                return false;
            };
            if !read_scattered_backing_into(
                mem,
                src_backing,
                src_offset,
                &mut self.local_copy_scratch,
            ) || !write_scattered_backing(mem, dst_backing, dst_offset, &self.local_copy_scratch)
            {
                return false;
            }
        }
        true
    }
}
