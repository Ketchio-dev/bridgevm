//! Regression for allocation-free virtio-gpu readable descriptor gathering.

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

#[test]
fn gather_readable_uses_read_into_and_preserves_unbacked_skip_semantics() {
    let mem = ReadIntoOnlyMem::new();
    let descs = [
        Descriptor {
            addr: FIRST_GPA,
            len: 4,
            flags: 0,
            next: 0,
        },
        Descriptor {
            addr: 0,
            len: 64,
            flags: DESC_F_WRITE,
            next: 0,
        },
        Descriptor {
            addr: MISSING_GPA,
            len: 4,
            flags: 0,
            next: 0,
        },
        Descriptor {
            addr: SECOND_GPA,
            len: 4,
            flags: 0,
            next: 0,
        },
    ];
    let mut out = vec![0xff; 32];

    VirtioGpu::gather_readable_into(&mem, &descs, &mut out);

    assert_eq!(out, b"headtail");
    assert_eq!(mem.read_into_calls.get(), 3);
}
