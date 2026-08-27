//! Regression for allocation-free console TX descriptor gathering.

use super::super::*;
use crate::fwcfg::GuestMemoryMut;
use std::cell::Cell;

const DESC_GPA: u64 = 0x100;
const PAYLOAD_GPA: u64 = 0x200;

struct ReadIntoOnlyMem {
    bytes: Vec<u8>,
    read_into_calls: Cell<usize>,
}

impl ReadIntoOnlyMem {
    fn new(payload: &[u8]) -> Self {
        let mut bytes = vec![0u8; 0x400];
        let desc = DESC_GPA as usize;
        bytes[desc..desc + 8].copy_from_slice(&PAYLOAD_GPA.to_le_bytes());
        bytes[desc + 8..desc + 12]
            .copy_from_slice(&u32::try_from(payload.len()).unwrap().to_le_bytes());
        let payload_start = PAYLOAD_GPA as usize;
        bytes[payload_start..payload_start + payload.len()].copy_from_slice(payload);
        Self {
            bytes,
            read_into_calls: Cell::new(0),
        }
    }
}

impl GuestMemoryMut for ReadIntoOnlyMem {
    fn write_bytes(&mut self, _gpa: u64, _data: &[u8]) -> bool {
        panic!("console TX gather must not write guest memory")
    }

    fn read_bytes(&self, _gpa: u64, _len: usize) -> Option<Vec<u8>> {
        panic!("console TX gather must not allocate through read_bytes")
    }

    fn read_into(&self, gpa: u64, dst: &mut [u8]) -> bool {
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
        self.read_into_calls
            .set(self.read_into_calls.get().saturating_add(1));
        true
    }
}

#[test]
fn read_chain_into_reads_descriptors_and_payload_without_read_bytes() {
    let payload = b"console-payload";
    let mem = ReadIntoOnlyMem::new(payload);
    let mut queue = VirtioConsoleQueue::new(0);
    queue.size = 1;
    queue.ready = true;
    queue.desc = DESC_GPA;
    let mut descs = Vec::with_capacity(1);
    let mut out = vec![0xff; payload.len()];

    assert!(VirtioConsole::read_chain_into(
        &mem,
        &queue,
        0,
        &mut descs,
        &mut out,
        payload.len(),
    ));
    assert_eq!(out, payload);
    assert_eq!(mem.read_into_calls.get(), 2);
}
