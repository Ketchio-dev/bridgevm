//! Write tracking for the guest's ramfb framebuffer, so the display exporter
//! reads guest pixels only after the guest wrote them. The exporter
//! write-protects the framebuffer's pages in the stage-2 tables
//! (`hv_vm_protect`); the guest's next store there exits with a permission
//! fault, which a vCPU answers by making the pages writable again and marking
//! the frame dirty, then retries the store. An idle desktop therefore costs no
//! exits and no frame compare, and a drawing guest at most one exit per export
//! tick. `BRIDGEVM_RAMFB_DIRTY_WATCH=0` turns it off (every tick compares).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

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
    /// The protected page range `(ipa, len)`, when armed.
    range: Mutex<Option<(u64, u64)>>,
    /// The guest wrote the range since the exporter last looked.
    dirty: AtomicBool,
    /// Any range is armed: lets every other data abort skip the mutex.
    armed: AtomicBool,
    /// `hv_vm_protect` refused a range once: compare every tick from then on.
    unavailable: AtomicBool,
    protect: Protect,
}

pub(crate) static FB_WATCH: FbWatch = FbWatch::new(hv_protect);

impl FbWatch {
    pub(crate) const fn new(protect: Protect) -> Self {
        Self {
            range: Mutex::new(None),
            dirty: AtomicBool::new(false),
            armed: AtomicBool::new(false),
            unavailable: AtomicBool::new(false),
            protect,
        }
    }

    fn range(&self) -> Option<(u64, u64)> {
        *self.range.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn set_range(&self, range: Option<(u64, u64)>) {
        *self.range.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = range;
        self.armed.store(range.is_some(), Ordering::Release);
    }

    /// vCPU side, first thing on a data abort: a stage-2 write permission
    /// fault inside the watched range makes it writable again, marks the frame
    /// dirty, and returns true; the vCPU then retries the store without
    /// advancing its PC. Every other abort returns false at once.
    pub(crate) fn take_write_fault(&self, ipa: u64, esr: u64) -> bool {
        const WNR: u64 = 1 << 6;
        let permission_fault = esr & 0x3c == 0x0c;
        if !permission_fault || esr & WNR == 0 || !self.armed.load(Ordering::Acquire) {
            return false;
        }
        let Some((start, len)) = self.range() else {
            return false;
        };
        if ipa < start || ipa - start >= len {
            return false;
        }
        // Writable first, then dirty: whenever the exporter sees the range
        // writable without a dirty mark, the mark is about to land.
        (self.protect)(start, len, WRITABLE);
        self.dirty.store(true, Ordering::Release);
        true
    }

    /// Exporter side, every tick, with the framebuffer the guest programmed
    /// (`None` when inactive). Moves the watch onto that framebuffer's pages
    /// and returns whether the frame may have changed since the last call:
    /// always on a new range or without a watch, otherwise only after a guest
    /// write, write-protecting the range again before the caller reads it.
    pub(crate) fn may_have_changed(&self, framebuffer: Option<(u64, u64)>) -> bool {
        let usable = enabled() && !self.unavailable.load(Ordering::Relaxed);
        let wanted = framebuffer.filter(|_| usable).and_then(page_range);
        let current = self.range();
        if wanted != current {
            self.set_range(None);
            if let Some((start, len)) = current {
                (self.protect)(start, len, WRITABLE);
            }
            if let Some((start, len)) = wanted {
                self.dirty.store(false, Ordering::Release);
                if (self.protect)(start, len, READ_ONLY) {
                    self.set_range(wanted);
                } else {
                    (self.protect)(start, len, WRITABLE);
                    self.unavailable.store(true, Ordering::Relaxed);
                    eprintln!(
                        "ramfb display export: write tracking unavailable for \
                         {start:#x}+{len:#x}; comparing every tick"
                    );
                }
            }
            return true;
        }
        let Some((start, len)) = wanted else {
            return true;
        };
        if !self.dirty.swap(false, Ordering::AcqRel) {
            return false;
        }
        (self.protect)(start, len, READ_ONLY);
        true
    }

    /// The exporter could not publish what it read: read it again next tick.
    pub(crate) fn mark_dirty(&self) {
        self.dirty.store(true, Ordering::Release);
    }

    /// Leave guest RAM fully writable (the exporter is stopping).
    pub(crate) fn release(&self) {
        if let Some((start, len)) = self.range() {
            self.set_range(None);
            (self.protect)(start, len, WRITABLE);
        }
    }
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
