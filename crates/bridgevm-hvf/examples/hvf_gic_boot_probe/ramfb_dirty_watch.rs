//! Write tracking for the guest's ramfb framebuffer, so the display exporter
//! reads guest pixels only after the guest wrote them. The exporter
//! write-protects the framebuffer's pages in the stage-2 tables
//! (`hv_vm_protect`); the guest's next store there exits with a permission
//! fault, which a vCPU answers by making the pages writable again and marking
//! the frame dirty, then retries the store. An idle desktop therefore costs no
//! exits and no frame compare, and a drawing guest at most one exit per export
//! tick. `BRIDGEVM_RAMFB_DIRTY_WATCH=0` turns it off (every tick compares).
//!
//! The guest chooses the framebuffer address, so the watch protects a range
//! only when the guest RAM the exporter reads backs every page of it; any
//! other range is never protected, and every tick compares. Pages are made
//! writable again only in ranges the watch itself protected.
//!
//! One lock covers each protection change together with the state that
//! records it, on the exporter and the vCPU side, so a vCPU always sees the
//! watch state that matches the protection. A store can still fault while its
//! page is protected and reach its vCPU's handler only after the exporter made
//! the page writable and moved or dropped the watch: every write permission
//! fault inside the pages the watch ever protected is therefore retried, never
//! emulated as MMIO.

use std::sync::{Mutex, MutexGuard, OnceLock, PoisonError};

use bridgevm_hvf::fwcfg::GuestMemoryMut;

use crate::hvf_abi::{HV_MEMORY_EXEC, HV_MEMORY_READ, HV_MEMORY_WRITE};

/// Stage-2 protection granule: Apple silicon hosts map 16 KiB pages.
const PAGE: u64 = 16 << 10;
const WRITABLE: u64 = HV_MEMORY_READ | HV_MEMORY_WRITE | HV_MEMORY_EXEC;
const READ_ONLY: u64 = HV_MEMORY_READ | HV_MEMORY_EXEC;

/// Sets stage-2 permissions on a page-aligned guest range; true on success.
pub(crate) type Protect = fn(u64, u64, u64) -> bool;

fn hv_protect(ipa: u64, len: u64, flags: u64) -> bool {
    let Ok(size) = usize::try_from(len) else {
        return false;
    };
    // SAFETY: the range lies inside guest RAM, which stays mapped for the VM's
    // lifetime; changing its permissions affects only guest accesses.
    unsafe { crate::hvf_abi::hv_vm_protect(ipa, size, flags) == 0 }
}

pub(crate) struct FbWatch {
    state: Mutex<State>,
    protect: Protect,
}

struct State {
    /// The protected page range `(ipa, len)`, when armed.
    armed: Option<(u64, u64)>,
    /// Bounds `[lo, hi)` of every range the watch ever protected, all of them
    /// in guest RAM.
    ever_protected: Option<(u64, u64)>,
    /// The guest wrote the armed range since the exporter last looked.
    dirty: bool,
    /// `hv_vm_protect` refused a range once: compare every tick from then on.
    unavailable: bool,
}

pub(crate) static FB_WATCH: FbWatch = FbWatch::new(hv_protect);

impl FbWatch {
    pub(crate) const fn new(protect: Protect) -> Self {
        Self {
            state: Mutex::new(State {
                armed: None,
                ever_protected: None,
                dirty: false,
                unavailable: false,
            }),
            protect,
        }
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// vCPU side, first thing on a data abort: a stage-2 write permission
    /// fault inside the watched range makes it writable again, marks the frame
    /// dirty, and returns true; the vCPU then retries the store without
    /// advancing its PC. A write permission fault elsewhere in the pages the
    /// watch ever protected is retried too. Every other abort returns false,
    /// and every non-permission fault (all MMIO exits) without taking the lock.
    pub(crate) fn take_write_fault(&self, ipa: u64, esr: u64) -> bool {
        const WNR: u64 = 1 << 6;
        if esr & 0x3c != 0x0c || esr & WNR == 0 {
            return false;
        }
        let mut state = self.state();
        if !state.ever_protected.is_some_and(|(lo, hi)| (lo..hi).contains(&ipa)) {
            return false;
        }
        let watched = |&(start, len): &(u64, u64)| (start..start + len).contains(&ipa);
        if let Some((start, len)) = state.armed.filter(watched) {
            (self.protect)(start, len, WRITABLE);
            state.dirty = true;
        }
        // Otherwise it faulted before the exporter made the page writable
        // again and moved or dropped the watch: the retry goes through.
        true
    }

    /// Exporter side, every tick, with the framebuffer the guest programmed
    /// (`None` when inactive). Moves the watch onto that framebuffer's pages
    /// and returns whether the frame may have changed since the last call:
    /// always on a new range or without a watch, otherwise only after a guest
    /// write, write-protecting the range again before the caller reads it.
    /// `ram` is the guest memory the caller reads the frame from.
    pub(crate) fn may_have_changed(
        &self,
        framebuffer: Option<(u64, u64)>,
        ram: &dyn GuestMemoryMut,
    ) -> bool {
        let mut state = self.state();
        let usable = enabled() && !state.unavailable;
        let wanted = framebuffer.filter(|_| usable).and_then(page_range);
        if wanted != state.armed {
            if let Some((start, len)) = state.armed.take() {
                (self.protect)(start, len, WRITABLE);
            }
            if let Some((start, len)) = wanted {
                let size = usize::try_from(len).ok();
                if size.and_then(|size| ram.host_ptr(start, size)).is_none() {
                    give_up(&mut state, (start, len), "framebuffer outside guest RAM");
                    return true;
                }
                let (lo, hi) = state.ever_protected.unwrap_or((start, start + len));
                state.ever_protected = Some((lo.min(start), hi.max(start + len)));
                state.dirty = false;
                self.protect_or_give_up(&mut state, (start, len));
            }
            return true;
        }
        let Some(range) = wanted else {
            return true;
        };
        if !std::mem::take(&mut state.dirty) {
            return false;
        }
        self.protect_or_give_up(&mut state, range);
        true
    }

    /// Write-protect `range` and arm the watch on it. If `hv_vm_protect`
    /// refuses, leave the range writable and compare every tick from now on.
    fn protect_or_give_up(&self, state: &mut State, (start, len): (u64, u64)) {
        if (self.protect)(start, len, READ_ONLY) {
            state.armed = Some((start, len));
        } else {
            (self.protect)(start, len, WRITABLE);
            give_up(state, (start, len), "hv_vm_protect refused");
        }
    }

    /// The exporter could not publish what it read: read it again next tick.
    pub(crate) fn mark_dirty(&self) {
        self.state().dirty = true;
    }

    /// Leave guest RAM fully writable (the exporter is stopping).
    pub(crate) fn release(&self) {
        let mut state = self.state();
        if let Some((start, len)) = state.armed.take() {
            (self.protect)(start, len, WRITABLE);
        }
    }
}

/// Stop tracking for good: every tick compares from now on.
fn give_up(state: &mut State, (start, len): (u64, u64), reason: &str) {
    state.armed = None;
    state.unavailable = true;
    eprintln!(
        "ramfb display export: write tracking unavailable for {start:#x}+{len:#x} \
         ({reason}); comparing every tick"
    );
}

/// The stage-2 pages covering `(gpa, len)`.
fn page_range((gpa, len): (u64, u64)) -> Option<(u64, u64)> {
    if len == 0 {
        return None;
    }
    let start = gpa & !(PAGE - 1);
    let end = gpa.checked_add(len)?.checked_add(PAGE - 1)? & !(PAGE - 1);
    Some((start, end - start))
}

fn enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var("BRIDGEVM_RAMFB_DIRTY_WATCH").as_deref() != Ok("0"))
}

#[cfg(test)]
#[path = "ramfb_dirty_watch_tests.rs"]
mod tests;
