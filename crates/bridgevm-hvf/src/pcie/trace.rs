//! Env-gated venus-start config-access tracing.

/// `BRIDGEVM_TRACE_VENUS_START=1`: log ECAM config-space accesses to the
/// virtio-gpu function. The venus KMD crashes before its first virtio
/// common-config access, so the PCI-config layer is the only device surface
/// that can still witness its last action. First 256 accesses then sampled.
pub(crate) fn venus_start_trace_cfg(what: &str, reg: u16, size: u8, value: u64) {
    use std::sync::atomic::{AtomicU64, Ordering};
    if !crate::virtio_gpu_trace::venus_start_trace_enabled() {
        return;
    }
    static COUNT: AtomicU64 = AtomicU64::new(0);
    let n = COUNT.fetch_add(1, Ordering::Relaxed) + 1;
    // Unsampled: config traffic is a few hundred accesses per boot and the
    // KMD's very last pre-crash access must not be sampled away.
    println!("venus-start: gpu {what} reg={reg:#x} size={size} value={value:#x} n={n}");
}
