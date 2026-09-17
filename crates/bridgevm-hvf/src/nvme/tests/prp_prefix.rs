use super::super::*;
use crate::fwcfg::GuestMemoryMut;
use std::cell::RefCell;

const MEMORY_BASE: u64 = 0x4000_0000;
const FIRST_LIST: u64 = MEMORY_BASE;
const SECOND_LIST: u64 = MEMORY_BASE + PAGE_SIZE_U64;
const FIRST_DATA: u64 = 0x8000_0000;

struct TrackingMemory {
    bytes: Vec<u8>,
    reads: RefCell<Vec<usize>>,
}

impl TrackingMemory {
    fn new(pages: usize) -> Self {
        Self {
            bytes: vec![0; pages * PAGE_SIZE],
            reads: RefCell::new(Vec::new()),
        }
    }

    fn write_entry(&mut self, list: u64, index: usize, value: u64) {
        let start = (list - MEMORY_BASE) as usize + index * 8;
        self.bytes[start..start + 8].copy_from_slice(&value.to_le_bytes());
    }
}

impl GuestMemoryMut for TrackingMemory {
    fn write_bytes(&mut self, gpa: u64, data: &[u8]) -> bool {
        let start = (gpa - MEMORY_BASE) as usize;
        let Some(end) = start.checked_add(data.len()) else {
            return false;
        };
        let Some(dst) = self.bytes.get_mut(start..end) else {
            return false;
        };
        dst.copy_from_slice(data);
        true
    }

    fn read_bytes(&self, gpa: u64, len: usize) -> Option<Vec<u8>> {
        let start = (gpa - MEMORY_BASE) as usize;
        self.bytes
            .get(start..start.checked_add(len)?)
            .map(<[u8]>::to_vec)
    }

    fn read_into(&self, gpa: u64, dst: &mut [u8]) -> bool {
        let start = (gpa - MEMORY_BASE) as usize;
        let Some(source) = self.bytes.get(start..start.saturating_add(dst.len())) else {
            return false;
        };
        dst.copy_from_slice(source);
        self.reads.borrow_mut().push(dst.len());
        true
    }
}

fn command(prp2: u64) -> SubmissionEntry {
    SubmissionEntry {
        opcode: NVM_OP_READ,
        command_id: 1,
        nsid: 1,
        prp1: FIRST_DATA,
        prp2,
        cdw10: 0,
        cdw11: 0,
        cdw12: 0,
        cdw13: 0,
        cdw14: 0,
        cdw15: 0,
    }
}

#[test]
fn common_128k_prp_list_reads_only_its_31_entries() {
    let mut mem = TrackingMemory::new(1);
    for index in 0..31 {
        mem.write_entry(
            FIRST_LIST,
            index,
            FIRST_DATA + (index as u64 + 1) * PAGE_SIZE_U64,
        );
    }
    let mut spans = Vec::new();
    let mut scratch = [0; PAGE_SIZE];
    assert!(prp_spans_into(
        &command(FIRST_LIST),
        128 * 1024,
        &mem,
        &mut spans,
        &mut scratch,
    ));
    assert_eq!(&*mem.reads.borrow(), &[31 * 8]);
    assert_eq!(spans.len(), 32);
    assert_eq!(spans[0], (FIRST_DATA, PAGE_SIZE));
    assert_eq!(spans[31], (FIRST_DATA + 31 * PAGE_SIZE_U64, PAGE_SIZE));
}

#[test]
fn chained_prp_list_reads_the_chain_page_and_only_the_second_prefix() {
    let mut mem = TrackingMemory::new(2);
    for index in 0..511 {
        mem.write_entry(
            FIRST_LIST,
            index,
            FIRST_DATA + (index as u64 + 1) * PAGE_SIZE_U64,
        );
    }
    mem.write_entry(FIRST_LIST, 511, SECOND_LIST);
    for index in 0..2 {
        mem.write_entry(
            SECOND_LIST,
            index,
            FIRST_DATA + (512 + index as u64) * PAGE_SIZE_U64,
        );
    }
    let mut spans = Vec::new();
    let mut scratch = [0; PAGE_SIZE];
    assert!(prp_spans_into(
        &command(FIRST_LIST),
        514 * PAGE_SIZE,
        &mem,
        &mut spans,
        &mut scratch,
    ));
    assert_eq!(&*mem.reads.borrow(), &[PAGE_SIZE, 2 * 8]);
    assert_eq!(spans.len(), 514);
    assert_eq!(
        spans.last(),
        Some(&(FIRST_DATA + 513 * PAGE_SIZE_U64, PAGE_SIZE))
    );
}

#[test]
fn short_list_scratch_fails_without_retaining_partial_spans() {
    let mut mem = TrackingMemory::new(1);
    for index in 0..31 {
        mem.write_entry(
            FIRST_LIST,
            index,
            FIRST_DATA + (index as u64 + 1) * PAGE_SIZE_U64,
        );
    }
    let sentinel = (0x1000, 7);
    let mut spans = vec![sentinel];
    let mut short = [0; 247];
    assert!(!prp_spans_into(
        &command(FIRST_LIST),
        128 * 1024,
        &mem,
        &mut spans,
        &mut short,
    ));
    assert_eq!(spans, vec![sentinel]);
    assert!(mem.reads.borrow().is_empty());
}
