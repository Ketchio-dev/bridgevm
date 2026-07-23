//! Split out of platform_virt.rs to keep files under 850 lines.

use crate::fwcfg::GuestMemoryMut;
use std::time::Duration;
use std::time::Instant;

/// A flat span of guest RAM implementing [`GuestMemoryMut`]. In live use the run
/// loop supplies a view over the HVF-mapped guest memory instead; this is the
/// host-side stand-in used for tests and offline pipeline exercises.
#[derive(Debug)]
pub struct FlatGuestRam {
    pub(crate) base: u64,
    pub(crate) bytes: Vec<u8>,
}

impl FlatGuestRam {
    pub fn new(base: u64, len: usize) -> Self {
        Self {
            base,
            bytes: vec![0u8; len],
        }
    }
    pub(crate) fn offset(&self, gpa: u64) -> Option<usize> {
        gpa.checked_sub(self.base)
            .and_then(|value| usize::try_from(value).ok())
    }
}

impl GuestMemoryMut for FlatGuestRam {
    fn write_bytes(&mut self, gpa: u64, data: &[u8]) -> bool {
        let Some(start) = self.offset(gpa) else {
            return false;
        };
        let Some(end) = start.checked_add(data.len()) else {
            return false;
        };
        if end > self.bytes.len() {
            return false;
        }
        self.bytes[start..end].copy_from_slice(data);
        true
    }
    fn read_bytes(&self, gpa: u64, len: usize) -> Option<Vec<u8>> {
        let start = self.offset(gpa)?;
        let end = start.checked_add(len)?;
        if end > self.bytes.len() {
            return None;
        }
        Some(self.bytes[start..end].to_vec())
    }

    fn read_into(&self, gpa: u64, dst: &mut [u8]) -> bool {
        let Some(start) = self.offset(gpa) else {
            return false;
        };
        let Some(end) = start.checked_add(dst.len()) else {
            return false;
        };
        if end > self.bytes.len() {
            return false;
        }
        dst.copy_from_slice(&self.bytes[start..end]);
        true
    }

    fn host_ptr(&self, gpa: u64, len: usize) -> Option<*mut u8> {
        let start = self.offset(gpa)?;
        let end = start.checked_add(len)?;
        if end > self.bytes.len() {
            return None;
        }
        Some(self.bytes.as_ptr().wrapping_add(start) as *mut u8)
    }
}

/// Report-pacing decision. A zero interval or a not-yet-emitted endpoint always
/// permits the next report; otherwise the caller must wait until `interval` has
/// elapsed since `last_emission`. Kept as a free function so the gate is unit
/// tested deterministically with synthetic `Instant`s.
pub(crate) fn report_pacing_allows_emission(
    interval: Duration,
    last_emission: Option<Instant>,
    now: Instant,
) -> bool {
    if interval.is_zero() {
        return true;
    }
    match last_emission {
        None => true,
        Some(last) => now.saturating_duration_since(last) >= interval,
    }
}
