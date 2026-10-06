//! Small synthetic media and guest-memory fixtures.

use super::super::super::tests::{temp_path, write_desc, TestMem};
use super::super::super::*;

pub(super) const TABLE: u64 = 0x1000;
pub(super) const HEADER: u64 = 0x2000;
pub(super) const SECTOR: u64 = 0x3000;
pub(super) const DATA: u64 = 0x4000;
pub(super) const STATUS: u64 = 0x5000;

pub(super) fn setup() -> (VirtioMmioBlock, TestMem) {
    static NEXT_FIXTURE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let sequence = NEXT_FIXTURE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let path = temp_path(&format!("logical-layout-{sequence}"));
    let mut media = vec![0x31; 512];
    media.extend_from_slice(&[0x72; 512]);
    std::fs::write(&path, media).unwrap();
    let mut dev = VirtioMmioBlock::open_read_only_modern(&path).unwrap();
    std::fs::remove_file(path).unwrap();
    dev.queue_num = 8;
    dev.queue_desc = TABLE;
    let mut mem = TestMem::new(0, 0x6000);
    mem.write(HEADER, &VIRTIO_BLK_T_IN.to_le_bytes());
    mem.write(HEADER + 8, &1u64.to_le_bytes());
    mem.write(SECTOR, &1u64.to_le_bytes());
    mem.write(DATA, &[0xa5; 1024]);
    mem.write(STATUS, &[0xa5]);
    (dev, mem)
}

pub(super) fn run(dev: &mut VirtioMmioBlock, mem: &mut TestMem, spans: &[(u64, u32, bool)]) -> u32 {
    for (i, &(addr, len, writable)) in spans.iter().enumerate() {
        write_desc(
            mem,
            TABLE,
            i as u16,
            Descriptor {
                addr,
                len,
                flags: if writable { DESC_F_WRITE } else { 0 }
                    | if i + 1 < spans.len() { DESC_F_NEXT } else { 0 },
                next: i as u16 + 1,
            },
        );
    }
    dev.process_descriptor_chain(mem, 0, &mut Vec::new(), &mut Vec::new())
        .written_len
}
