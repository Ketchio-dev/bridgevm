//! BRIDGEVM_TRACE_VENUS_START stdout mirror for the KMD start path.

use super::*;
use crate::virtio_gpu_3d;
use crate::virtio_gpu_3d::VIRTIO_GPU_CMD_CTX_CREATE;
use crate::virtio_gpu_3d::VIRTIO_GPU_CMD_CTX_DESTROY;
use crate::virtio_gpu_3d::VIRTIO_GPU_CMD_GET_CAPSET;
use crate::virtio_gpu_3d::VIRTIO_GPU_CMD_GET_CAPSET_INFO;
use crate::virtio_gpu_3d::VIRTIO_GPU_CMD_RESOURCE_CREATE_BLOB;
use crate::virtio_gpu_3d::VIRTIO_GPU_CMD_RESOURCE_MAP_BLOB;
use crate::virtio_gpu_3d::VIRTIO_GPU_CMD_RESOURCE_UNMAP_BLOB;
use crate::virtio_gpu_trace::venus_start_trace_enabled;
use std::fmt::Write as _;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;

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
