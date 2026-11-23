//! Guest RAM mappings and GPU shared-memory mapping.

use crate::*;
use bridgevm_hvf::nvme::NvmeIoWorker;

/// A GuestMemoryMut view over the actual HVF-mapped guest RAM, so fw_cfg DMA
/// reads/writes hit real firmware memory (not a throwaway buffer).
pub(crate) struct MappedRam {
    pub(crate) base: u64,
    pub(crate) ptr: *mut u8,
    pub(crate) len: usize,
}

/// vCPU `index`'s NVMe transfer thread, over its own view of the guest RAM at
/// `ram`/`len` (bridgevm_hvf::nvme::NvmeIoWorker; the thread starts with the
/// first unlocked transfer). Declare it after the RAM backing: dropping it
/// joins the thread.
pub(crate) fn vcpu_nvme_io_worker(index: u64, ram: usize, len: usize) -> NvmeIoWorker {
    NvmeIoWorker::new(format!("nvme-io-{index}"), move || MappedRam {
        base: machine::RAM_BASE,
        ptr: ram as *mut u8,
        len,
    })
}

/// `BRIDGEVM_TRACE_VENUS_START=1`: flag EC=0x24 exits whose syndrome has
/// ISV=0 (no valid instruction syndrome — stp/ldp, DC ZVA, NEON, atomics).
/// SRT/size are meaningless for these exits. The run loops refuse them before
/// device effects or register writeback; this trace retains the rejected exit.
/// It does not establish the cause of a past guest failure.
pub(crate) fn trace_isv0_data_abort(esr: u64, pc: u64, ipa: u64) {
    use std::sync::atomic::{AtomicU64, Ordering};
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    let enabled = *ENABLED.get_or_init(|| {
        std::env::var("BRIDGEVM_TRACE_VENUS_START")
            .ok()
            .is_some_and(|v| matches!(v.trim(), "1" | "true" | "yes" | "on"))
    });
    if !enabled || (esr >> 24) & 1 == 1 {
        return;
    }
    static COUNT: AtomicU64 = AtomicU64::new(0);
    let n = COUNT.fetch_add(1, Ordering::Relaxed) + 1;
    if n <= 16 || n % 65536 == 0 {
        println!(
            "venus-start: ISV=0 data abort n={n} pc={pc:#x} ipa={ipa:#x} esr={esr:#x} (MMIO emulation refused)"
        );
    }
}

#[derive(Debug, Default)]
pub(crate) struct HvGpuShmMapState {
    pub(crate) bar2_base: Option<u64>,
    pub(crate) ecam_writes: u64,
    pub(crate) base_changes: u64,
}

#[derive(Debug, Clone)]
pub(crate) struct HvGpuShmMapPort {
    pub(crate) state: Arc<Mutex<HvGpuShmMapState>>,
}

impl GpuShmMapPort for HvGpuShmMapPort {
    fn map(&mut self, host_ptr: *mut u8, size: usize, shm_offset: u64) -> Result<(), i32> {
        let state = self.state.lock().unwrap();
        let Some(bar2_base) = state.bar2_base else {
            eprintln!(
                "virtio-gpu hv shm map: BAR2 unassigned offset={shm_offset:#x} size={size:#x} ecam_writes={} base_changes={}",
                state.ecam_writes, state.base_changes
            );
            return Err(-12);
        };
        drop(state);
        if (host_ptr as usize) % 0x4000 != 0 || (size % 0x4000) != 0 || shm_offset % 0x4000 != 0 {
            eprintln!(
                "virtio-gpu hv shm map: unaligned host_ptr={host_ptr:p} offset={shm_offset:#x} size={size:#x}"
            );
            return Err(-22);
        }
        let guest_pa = bar2_base.checked_add(shm_offset).ok_or(-12)?;
        // Pre-touch every page with a WRITE before hv_vm_map: fresh anonymous
        // host memory is backed by the shared zero page until first write, and
        // hv_vm_map pins whatever physical page backs the VA at map time. A
        // later HOST write would COW onto a new page the guest never sees —
        // the guest then reads stale zeros forever (host-written venus fence
        // feedback slots and GPU results were invisible to the guest).
        for off in (0..size).step_by(0x4000) {
            unsafe {
                let p = host_ptr.add(off);
                p.write_volatile(p.read_volatile());
            }
        }
        let ret = unsafe {
            hv_vm_map(
                host_ptr.cast::<c_void>(),
                guest_pa,
                size,
                HV_MEMORY_READ | HV_MEMORY_WRITE,
            )
        };
        eprintln!(
            "virtio-gpu hv shm map: offset={shm_offset:#x} size={size:#x} host_ptr={host_ptr:p} guest_pa={guest_pa:#x} ret={ret:#x}"
        );
        (ret == 0).then_some(()).ok_or(ret as i32)
    }
    fn unmap(&mut self, shm_offset: u64, size: usize) -> Result<(), i32> {
        let Some(bar2_base) = self.state.lock().unwrap().bar2_base else {
            eprintln!(
                "virtio-gpu hv shm unmap: BAR2 unassigned offset={shm_offset:#x} size={size:#x}"
            );
            return Err(-12);
        };
        let guest_pa = bar2_base.checked_add(shm_offset).ok_or(-12)?;
        let ret = unsafe { hv_vm_unmap(guest_pa, size) };
        eprintln!(
            "virtio-gpu hv shm unmap: offset={shm_offset:#x} size={size:#x} guest_pa={guest_pa:#x} ret={ret:#x}"
        );
        (ret == 0).then_some(()).ok_or(ret as i32)
    }
}

pub(crate) fn install_gpu_shm_port(platform: &mut VirtPlatform) {
    let hv_gpu_shm_state = Arc::new(Mutex::new(HvGpuShmMapState::default()));
    let installed_hv_gpu_shm_port =
        platform.set_virtio_gpu_shm_map_port(Box::new(HvGpuShmMapPort {
            state: Arc::clone(&hv_gpu_shm_state),
        }));
    register_gpu_shm_state(hv_gpu_shm_state);
    if installed_hv_gpu_shm_port {
        println!("virtio-gpu host-visible shm map port: hv_vm_map enabled");
    }
}

/// The one shm-map state for this process, so any vCPU can refresh the cached
/// BAR2 base without threading it through the secondary run-loop context.
static GPU_SHM_STATE: OnceLock<Arc<Mutex<HvGpuShmMapState>>> = OnceLock::new();

fn register_gpu_shm_state(state: Arc<Mutex<HvGpuShmMapState>>) {
    let _ = GPU_SHM_STATE.set(state);
}

/// Refresh the cached BAR2 base after a PCI config-space write.
///
/// The base is cached because `HvGpuShmMapPort::map` runs with the GPU device's
/// own lock held and cannot re-enter the platform to query it. Every vCPU that
/// writes config space must call this: Windows reprograms virtio-gpu BAR2 from
/// a secondary CPU, and with a stale base `hv_vm_map` installs the Venus ring
/// at an address the guest stopped using. Every guest ring access then faults
/// into the RAZ/WI shm handler and `vkCreateInstance` spins forever.
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

impl GuestMemoryMut for MappedRam {
    fn write_bytes(&mut self, gpa: u64, data: &[u8]) -> bool {
        let Some(off) = gpa
            .checked_sub(self.base)
            .and_then(|value| usize::try_from(value).ok())
        else {
            return false;
        };
        let Some(end) = off.checked_add(data.len()) else {
            return false;
        };
        if end > self.len {
            return false;
        }
        // SAFETY: Category 10/11 - `off..end` was checked to stay inside the
        // live HVF RAM mapping, so `ptr.add(off)` is in-bounds for `data.len()`.
        unsafe { std::ptr::copy_nonoverlapping(data.as_ptr(), self.ptr.add(off), data.len()) };
        true
    }
    fn read_bytes(&self, gpa: u64, len: usize) -> Option<Vec<u8>> {
        let off = gpa
            .checked_sub(self.base)
            .and_then(|value| usize::try_from(value).ok())?;
        let end = off.checked_add(len)?;
        if end > self.len {
            return None;
        }
        let mut v = vec![0u8; len];
        // SAFETY: Category 10/11 - `off..end` was checked to stay inside the
        // live HVF RAM mapping, so copying `len` bytes from `ptr.add(off)` is in-bounds.
        unsafe { std::ptr::copy_nonoverlapping(self.ptr.add(off), v.as_mut_ptr(), len) };
        Some(v)
    }
    fn read_into(&self, gpa: u64, dst: &mut [u8]) -> bool {
        let Some(off) = gpa
            .checked_sub(self.base)
            .and_then(|value| usize::try_from(value).ok())
        else {
            return false;
        };
        let Some(end) = off.checked_add(dst.len()) else {
            return false;
        };
        if end > self.len {
            return false;
        }
        // SAFETY: Category 10/11 - `off..end` was checked to stay inside the
        // live HVF RAM mapping, so copying `dst.len()` bytes from `ptr.add(off)`
        // is in-bounds.
        unsafe { std::ptr::copy_nonoverlapping(self.ptr.add(off), dst.as_mut_ptr(), dst.len()) };
        true
    }
    fn host_ptr(&self, gpa: u64, len: usize) -> Option<*mut u8> {
        let off = gpa
            .checked_sub(self.base)
            .and_then(|value| usize::try_from(value).ok())?;
        let end = off.checked_add(len)?;
        if end > self.len {
            return None;
        }
        // SAFETY: Category 10/11 - `off..end` was checked to stay inside the
        // fixed live HVF RAM mapping. The returned pointer is valid for the
        // guest RAM mapping lifetime; virglrenderer drops it on resource unref.
        Some(unsafe { self.ptr.add(off) })
    }
}

pub(crate) fn reset_guest_ram_for_boot(guest_ram: &mut MappedRam, dtb: &[u8]) {
    assert!(dtb.len() < guest_ram.len, "DTB must fit in guest RAM");
    // SAFETY: the mapped RAM view carries the live allocation pointer and its
    // exact allocation length for the whole probe lifetime.
    unsafe { guest_ram_backing::zero_mapped_memory(guest_ram.ptr, guest_ram.len) };
    assert!(
        guest_ram.write_bytes(machine::RAM_BASE, dtb),
        "copy DTB to guest RAM base"
    );
}

#[cfg(test)]
mod mapped_ram_tests {
    use super::*;

    #[test]
    fn mapped_ram_rejects_ranges_that_overflow_host_offset() {
        let mut bytes = [0u8; 16];
        let mut ram = MappedRam {
            base: 0,
            ptr: bytes.as_mut_ptr(),
            len: bytes.len(),
        };
        let overflowing_gpa = usize::MAX as u64;

        assert_eq!(ram.read_bytes(overflowing_gpa, 2), None);
        assert!(!ram.write_bytes(overflowing_gpa, &[1, 2]));
    }

    #[test]
    fn reset_guest_ram_for_boot_zeroes_ram_and_places_dtb_at_base() {
        let mut bytes = [0xa5u8; 16];
        let mut ram = MappedRam {
            base: machine::RAM_BASE,
            ptr: bytes.as_mut_ptr(),
            len: bytes.len(),
        };
        let dtb = [1, 2, 3, 4];

        reset_guest_ram_for_boot(&mut ram, &dtb);

        assert_eq!(&bytes[..dtb.len()], dtb.as_slice());
        assert!(bytes[dtb.len()..].iter().all(|byte| *byte == 0));
    }
}
