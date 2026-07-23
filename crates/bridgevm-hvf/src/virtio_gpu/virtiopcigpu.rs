//! Split out of virtio_gpu.rs to keep files under 850 lines.

use super::*;
use crate::fwcfg::GuestMemoryMut;
use crate::msix::MsixTable;
use crate::pcie::VIRTIO_GPU_MSIX_VECTOR_COUNT;
use crate::virtio_gpu_3d;
use crate::virtio_gpu_3d::GpuShmMapPort;
use crate::virtio_gpu_3d::VirtioGpu3dBackend;
use crate::virtio_gpu_3d::VIRTIO_GPU_CMD_CTX_ATTACH_RESOURCE;
use crate::virtio_gpu_3d::VIRTIO_GPU_CMD_CTX_CREATE;
use crate::virtio_gpu_3d::VIRTIO_GPU_CMD_CTX_DESTROY;
use crate::virtio_gpu_3d::VIRTIO_GPU_CMD_CTX_DETACH_RESOURCE;
use crate::virtio_gpu_3d::VIRTIO_GPU_CMD_GET_CAPSET;
use crate::virtio_gpu_3d::VIRTIO_GPU_CMD_GET_CAPSET_INFO;
use crate::virtio_gpu_3d::VIRTIO_GPU_CMD_RESOURCE_CREATE_3D;
use crate::virtio_gpu_3d::VIRTIO_GPU_CMD_RESOURCE_CREATE_BLOB;
use crate::virtio_gpu_3d::VIRTIO_GPU_CMD_RESOURCE_MAP_BLOB;
use crate::virtio_gpu_3d::VIRTIO_GPU_CMD_RESOURCE_UNMAP_BLOB;
use crate::virtio_gpu_3d::VIRTIO_GPU_CMD_SUBMIT_3D;
use crate::virtio_gpu_3d::VIRTIO_GPU_CMD_TRANSFER_FROM_HOST_3D;
use crate::virtio_gpu_3d::VIRTIO_GPU_CMD_TRANSFER_TO_HOST_3D;
use crate::virtio_gpu_3d::VIRTIO_GPU_FLAG_FENCE;
use crate::virtio_gpu_trace::venus_start_trace_enabled;
use crate::virtio_gpu_trace::write_json_string;
use std::fmt::Write as _;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug)]
pub struct VirtioPciGpu {
    pub(crate) gpu: VirtioGpu,
    pub(crate) msix: MsixTable,
}

impl VirtioPciGpu {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            gpu: VirtioGpu::new(width, height),
            msix: MsixTable::new(VIRTIO_GPU_MSIX_VECTOR_COUNT),
        }
    }

    pub fn with_3d_backend(width: u32, height: u32, backend: Box<dyn VirtioGpu3dBackend>) -> Self {
        Self {
            gpu: VirtioGpu::with_3d_backend(width, height, backend),
            msix: MsixTable::new(VIRTIO_GPU_MSIX_VECTOR_COUNT),
        }
    }

    pub fn with_3d_backend_and_shm_map_port(
        width: u32,
        height: u32,
        backend: Box<dyn VirtioGpu3dBackend>,
        map_port: Box<dyn GpuShmMapPort>,
        shm_window_size: u64,
    ) -> Self {
        let mut gpu = VirtioGpu::with_3d_backend(width, height, backend);
        gpu.set_shm_map_port(map_port, shm_window_size);
        Self {
            gpu,
            msix: MsixTable::new(VIRTIO_GPU_MSIX_VECTOR_COUNT),
        }
    }

    pub fn set_shm_map_port(&mut self, port: Box<dyn GpuShmMapPort>, window_size: u64) {
        self.gpu.set_shm_map_port(port, window_size);
    }

    pub fn set_vblank_interval(&mut self, interval: Duration) {
        self.gpu.set_vblank_interval(interval);
    }

    /// Host-driven scanout resize. Returns true when the geometry changed and a
    /// DISPLAY event + config-change interrupt were armed; the caller flushes
    /// the resulting MSI-X via `drain_pending_msix_into`.
    pub fn request_display_resolution(&mut self, width: u32, height: u32) -> bool {
        self.gpu.request_display_resolution(width, height)
    }

    /// Current reported scanout geometry.
    pub fn display_resolution(&self) -> (u32, u32) {
        (self.gpu.width, self.gpu.height)
    }

    pub fn set_vblank_wake(&mut self, wake: Arc<VblankWakeState>) {
        self.gpu.set_vblank_wake(wake);
    }

    pub fn vblank_wake(&self) -> Option<Arc<VblankWakeState>> {
        self.gpu.vblank_wake()
    }

    pub fn set_3d_scanout_readback_interval(&mut self, interval: Duration) {
        self.gpu.set_3d_scanout_readback_interval(interval);
    }

    pub fn set_3d_scanout_deferred(&mut self, deferred: bool) {
        self.gpu.set_3d_scanout_deferred(deferred);
    }

    pub fn service_deferred_3d_scanout(&mut self) {
        self.gpu.service_deferred_3d_scanout();
    }

    pub fn set_3d_scanout_iosurface(&mut self, enabled: bool, verify: bool) {
        self.gpu.set_3d_scanout_iosurface(enabled, verify);
    }

    pub fn new_from_env() -> Self {
        let (width, height) = parse_resolution_env();
        Self::new(width, height)
    }

    pub fn stats(&self) -> VirtioGpuStats {
        self.gpu.stats()
    }

    pub fn interrupt_line_level(&self) -> bool {
        self.gpu.interrupt_line_level()
    }
}

pub(crate) fn common_cfg_offset(offset: u64) -> Option<u64> {
    (PCI_COMMON_CFG_OFFSET..PCI_COMMON_CFG_OFFSET + PCI_CFG_REGION_SIZE)
        .contains(&offset)
        .then_some(offset - PCI_COMMON_CFG_OFFSET)
}

pub(crate) fn device_cfg_offset(offset: u64) -> Option<u64> {
    (PCI_DEVICE_CFG_OFFSET..PCI_DEVICE_CFG_OFFSET + PCI_CFG_REGION_SIZE)
        .contains(&offset)
        .then_some(offset - PCI_DEVICE_CFG_OFFSET)
}

pub(crate) fn notify_queue_index(offset: u64) -> Option<u16> {
    let rel = offset.checked_sub(PCI_NOTIFY_CFG_OFFSET)?;
    (rel < PCI_CFG_REGION_SIZE).then_some((rel / 4) as u16)
}

pub(crate) fn queue_bit(index: usize) -> Option<u8> {
    (index < u8::BITS as usize).then(|| 1u8 << index)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Descriptor {
    pub(crate) addr: u64,
    pub(crate) len: u32,
    pub(crate) flags: u16,
    pub(crate) next: u16,
}

impl Descriptor {
    pub(crate) fn read(mem: &dyn GuestMemoryMut, gpa: u64) -> Option<Self> {
        let mut bytes = [0u8; DESC_SIZE as usize];
        if !mem.read_into(gpa, &mut bytes) {
            return None;
        }
        Some(Self {
            addr: u64::from_le_bytes(bytes[0..8].try_into().unwrap()),
            len: u32::from_le_bytes(bytes[8..12].try_into().unwrap()),
            flags: u16::from_le_bytes(bytes[12..14].try_into().unwrap()),
            next: u16::from_le_bytes(bytes[14..16].try_into().unwrap()),
        })
    }
}

impl CtrlHdr {
    pub(crate) fn parse(bytes: &[u8]) -> Option<Self> {
        Some(Self {
            typ: read_le_u32(bytes, 0)?,
            flags: read_le_u32(bytes, 4)?,
            fence_id: read_le_u64(bytes, 8)?,
            ctx_id: read_le_u32(bytes, 16)?,
            padding: read_le_u32(bytes, 20)?,
        })
    }

    pub(crate) fn response(self, typ: u32) -> Self {
        Self {
            typ,
            flags: self.flags & VIRTIO_GPU_FLAG_FENCE,
            fence_id: if self.flags & VIRTIO_GPU_FLAG_FENCE != 0 {
                self.fence_id
            } else {
                0
            },
            ctx_id: self.ctx_id,
            padding: self.padding,
        }
    }

    pub(crate) fn ring_idx(self) -> u8 {
        if self.flags & virtio_gpu_3d::VIRTIO_GPU_FLAG_INFO_RING_IDX != 0 {
            (self.padding & 0xff) as u8
        } else {
            0
        }
    }

    pub(crate) fn append_to(self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.typ.to_le_bytes());
        out.extend_from_slice(&self.flags.to_le_bytes());
        out.extend_from_slice(&self.fence_id.to_le_bytes());
        out.extend_from_slice(&self.ctx_id.to_le_bytes());
        out.extend_from_slice(&self.padding.to_le_bytes());
    }
}

pub(crate) fn response_hdr_into(out: &mut Vec<u8>, typ: u32, request: Option<CtrlHdr>) {
    let hdr = request.map_or(
        CtrlHdr {
            typ,
            flags: 0,
            fence_id: 0,
            ctx_id: 0,
            padding: 0,
        },
        |hdr| hdr.response(typ),
    );
    out.clear();
    out.reserve(24);
    hdr.append_to(out);
}

/// Commands whose backend call can leave rendering or transfer work in flight.
/// Every other command is synchronous and may complete its virtqueue fence as
/// soon as the call returns, even when it was routed through the 3D backend.
pub(crate) fn command_requires_backend_fence(typ: u32) -> bool {
    matches!(
        typ,
        VIRTIO_GPU_CMD_TRANSFER_TO_HOST_3D
            | VIRTIO_GPU_CMD_TRANSFER_FROM_HOST_3D
            | VIRTIO_GPU_CMD_SUBMIT_3D
    )
}

pub(crate) fn command_name(typ: u32) -> &'static str {
    match typ {
        VIRTIO_GPU_CMD_GET_DISPLAY_INFO => "GET_DISPLAY_INFO",
        VIRTIO_GPU_CMD_RESOURCE_CREATE_2D => "RESOURCE_CREATE_2D",
        VIRTIO_GPU_CMD_RESOURCE_UNREF => "RESOURCE_UNREF",
        VIRTIO_GPU_CMD_SET_SCANOUT => "SET_SCANOUT",
        VIRTIO_GPU_CMD_RESOURCE_FLUSH => "RESOURCE_FLUSH",
        VIRTIO_GPU_CMD_TRANSFER_TO_HOST_2D => "TRANSFER_TO_HOST_2D",
        VIRTIO_GPU_CMD_RESOURCE_ATTACH_BACKING => "RESOURCE_ATTACH_BACKING",
        VIRTIO_GPU_CMD_RESOURCE_DETACH_BACKING => "RESOURCE_DETACH_BACKING",
        VIRTIO_GPU_CMD_GET_CAPSET_INFO => "GET_CAPSET_INFO",
        VIRTIO_GPU_CMD_GET_CAPSET => "GET_CAPSET",
        VIRTIO_GPU_CMD_GET_EDID => "GET_EDID",
        VIRTIO_GPU_CMD_RESOURCE_CREATE_BLOB => "RESOURCE_CREATE_BLOB",
        VIRTIO_GPU_CMD_SET_SCANOUT_BLOB => "SET_SCANOUT_BLOB",
        VIRTIO_GPU_CMD_CTX_CREATE => "CTX_CREATE",
        VIRTIO_GPU_CMD_CTX_DESTROY => "CTX_DESTROY",
        VIRTIO_GPU_CMD_CTX_ATTACH_RESOURCE => "CTX_ATTACH_RESOURCE",
        VIRTIO_GPU_CMD_CTX_DETACH_RESOURCE => "CTX_DETACH_RESOURCE",
        VIRTIO_GPU_CMD_RESOURCE_CREATE_3D => "RESOURCE_CREATE_3D",
        VIRTIO_GPU_CMD_TRANSFER_TO_HOST_3D => "TRANSFER_TO_HOST_3D",
        VIRTIO_GPU_CMD_TRANSFER_FROM_HOST_3D => "TRANSFER_FROM_HOST_3D",
        VIRTIO_GPU_CMD_SUBMIT_3D => "SUBMIT_3D",
        VIRTIO_GPU_CMD_RESOURCE_MAP_BLOB => "RESOURCE_MAP_BLOB",
        VIRTIO_GPU_CMD_RESOURCE_UNMAP_BLOB => "RESOURCE_UNMAP_BLOB",
        VIRTIO_GPU_CMD_UPDATE_CURSOR => "UPDATE_CURSOR",
        VIRTIO_GPU_CMD_MOVE_CURSOR => "MOVE_CURSOR",
        _ => "UNKNOWN",
    }
}

pub(crate) fn trace_sample(count: u64) -> bool {
    count <= 64 || count % 1024 == 0
}

pub(crate) fn venus_start_trace_msix(what: &str, vector: u16, enabled: bool, masked: bool) {
    if !venus_start_trace_enabled() {
        return;
    }
    static COUNT: AtomicU64 = AtomicU64::new(0);
    let n = COUNT.fetch_add(1, Ordering::Relaxed) + 1;
    if trace_sample(n) {
        println!(
            "venus-start: msix {what} vector={vector} fn_enabled={enabled} fn_masked={masked} n={n}"
        );
    }
}

pub(crate) fn venus_start_trace_msix_queue(what: &str, queue_index: usize, vector: u16) {
    if !venus_start_trace_enabled() {
        return;
    }
    static COUNT: AtomicU64 = AtomicU64::new(0);
    let n = COUNT.fetch_add(1, Ordering::Relaxed) + 1;
    if trace_sample(n) {
        println!("venus-start: msix queue={queue_index} {what} vector={vector} n={n}");
    }
}

/// Stdout mirror of the command trace for the venus KMD start path
/// (`BRIDGEVM_TRACE_VENUS_START=1`). Capset/blob/context lifecycle commands
/// and every error response print unconditionally — those are exactly the
/// accesses DxgkDdiStartDevice makes before the crash — while the high-rate
/// steady-state commands (SUBMIT_3D NOPs, transfers, flushes) are sampled.
pub(crate) fn venus_start_trace_command(request: &[u8], hdr: CtrlHdr, response: &[u8]) {
    if !venus_start_trace_enabled() {
        return;
    }
    let response_type = read_le_u32(response, 0).unwrap_or(0);
    let is_error = response_type >= VIRTIO_GPU_RESP_ERR_UNSPEC;
    let always = is_error
        || matches!(
            hdr.typ,
            VIRTIO_GPU_CMD_GET_CAPSET_INFO
                | VIRTIO_GPU_CMD_GET_CAPSET
                | VIRTIO_GPU_CMD_RESOURCE_CREATE_BLOB
                | VIRTIO_GPU_CMD_RESOURCE_MAP_BLOB
                | VIRTIO_GPU_CMD_RESOURCE_UNMAP_BLOB
                | VIRTIO_GPU_CMD_CTX_CREATE
                | VIRTIO_GPU_CMD_CTX_DESTROY
        );
    if !always {
        static COUNT: AtomicU64 = AtomicU64::new(0);
        let n = COUNT.fetch_add(1, Ordering::Relaxed) + 1;
        if !trace_sample(n) {
            return;
        }
    }
    let mut line = format!(
        "venus-start: cmd {} typ={:#x} ctx={} flags={:#x} -> {} typ={:#x}",
        command_name(hdr.typ),
        hdr.typ,
        hdr.ctx_id,
        hdr.flags,
        response_name(response_type),
        response_type
    );
    match hdr.typ {
        VIRTIO_GPU_CMD_GET_CAPSET_INFO => {
            let _ = write!(
                line,
                " capset_index={}",
                read_le_u32(request, 24).unwrap_or(u32::MAX)
            );
            if response_type == virtio_gpu_3d::VIRTIO_GPU_RESP_OK_CAPSET_INFO {
                let _ = write!(
                    line,
                    " capset_id={} max_version={} max_size={}",
                    read_le_u32(response, 24).unwrap_or(0),
                    read_le_u32(response, 28).unwrap_or(0),
                    read_le_u32(response, 32).unwrap_or(0)
                );
            }
        }
        VIRTIO_GPU_CMD_GET_CAPSET => {
            let _ = write!(
                line,
                " capset_id={} version={} response_bytes={}",
                read_le_u32(request, 24).unwrap_or(0),
                read_le_u32(request, 28).unwrap_or(0),
                response.len().saturating_sub(24)
            );
        }
        VIRTIO_GPU_CMD_RESOURCE_CREATE_BLOB => {
            let _ = write!(
                line,
                " resource_id={} blob_mem={} blob_flags={:#x} blob_id={} size={} nr_entries={}",
                read_le_u32(request, 24).unwrap_or(0),
                read_le_u32(request, 28).unwrap_or(0),
                read_le_u32(request, 32).unwrap_or(0),
                read_le_u64(request, 40).unwrap_or(0),
                read_le_u64(request, 48).unwrap_or(0),
                read_le_u32(request, 36).unwrap_or(0)
            );
        }
        VIRTIO_GPU_CMD_RESOURCE_MAP_BLOB => {
            let _ = write!(
                line,
                " resource_id={} shm_offset={:#x}",
                read_le_u32(request, 24).unwrap_or(0),
                read_le_u64(request, 32).unwrap_or(0)
            );
            if response_type == virtio_gpu_3d::VIRTIO_GPU_RESP_OK_MAP_INFO {
                let _ = write!(
                    line,
                    " map_info={:#x}",
                    read_le_u32(response, 24).unwrap_or(0)
                );
            }
        }
        VIRTIO_GPU_CMD_RESOURCE_UNMAP_BLOB => {
            let _ = write!(
                line,
                " resource_id={}",
                read_le_u32(request, 24).unwrap_or(0)
            );
        }
        _ => {}
    }
    println!("{line}");
}

pub(crate) fn response_name(typ: u32) -> &'static str {
    match typ {
        VIRTIO_GPU_RESP_OK_NODATA => "OK_NODATA",
        VIRTIO_GPU_RESP_OK_DISPLAY_INFO => "OK_DISPLAY_INFO",
        virtio_gpu_3d::VIRTIO_GPU_RESP_OK_CAPSET_INFO => "OK_CAPSET_INFO",
        virtio_gpu_3d::VIRTIO_GPU_RESP_OK_CAPSET => "OK_CAPSET",
        VIRTIO_GPU_RESP_OK_EDID => "OK_EDID",
        virtio_gpu_3d::VIRTIO_GPU_RESP_OK_MAP_INFO => "OK_MAP_INFO",
        VIRTIO_GPU_RESP_ERR_UNSPEC => "ERR_UNSPEC",
        virtio_gpu_3d::VIRTIO_GPU_RESP_ERR_OUT_OF_MEMORY => "ERR_OUT_OF_MEMORY",
        virtio_gpu_3d::VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER => "ERR_INVALID_PARAMETER",
        _ => "UNKNOWN",
    }
}

pub(crate) fn write_trace_command_details(out: &mut String, request: &[u8], hdr: CtrlHdr) {
    match hdr.typ {
        VIRTIO_GPU_CMD_RESOURCE_CREATE_2D => {
            let _ = write!(
                out,
                ",\"resource_id\":{},\"format\":{},\"width\":{},\"height\":{}",
                read_le_u32(request, 24).unwrap_or(0),
                read_le_u32(request, 28).unwrap_or(0),
                read_le_u32(request, 32).unwrap_or(0),
                read_le_u32(request, 36).unwrap_or(0)
            );
        }
        VIRTIO_GPU_CMD_RESOURCE_UNREF
        | VIRTIO_GPU_CMD_RESOURCE_DETACH_BACKING
        | VIRTIO_GPU_CMD_RESOURCE_UNMAP_BLOB => {
            let _ = write!(
                out,
                ",\"resource_id\":{}",
                read_le_u32(request, 24).unwrap_or(0)
            );
        }
        VIRTIO_GPU_CMD_RESOURCE_ATTACH_BACKING => {
            let _ = write!(
                out,
                ",\"resource_id\":{},\"nr_entries\":{}",
                read_le_u32(request, 24).unwrap_or(0),
                read_le_u32(request, 28).unwrap_or(0)
            );
        }
        VIRTIO_GPU_CMD_SET_SCANOUT => {
            let rect = read_rect(request, 24).unwrap_or(Rect {
                x: 0,
                y: 0,
                width: 0,
                height: 0,
            });
            let _ = write!(
                out,
                ",\"scanout_id\":{},\"resource_id\":{},\"rect_x\":{},\"rect_y\":{},\"rect_w\":{},\"rect_h\":{}",
                read_le_u32(request, 40).unwrap_or(u32::MAX),
                read_le_u32(request, 44).unwrap_or(0),
                rect.x,
                rect.y,
                rect.width,
                rect.height
            );
        }
        VIRTIO_GPU_CMD_RESOURCE_FLUSH => {
            let rect = read_rect(request, 24).unwrap_or(Rect {
                x: 0,
                y: 0,
                width: 0,
                height: 0,
            });
            let _ = write!(
                out,
                ",\"resource_id\":{},\"rect_x\":{},\"rect_y\":{},\"rect_w\":{},\"rect_h\":{}",
                read_le_u32(request, 40).unwrap_or(0),
                rect.x,
                rect.y,
                rect.width,
                rect.height
            );
        }
        VIRTIO_GPU_CMD_TRANSFER_TO_HOST_2D => {
            let rect = read_rect(request, 24).unwrap_or(Rect {
                x: 0,
                y: 0,
                width: 0,
                height: 0,
            });
            let _ = write!(
                out,
                ",\"resource_id\":{},\"offset\":{},\"rect_x\":{},\"rect_y\":{},\"rect_w\":{},\"rect_h\":{}",
                read_le_u32(request, 48).unwrap_or(0),
                read_le_u64(request, 40).unwrap_or(0),
                rect.x,
                rect.y,
                rect.width,
                rect.height
            );
        }
        VIRTIO_GPU_CMD_GET_CAPSET_INFO => {
            let _ = write!(
                out,
                ",\"capset_index\":{}",
                read_le_u32(request, 24).unwrap_or(u32::MAX)
            );
        }
        VIRTIO_GPU_CMD_GET_CAPSET => {
            let _ = write!(
                out,
                ",\"capset_id\":{},\"capset_version\":{}",
                read_le_u32(request, 24).unwrap_or(0),
                read_le_u32(request, 28).unwrap_or(0)
            );
        }
        VIRTIO_GPU_CMD_RESOURCE_CREATE_BLOB => {
            let _ = write!(
                out,
                ",\"resource_id\":{},\"blob_mem\":{},\"blob_flags\":{},\"nr_entries\":{},\"blob_id\":{},\"blob_size\":{}",
                read_le_u32(request, 24).unwrap_or(0),
                read_le_u32(request, 28).unwrap_or(0),
                read_le_u32(request, 32).unwrap_or(0),
                read_le_u32(request, 36).unwrap_or(0),
                read_le_u64(request, 40).unwrap_or(0),
                read_le_u64(request, 48).unwrap_or(0)
            );
        }
        VIRTIO_GPU_CMD_SET_SCANOUT_BLOB => {
            let rect = read_rect(request, 24).unwrap_or(Rect {
                x: 0,
                y: 0,
                width: 0,
                height: 0,
            });
            let _ = write!(
                out,
                ",\"scanout_id\":{},\"resource_id\":{},\"width\":{},\"height\":{},\"format\":{},\"stride0\":{},\"offset0\":{},\"rect_x\":{},\"rect_y\":{},\"rect_w\":{},\"rect_h\":{}",
                read_le_u32(request, 40).unwrap_or(u32::MAX),
                read_le_u32(request, 44).unwrap_or(0),
                read_le_u32(request, 48).unwrap_or(0),
                read_le_u32(request, 52).unwrap_or(0),
                read_le_u32(request, 56).unwrap_or(0),
                read_le_u32(request, 64).unwrap_or(0),
                read_le_u32(request, 80).unwrap_or(0),
                rect.x,
                rect.y,
                rect.width,
                rect.height
            );
        }
        VIRTIO_GPU_CMD_CTX_CREATE => {
            let nlen = read_le_u32(request, 24).unwrap_or(0).min(64) as usize;
            let name_end = 32usize.saturating_add(nlen).min(request.len());
            let _ = write!(
                out,
                ",\"context_init\":{},\"name_len\":{},\"debug_name\":",
                read_le_u32(request, 28).unwrap_or(0),
                nlen
            );
            let name = request
                .get(32..name_end)
                .map(String::from_utf8_lossy)
                .unwrap_or_default();
            write_json_string(out, name.as_ref());
        }
        VIRTIO_GPU_CMD_CTX_ATTACH_RESOURCE | VIRTIO_GPU_CMD_CTX_DETACH_RESOURCE => {
            let _ = write!(
                out,
                ",\"resource_id\":{}",
                read_le_u32(request, 24).unwrap_or(0)
            );
        }
        VIRTIO_GPU_CMD_RESOURCE_CREATE_3D => {
            let _ = write!(
                out,
                ",\"resource_id\":{},\"target\":{},\"format\":{},\"bind\":{},\"width\":{},\"height\":{},\"depth\":{},\"array_size\":{},\"last_level\":{},\"nr_samples\":{},\"resource_flags\":{}",
                read_le_u32(request, 24).unwrap_or(0),
                read_le_u32(request, 28).unwrap_or(0),
                read_le_u32(request, 32).unwrap_or(0),
                read_le_u32(request, 36).unwrap_or(0),
                read_le_u32(request, 40).unwrap_or(0),
                read_le_u32(request, 44).unwrap_or(0),
                read_le_u32(request, 48).unwrap_or(0),
                read_le_u32(request, 52).unwrap_or(0),
                read_le_u32(request, 56).unwrap_or(0),
                read_le_u32(request, 60).unwrap_or(0),
                read_le_u32(request, 64).unwrap_or(0)
            );
        }
        VIRTIO_GPU_CMD_TRANSFER_TO_HOST_3D | VIRTIO_GPU_CMD_TRANSFER_FROM_HOST_3D => {
            let _ = write!(
                out,
                ",\"resource_id\":{},\"level\":{},\"stride\":{},\"layer_stride\":{},\"transfer_offset\":{},\"box_x\":{},\"box_y\":{},\"box_z\":{},\"box_w\":{},\"box_h\":{},\"box_d\":{}",
                read_le_u32(request, 56).unwrap_or(0),
                read_le_u32(request, 60).unwrap_or(0),
                read_le_u32(request, 64).unwrap_or(0),
                read_le_u32(request, 68).unwrap_or(0),
                read_le_u64(request, 48).unwrap_or(0),
                read_le_u32(request, 24).unwrap_or(0),
                read_le_u32(request, 28).unwrap_or(0),
                read_le_u32(request, 32).unwrap_or(0),
                read_le_u32(request, 36).unwrap_or(0),
                read_le_u32(request, 40).unwrap_or(0),
                read_le_u32(request, 44).unwrap_or(0)
            );
        }
        VIRTIO_GPU_CMD_SUBMIT_3D => {
            let size = read_le_u32(request, 24).unwrap_or(0) as usize;
            let payload_start = 32usize.min(request.len());
            let payload_end = payload_start.saturating_add(size).min(request.len());
            let payload = request.get(payload_start..payload_end).unwrap_or(&[]);
            let _ = write!(
                out,
                ",\"submit_size\":{},\"submit_dwords\":{},\"submit_prefix_hex\":",
                size,
                size.div_ceil(4)
            );
            write_hex_prefix_json(out, payload, submit_trace_prefix_len());
        }
        VIRTIO_GPU_CMD_RESOURCE_MAP_BLOB => {
            let _ = write!(
                out,
                ",\"resource_id\":{},\"shm_offset\":{}",
                read_le_u32(request, 24).unwrap_or(0),
                read_le_u64(request, 32).unwrap_or(0)
            );
        }
        _ => {}
    }
}
