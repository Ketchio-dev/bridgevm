//! BRIDGEVM_TRACE_VENUS_START rejection-reason trace helpers.

use crate::virtio_gpu_trace::venus_start_trace_enabled;

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
