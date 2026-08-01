//! Cached virtio-gpu BAR2 base, refreshed from every vCPU.

use super::*;
use std::sync::OnceLock;

/// The one shm-map state for this process, so any vCPU can refresh the cached
/// BAR2 base without threading it through the secondary run-loop context.
static GPU_SHM_STATE: OnceLock<Arc<Mutex<HvGpuShmMapState>>> = OnceLock::new();

pub(crate) fn register(state: Arc<Mutex<HvGpuShmMapState>>) {
    let _ = GPU_SHM_STATE.set(state);
}

/// Refresh the cached BAR2 base after a PCI config-space write.
///
/// The base is cached because `HvGpuShmMapPort::map` runs with the GPU device's
/// own lock held and cannot re-enter the platform to query it. Every vCPU that
/// writes config space must call this: Windows reprograms virtio-gpu BAR2 from
/// a secondary CPU, and while only cpu0 refreshed the cache the base stayed
/// stale, so `hv_vm_map` installed the Venus ring at an address the guest had
/// stopped using. Every guest ring access then faulted into the RAZ/WI shm
/// handler and `vkCreateInstance` spun forever.
pub(crate) fn refresh_on_ecam_write(platform: &VirtPlatform, ipa: u64, is_write: bool) {
    if !is_write || machine::device_at(ipa) != Some("pcie-ecam") {
        return;
    }
    let Some(state) = GPU_SHM_STATE.get() else {
        return;
    };
    let base = platform.virtio_gpu_host_visible_bar_base();
    let mut state = state.lock().unwrap();
    state.ecam_writes = state.ecam_writes.saturating_add(1);
    if state.bar2_base != base {
        state.base_changes = state.base_changes.saturating_add(1);
        eprintln!(
            "virtio-gpu hv shm BAR2 update: ipa={ipa:#x} old={:?} new={base:?} ecam_writes={} base_changes={}",
            state.bar2_base, state.ecam_writes, state.base_changes
        );
    }
    state.bar2_base = base;
}
