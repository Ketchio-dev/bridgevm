use super::*;
use std::cell::Cell;

struct TestMem {
    bytes: Vec<u8>,
    reads: Cell<usize>,
}

impl GuestMemoryMut for TestMem {
    fn write_bytes(&mut self, _gpa: u64, _data: &[u8]) -> bool {
        false
    }

    fn read_bytes(&self, _gpa: u64, _len: usize) -> Option<Vec<u8>> {
        panic!("cursor reads must use read_into")
    }

    fn read_into(&self, gpa: u64, dst: &mut [u8]) -> bool {
        self.reads.set(self.reads.get() + 1);
        usize::try_from(gpa)
            .ok()
            .and_then(|start| self.bytes.get(start..start + dst.len()))
            .is_some_and(|src| {
                dst.copy_from_slice(src);
                true
            })
    }
}

fn entries() -> [BackingEntry; 3] {
    [
        BackingEntry { addr: 0, len: 4 },
        BackingEntry { addr: 4, len: 4 },
        BackingEntry { addr: 8, len: 4 },
    ]
}

#[test]
fn reads_forward_then_rewinds_with_one_cursor() {
    let mem = TestMem {
        bytes: b"abcdefghijkl".to_vec(),
        reads: Cell::new(0),
    };
    let mut cursor = BackingReadCursor::default();
    let mut forward = [0; 5];
    let mut rewind = [0; 3];

    assert!(read_from_backing_into_from(
        &mem,
        &entries(),
        5,
        &mut forward,
        &mut cursor
    ));
    assert_eq!(&forward, b"fghij");
    assert!(read_from_backing_into_from(
        &mem,
        &entries(),
        1,
        &mut rewind,
        &mut cursor
    ));
    assert_eq!(&rewind, b"bcd");
}

#[test]
fn rejects_uncovered_range_before_reading_memory() {
    let mem = TestMem {
        bytes: b"abcdefghijkl".to_vec(),
        reads: Cell::new(0),
    };
    let mut cursor = BackingReadCursor::default();
    let mut dst = [0xaa; 4];

    assert!(!read_from_backing_into_from(
        &mem,
        &entries(),
        10,
        &mut dst,
        &mut cursor
    ));
    assert_eq!(dst, [0xaa; 4]);
    assert_eq!(mem.reads.get(), 0);
}
