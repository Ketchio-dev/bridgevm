//! Fail-closed regressions for allocation-free readable descriptor gathering.

use super::super::*;
use crate::fwcfg::GuestMemoryMut;
use std::cell::Cell;

const FIRST_GPA: u64 = 0x100;
const SECOND_GPA: u64 = 0x200;
const MISSING_GPA: u64 = 0x300;

struct ReadIntoOnlyMem {
    bytes: Vec<u8>,
    read_into_calls: Cell<usize>,
}

impl ReadIntoOnlyMem {
    fn new() -> Self {
        let mut bytes = vec![0u8; 0x280];
        bytes[FIRST_GPA as usize..FIRST_GPA as usize + 4].copy_from_slice(b"head");
        bytes[SECOND_GPA as usize..SECOND_GPA as usize + 4].copy_from_slice(b"tail");
        Self {
            bytes,
            read_into_calls: Cell::new(0),
        }
    }
}

impl GuestMemoryMut for ReadIntoOnlyMem {
    fn write_bytes(&mut self, _gpa: u64, _data: &[u8]) -> bool {
        panic!("GPU request gather must not write guest memory")
    }

    fn read_bytes(&self, _gpa: u64, _len: usize) -> Option<Vec<u8>> {
        panic!("GPU request gather must not allocate through read_bytes")
    }

    fn read_into(&self, gpa: u64, dst: &mut [u8]) -> bool {
        self.read_into_calls
            .set(self.read_into_calls.get().saturating_add(1));
        let Ok(start) = usize::try_from(gpa) else {
            return false;
        };
        let Some(end) = start.checked_add(dst.len()) else {
            return false;
        };
        let Some(src) = self.bytes.get(start..end) else {
            return false;
        };
        dst.copy_from_slice(src);
        true
    }
}

fn desc(addr: u64, flags: u16) -> Descriptor {
    Descriptor {
        addr,
        len: 4,
        flags,
        next: 0,
    }
}

#[test]
fn unreadable_request_descriptor_clears_prefix_and_stops_before_suffix() {
    let mem = ReadIntoOnlyMem::new();
    let descs = [
        desc(FIRST_GPA, 0),
        desc(MISSING_GPA, 0),
        desc(SECOND_GPA, 0),
        desc(0, DESC_F_WRITE),
    ];
    let mut out = vec![0xff; 32];
    VirtioGpu::gather_readable_into(&mem, &descs, &mut out);
    assert!(out.is_empty() && mem.read_into_calls.get() == 2);
}

#[test]
fn readable_after_writable_clears_prefix_without_reading_any_suffix() {
    let mem = ReadIntoOnlyMem::new();
    let descs = [
        desc(FIRST_GPA, 0),
        desc(0, DESC_F_WRITE),
        desc(MISSING_GPA, 0),
        desc(SECOND_GPA, 0),
    ];
    let mut out = vec![0xff; 32];
    VirtioGpu::gather_readable_into(&mem, &descs, &mut out);
    assert!(out.is_empty() && mem.read_into_calls.get() == 1);
}
