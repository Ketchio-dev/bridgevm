//! A completion's phase tag is the last thing the guest can see of it.
//!
//! A guest that polls its completion queue takes an entry as soon as its phase tag flips, then reads
//! the rest of the entry and the command's data. Apple silicon lets another core see stores out of
//! order, so the dword holding the phase is written last, after a release fence that orders every
//! earlier store (the data, the rest of the entry) before it.

use super::super::*;
use super::helpers::*;
use crate::fwcfg::GuestMemoryMut;

/// Guest RAM that records each store's address and length.
struct Recording {
    inner: FakeMem,
    writes: Vec<(u64, usize)>,
}

impl GuestMemoryMut for Recording {
    fn write_bytes(&mut self, gpa: u64, data: &[u8]) -> bool {
        self.writes.push((gpa, data.len()));
        self.inner.write_bytes(gpa, data)
    }

    fn read_bytes(&self, gpa: u64, len: usize) -> Option<Vec<u8>> {
        self.inner.read_bytes(gpa, len)
    }

    fn read_into(&self, gpa: u64, dst: &mut [u8]) -> bool {
        self.inner.read_into(gpa, dst)
    }
}

#[test]
fn the_phase_dword_is_stored_after_the_data_and_the_rest_of_the_entry() {
    let (mut ctrl, mem) = enabled_controller();
    let mut mem = Recording {
        inner: mem,
        writes: Vec::new(),
    };
    // Identify controller: 4 KiB of data, then the completion.
    let sqe = encode_sqe(ADMIN_OP_IDENTIFY, 7, 0, DATA_BASE, 1, 0, 0);
    assert!(mem.inner.write_bytes(ASQ_BASE, &sqe));
    ctrl.mmio_write(REG_DOORBELL_BASE, 4, 1);
    ctrl.process(&mut mem);
    let entry = read_completion(&mem.inner, ACQ_BASE, 0);
    assert_eq!(completion_status(&entry), SC_SUCCESS);

    let in_entry = |&(gpa, len): &(u64, usize)| {
        gpa >= ACQ_BASE && gpa + len as u64 <= ACQ_BASE + CQ_ENTRY_SIZE
    };
    let last = *mem.writes.last().expect("stores");
    assert_eq!(
        last,
        (ACQ_BASE + 12, 4),
        "the phase dword is not the last store: {:?}",
        mem.writes
    );
    let first_part = mem.writes.iter().position(in_entry).expect("entry stores");
    assert_eq!(
        mem.writes[first_part],
        (ACQ_BASE, 12),
        "the rest of the entry first: {:?}",
        mem.writes
    );
    assert!(
        mem.writes[..first_part]
            .iter()
            .any(|&(gpa, _)| gpa == DATA_BASE),
        "the data comes before the entry: {:?}",
        mem.writes
    );
}
