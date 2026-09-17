use super::*;
use std::cell::Cell;
use std::collections::BTreeMap;

struct FragmentedMem {
    regions: BTreeMap<u64, Vec<u8>>,
    reads: Cell<usize>,
}

impl FragmentedMem {
    fn new(regions: impl IntoIterator<Item = (u64, &'static [u8])>) -> Self {
        Self {
            regions: regions
                .into_iter()
                .map(|(gpa, bytes)| (gpa, bytes.to_vec()))
                .collect(),
            reads: Cell::new(0),
        }
    }
}

impl GuestMemoryMut for FragmentedMem {
    fn write_bytes(&mut self, _gpa: u64, _data: &[u8]) -> bool {
        panic!("backing reads must not write guest memory")
    }

    fn read_bytes(&self, _gpa: u64, _len: usize) -> Option<Vec<u8>> {
        panic!("backing reads must use the allocation-free read_into path")
    }

    fn read_into(&self, gpa: u64, dst: &mut [u8]) -> bool {
        self.reads.set(self.reads.get() + 1);
        self.regions
            .range(..=gpa)
            .next_back()
            .is_some_and(|(base, bytes)| {
                let Some(offset) = gpa.checked_sub(*base).and_then(|v| usize::try_from(v).ok())
                else {
                    return false;
                };
                bytes.get(offset..offset + dst.len()).is_some_and(|src| {
                    dst.copy_from_slice(src);
                    true
                })
            })
    }
}

#[test]
fn reads_one_logical_range_across_fragmented_entries() {
    let mem = FragmentedMem::new([
        (0x100, b"abcd".as_slice()),
        (0x300, b"ef".as_slice()),
        (0x500, b"ghijk".as_slice()),
    ]);
    let backing = [
        BackingEntry {
            addr: 0x100,
            len: 4,
        },
        BackingEntry {
            addr: 0x300,
            len: 2,
        },
        BackingEntry {
            addr: 0x500,
            len: 5,
        },
    ];
    let mut dst = [0; 7];

    assert!(read_from_backing_into(&mem, &backing, 2, &mut dst));
    assert_eq!(&dst, b"cdefghi");
    assert_eq!(mem.reads.get(), 3);
}

#[test]
fn rejects_out_of_range_before_touching_guest_memory() {
    let mem = FragmentedMem::new([(0x100, b"abcd".as_slice())]);
    let backing = [BackingEntry {
        addr: 0x100,
        len: 4,
    }];
    let mut dst = [0xaa; 3];

    assert!(!read_from_backing_into(&mem, &backing, 2, &mut dst));
    assert_eq!(dst, [0xaa; 3]);
    assert_eq!(mem.reads.get(), 0);
}

#[test]
fn fails_closed_when_a_fragment_is_unreadable() {
    let mem = FragmentedMem::new([(0x100, b"head".as_slice())]);
    let backing = [
        BackingEntry {
            addr: 0x100,
            len: 4,
        },
        BackingEntry {
            addr: 0x300,
            len: 4,
        },
    ];
    let mut dst = [0; 8];

    assert!(!read_from_backing_into(&mem, &backing, 0, &mut dst));
    assert_eq!(mem.reads.get(), 2);
}

#[test]
fn rejects_overflowing_guest_address() {
    let mem = FragmentedMem::new([]);
    let backing = [BackingEntry {
        addr: u64::MAX,
        len: 2,
    }];
    let mut dst = [0; 1];

    assert!(!read_from_backing_into(&mem, &backing, 1, &mut dst));
    assert_eq!(mem.reads.get(), 0);
}
