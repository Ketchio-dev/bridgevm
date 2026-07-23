//! Split out of virtio_gpu_3d.rs to keep files under 850 lines.

use super::*;
use crate::fwcfg::GuestMemoryMut;
use crate::virtio_gpu_trace::venus_start_trace_enabled;

pub(crate) fn is_local_scanout_resource(args: Create3dArgs) -> bool {
    let display_binds = VIRGL_BIND_DISPLAY_TARGET | VIRGL_BIND_SCANOUT;
    args.target == PIPE_TEXTURE_2D
        && (1..=4).contains(&args.format)
        && args.bind & display_binds == display_binds
        && args.width > 0
        && args.height > 0
        && args.width <= MAX_LOCAL_SCANOUT_DIMENSION
        && args.height <= MAX_LOCAL_SCANOUT_DIMENSION
        && args.depth == 1
        && args.array_size == 1
        && args.last_level == 0
        && args.nr_samples == 0
}

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

/// `BRIDGEVM_TRACE_VENUS_START=1` rejection-reason lines: the JSONL command
/// trace records THAT a command failed but not WHICH validation branch fired;
/// the venus KMD start crash needs the branch.
pub(crate) fn venus_start_trace_reject(what: &str, reason: &str) {
    if venus_start_trace_enabled() {
        println!("venus-start: {what} REJECT: {reason}");
    }
}

pub(crate) fn venus_start_trace_unmap_blob_reject(resource_id: u32, reason: &str) {
    if venus_start_trace_enabled() {
        println!("venus-start: unmap_blob REJECT resource={resource_id} reason={reason}");
    }
}

pub(crate) fn venus_start_trace_map_blob_reject(
    resource_id: u32,
    shm_offset: u64,
    size: u64,
    shm_window_size: u64,
    reason: &str,
) {
    if venus_start_trace_enabled() {
        println!(
            "venus-start: map_blob REJECT resource={resource_id} shm_offset={shm_offset:#x} size={size} window={shm_window_size:#x} reason={reason}"
        );
    }
}

pub fn response_hdr(typ: u32, request: Option<CtrlHdr3d>) -> Vec<u8> {
    let mut out = Vec::with_capacity(CTRL_HDR_LEN);
    response_hdr_into(&mut out, typ, request);
    out
}

pub fn response_hdr_into(out: &mut Vec<u8>, typ: u32, request: Option<CtrlHdr3d>) {
    let (flags, fence_id, ctx_id, padding) = request.map_or((0, 0, 0, 0), |hdr| {
        (
            hdr.flags & (VIRTIO_GPU_FLAG_FENCE | VIRTIO_GPU_FLAG_INFO_RING_IDX),
            if hdr.fenced() { hdr.fence_id } else { 0 },
            hdr.ctx_id,
            hdr.padding,
        )
    });
    out.clear();
    out.reserve(CTRL_HDR_LEN);
    out.extend_from_slice(&typ.to_le_bytes());
    out.extend_from_slice(&flags.to_le_bytes());
    out.extend_from_slice(&fence_id.to_le_bytes());
    out.extend_from_slice(&ctx_id.to_le_bytes());
    out.extend_from_slice(&padding.to_le_bytes());
}

pub(crate) fn read_le_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

pub(crate) fn read_le_u64(bytes: &[u8], offset: usize) -> Option<u64> {
    Some(u64::from_le_bytes(
        bytes.get(offset..offset + 8)?.try_into().ok()?,
    ))
}

pub(crate) fn round_up_usize(value: usize, align: usize) -> usize {
    value.div_ceil(align) * align
}

pub(crate) fn aligned_u64(value: u64, align: u64) -> bool {
    value % align == 0
}

pub(crate) fn aligned_usize(value: usize, align: usize) -> bool {
    value % align == 0
}

pub(crate) fn resolve_blob_iovecs_into(
    mem: &dyn GuestMemoryMut,
    backing: &[BlobMemEntry],
    out: &mut Vec<BlobHostIovec>,
) -> bool {
    let start = out.len();
    out.reserve(backing.len());
    for entry in backing {
        let len = entry.len as usize;
        let Some(host_ptr) = mem.host_ptr(entry.addr, len) else {
            out.truncate(start);
            return false;
        };
        if host_ptr.is_null() {
            out.truncate(start);
            return false;
        }
        out.push(BlobHostIovec { host_ptr, len });
    }
    true
}

#[cfg(test)]
#[derive(Debug, Default)]
pub struct MockBackend {
    pub capset_info: Option<CapsetInfo>,
    pub capset: Vec<u8>,
    pub capset_calls: u32,
    pub created: Vec<(u32, u32, Vec<u8>)>,
    pub destroyed: Vec<u32>,
    pub attached: Vec<(u32, u32)>,
    pub detached: Vec<(u32, u32)>,
    pub created_3d: Vec<Create3dArgs>,
    pub backing_attached: Vec<(u32, usize, usize)>,
    pub backing_detached: Vec<u32>,
    pub transfers_3d: Vec<(Transfer3dArgs, bool)>,
    pub submits: Vec<(u32, Vec<u8>)>,
    pub blobs: Vec<(u32, u32, u64, u64)>,
    pub blob_iovecs: Vec<(u32, usize, usize)>,
    pub mapped: std::collections::BTreeMap<u32, MappedBlob>,
    pub unmapped: Vec<u32>,
    pub scanout_reads: Vec<(u32, u32, u32)>,
    pub scanout_blits: Vec<(u32, u32, u32)>,
    pub destroyed_resources: Vec<u32>,
    pub fences: Vec<CompletedFence>,
    pub completed: Vec<CompletedFence>,
    pub fence_polls: u64,
    pub fence_after_queue_polls: u64,
    pub reject_fence_ring: Option<u8>,
    pub reject_legacy_3d: bool,
}

#[cfg(test)]
impl MockBackend {
    pub fn new_venus() -> Self {
        let mut capset = vec![0u8; 160];
        capset[0..4].copy_from_slice(&1u32.to_le_bytes());
        Self {
            capset_info: Some(CapsetInfo {
                capset_id: 4,
                max_version: 1,
                max_size: 160,
            }),
            capset,
            ..Self::default()
        }
    }
}

#[cfg(test)]
impl VirtioGpu3dBackend for std::sync::Arc<std::sync::Mutex<MockBackend>> {
    fn capset_info(&mut self, capset_index: u32) -> Option<CapsetInfo> {
        (capset_index == 0)
            .then(|| self.lock().unwrap().capset_info)
            .flatten()
    }

    fn capset(&mut self, capset_id: u32, _version: u32) -> Option<Vec<u8>> {
        let mut inner = self.lock().unwrap();
        inner.capset_calls += 1;
        (inner.capset_info.map(|info| info.capset_id) == Some(capset_id))
            .then(|| inner.capset.clone())
    }

    fn capset_into(&mut self, capset_id: u32, _version: u32, out: &mut Vec<u8>) -> bool {
        let inner = self.lock().unwrap();
        if inner.capset_info.map(|info| info.capset_id) != Some(capset_id) {
            return false;
        }
        out.extend_from_slice(&inner.capset);
        true
    }

    fn ctx_create(&mut self, ctx_id: u32, context_init: u32, name: &[u8]) -> bool {
        self.lock()
            .unwrap()
            .created
            .push((ctx_id, context_init, name.to_vec()));
        true
    }

    fn ctx_destroy(&mut self, ctx_id: u32) {
        self.lock().unwrap().destroyed.push(ctx_id);
    }

    fn ctx_attach_resource(&mut self, ctx_id: u32, resource_id: u32) {
        self.lock().unwrap().attached.push((ctx_id, resource_id));
    }

    fn ctx_detach_resource(&mut self, ctx_id: u32, resource_id: u32) {
        self.lock().unwrap().detached.push((ctx_id, resource_id));
    }

    fn supports_legacy_3d_resources(&self) -> bool {
        !self.lock().unwrap().reject_legacy_3d
    }

    fn create_3d(&mut self, args: Create3dArgs) -> bool {
        self.lock().unwrap().created_3d.push(args);
        true
    }

    fn attach_backing(&mut self, resource_id: u32, iovecs: &[BlobHostIovec]) -> bool {
        self.lock().unwrap().backing_attached.push((
            resource_id,
            iovecs.len(),
            iovecs.iter().map(|iov| iov.len).sum(),
        ));
        true
    }

    fn detach_backing(&mut self, resource_id: u32) -> bool {
        self.lock().unwrap().backing_detached.push(resource_id);
        true
    }

    fn transfer_3d(&mut self, args: Transfer3dArgs, to_host: bool) -> bool {
        self.lock().unwrap().transfers_3d.push((args, to_host));
        true
    }

    fn submit_3d(&mut self, ctx_id: u32, cmdbuf: &[u8]) -> bool {
        self.lock().unwrap().submits.push((ctx_id, cmdbuf.to_vec()));
        true
    }

    fn create_blob(&mut self, args: CreateBlobArgs<'_>) -> bool {
        let mut inner = self.lock().unwrap();
        inner
            .blobs
            .push((args.resource_id, args.blob_mem, args.blob_id, args.size));
        inner.blob_iovecs.push((
            args.resource_id,
            args.iovecs.len(),
            args.iovecs.iter().map(|iov| iov.len).sum(),
        ));
        true
    }

    fn map_blob(&mut self, resource_id: u32) -> Option<MappedBlob> {
        self.lock().unwrap().mapped.get(&resource_id).copied()
    }

    fn unmap_blob(&mut self, resource_id: u32) {
        self.lock().unwrap().unmapped.push(resource_id);
    }

    fn scanout_map(&mut self, resource_id: u32) -> Option<ScanoutMappedBlob> {
        self.lock()
            .unwrap()
            .mapped
            .get(&resource_id)
            .map(|mapped| ScanoutMappedBlob {
                host_ptr: mapped.host_ptr.cast_const(),
                size: mapped.size,
            })
    }

    fn scanout_unmap(&mut self, resource_id: u32) {
        self.lock().unwrap().unmapped.push(resource_id);
    }

    fn scanout_read(&mut self, resource_id: u32, width: u32, height: u32, out: &mut [u8]) -> bool {
        self.lock()
            .unwrap()
            .scanout_reads
            .push((resource_id, width, height));
        for (index, byte) in out.iter_mut().enumerate() {
            *byte = index as u8;
        }
        true
    }

    fn scanout_blit_iosurface(&mut self, resource_id: u32, width: u32, height: u32) -> Option<u32> {
        self.lock()
            .unwrap()
            .scanout_blits
            .push((resource_id, width, height));
        Some(42)
    }

    fn destroy_resource(&mut self, resource_id: u32) {
        self.lock().unwrap().destroyed_resources.push(resource_id);
    }

    fn create_fence(&mut self, ctx_id: u32, ring_idx: u8, fence_id: u64) -> bool {
        let mut inner = self.lock().unwrap();
        inner.fences.push(CompletedFence {
            ctx_id,
            ring_idx,
            fence_id,
        });
        inner.reject_fence_ring != Some(ring_idx)
    }

    fn poll_fences(&mut self) {
        self.lock().unwrap().fence_polls += 1;
    }

    fn poll_fences_after_queue(&mut self) {
        self.lock().unwrap().fence_after_queue_polls += 1;
    }

    fn drain_completed_fences_into(&mut self, out: &mut Vec<CompletedFence>) {
        out.append(&mut self.lock().unwrap().completed);
    }

    fn reset(&mut self) {
        self.lock().unwrap().completed.clear();
    }
}

#[cfg(test)]
#[derive(Debug, Default)]
pub(crate) struct MockMapPort {
    pub(crate) maps: Vec<(usize, usize, u64)>,
    pub(crate) unmaps: Vec<(u64, usize)>,
}

#[cfg(test)]
impl GpuShmMapPort for std::sync::Arc<std::sync::Mutex<MockMapPort>> {
    fn map(&mut self, host_ptr: *mut u8, size: usize, shm_offset: u64) -> Result<(), i32> {
        self.lock()
            .unwrap()
            .maps
            .push((host_ptr as usize, size, shm_offset));
        Ok(())
    }

    fn unmap(&mut self, shm_offset: u64, size: usize) -> Result<(), i32> {
        self.lock().unwrap().unmaps.push((shm_offset, size));
        Ok(())
    }
}
