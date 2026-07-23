//! Guest RAM mappings and GPU shared-memory mapping.

use crate::*;

/// A GuestMemoryMut view over the actual HVF-mapped guest RAM, so fw_cfg DMA
/// reads/writes hit real firmware memory (not a throwaway buffer).
pub(crate) struct MappedRam {
    pub(crate) base: u64,
    pub(crate) ptr: *mut u8,
    pub(crate) len: usize,
}

/// `BRIDGEVM_TRACE_VENUS_START=1`: flag EC=0x24 exits whose syndrome has
/// ISV=0 (no valid instruction syndrome — stp/ldp, DC ZVA, NEON, atomics).
/// The srt/size fields the MMIO decode uses below are meaningless for these,
/// so a read writes back to a bogus register (typically X0) — silent guest
/// state corruption. The venus KMD dies with no bugcheck before its first
/// virtio access; an ISV=0 access into a device window is a prime suspect.
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
            "venus-start: ISV=0 data abort n={n} pc={pc:#x} ipa={ipa:#x} esr={esr:#x} (decode below is garbage)"
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
    unsafe {
        // SAFETY: Category 10/11 - `guest_ram.ptr` points to the live RAM mapping
        // with `guest_ram.len` bytes allocated by the probe before this helper runs.
        std::ptr::write_bytes(guest_ram.ptr, 0, guest_ram.len);
    }
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
