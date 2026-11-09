use crate::fwcfg::GuestMemoryMut;
use std::sync::atomic::{fence, AtomicBool, AtomicU16, Ordering::Acquire, Ordering::Relaxed};
use std::sync::Arc;
use std::time::{Duration, Instant};

struct FailingWrite {
    failure: usize,
    writes: Vec<u64>,
}
impl GuestMemoryMut for FailingWrite {
    fn read_bytes(&self, _: u64, _: usize) -> Option<Vec<u8>> {
        None
    }
    fn read_into(&self, _: u64, bytes: &mut [u8]) -> bool {
        bytes.fill(0);
        true
    }
    fn write_bytes(&mut self, address: u64, _: &[u8]) -> bool {
        self.writes.push(address);
        self.writes.len() != self.failure
    }
}
#[test]
fn failed_used_element_write_does_not_publish_the_index() {
    for failure in [1, 2] {
        let mut mem = FailingWrite {
            failure,
            writes: Vec::new(),
        };
        super::super::address::write_used(&mut mem, 0x1000, 1, 7, 4);
        assert!(
            !mem.writes.contains(&0x1002),
            "failed element must not publish used index"
        );
        assert_eq!(mem.writes.len(), failure);
    }
}
#[test]
fn successful_used_element_is_published_before_its_index() {
    let mut mem = FailingWrite {
        failure: usize::MAX,
        writes: Vec::new(),
    };
    super::super::address::write_used(&mut mem, 0x1000, 1, 7, 4);
    assert_eq!(mem.writes, [0x1004, 0x1008, 0x1002]);
}

/// Halfword cells with relaxed accesses: the plain loads and stores of a
/// mapped guest page, without a data race in the test itself.
#[derive(Clone)]
struct SharedRam(Arc<Vec<AtomicU16>>);

impl SharedRam {
    fn new(len: usize) -> Self {
        Self(Arc::new((0..len / 2).map(|_| AtomicU16::new(0)).collect()))
    }

    fn cells(&self, gpa: u64, len: usize) -> Option<&[AtomicU16]> {
        let start = usize::try_from(gpa).ok()?;
        (start % 2 == 0 && len % 2 == 0).then_some(())?;
        self.0.get(start / 2..(start + len) / 2)
    }

    fn get(&self, gpa: u64) -> u16 {
        self.0[gpa as usize / 2].load(Relaxed)
    }
}

impl GuestMemoryMut for SharedRam {
    // Out of line, like the live mapping behind `dyn GuestMemoryMut`: the
    // accesses leave the compiler in program order, so any reordering the
    // other side sees is the CPU's.
    #[inline(never)]
    fn write_bytes(&mut self, gpa: u64, data: &[u8]) -> bool {
        let Some(cells) = self.cells(gpa, data.len()) else {
            return false;
        };
        for (cell, half) in cells.iter().zip(data.chunks_exact(2)) {
            cell.store(u16::from_le_bytes([half[0], half[1]]), Relaxed);
        }
        true
    }

    #[inline(never)]
    fn read_bytes(&self, gpa: u64, len: usize) -> Option<Vec<u8>> {
        let cells = self.cells(gpa, len)?;
        Some(
            cells
                .iter()
                .flat_map(|c| c.load(Relaxed).to_le_bytes())
                .collect(),
        )
    }
}

/// The used ring as a guest driver on another core sees it. A driver that
/// polls the ring (netkvm with interrupts off) reads `used.idx`, then, after
/// its read barrier, the element; Arm reorders the device's stores, so without
/// a barrier between them the index can arrive first and the driver takes the
/// previous lap's element (live: netkvm then walked a cyclic descriptor chain
/// on vCPU0 until Windows reset itself, mid-download). Fails within
/// milliseconds without the barrier in a release build; in a debug build the
/// stores are too far apart to overtake each other.
#[test]
fn a_used_index_is_never_visible_before_its_element() {
    const USED: u64 = 64;
    const SIZE: u16 = 256;
    // How far the device may run ahead of the last index the driver checked.
    // The element the driver reads is then at most LEAD newer than the
    // index it saw, far inside the i16 range that tells newer from older, even
    // when the driver is descheduled between the two loads.
    const LEAD: u16 = 1024;
    let ram = SharedRam::new(USED as usize + 4 + 8 * usize::from(SIZE));
    let stop = Arc::new(AtomicBool::new(false));
    let acked = Arc::new(AtomicU16::new(0));
    let device = {
        let (mut ram, stop, acked) = (ram.clone(), Arc::clone(&stop), Arc::clone(&acked));
        std::thread::spawn(move || {
            // Each element names the index it is published under, so an
            // element of an earlier lap is recognisable.
            let mut next = 0u16;
            while !stop.load(Relaxed) {
                if next.wrapping_sub(acked.load(Relaxed)) >= LEAD {
                    std::thread::yield_now();
                    continue;
                }
                super::super::address::write_used(&mut ram, USED, SIZE, next, u32::from(next));
                next = next.wrapping_add(1);
            }
        })
    };
    let (mut seen, mut checks, mut stale) = (0u16, 0u64, 0u64);
    let deadline = Instant::now() + Duration::from_secs(1);
    while Instant::now() < deadline && stale == 0 {
        let idx = ram.get(USED + 2);
        if idx == seen {
            continue;
        }
        seen = idx;
        fence(Acquire); // the driver's virtio_rmb
        let published = idx.wrapping_sub(1);
        let id = ram.get(USED + 4 + u64::from(published % SIZE) * 8);
        // Newer is fine (the device lapped the driver); older is stale.
        stale += u64::from((id.wrapping_sub(published) as i16) < 0);
        checks += 1;
        acked.store(idx, Relaxed);
    }
    stop.store(true, Relaxed);
    device.join().unwrap();
    assert_eq!(
        stale, 0,
        "a used index was visible before its element ({checks} checks)"
    );
    assert!(
        checks > 1000,
        "the driver saw too few publications ({checks})"
    );
}
