use super::super::*;
use crate::fwcfg::GuestMemoryMut;
struct Unmapped;
impl GuestMemoryMut for Unmapped {
    fn read_bytes(&self, _: u64, _: usize) -> Option<Vec<u8>> {
        None
    }
    fn write_bytes(&mut self, _: u64, _: &[u8]) -> bool {
        panic!("invalid queue must not write")
    }
}
#[test]
fn overflowing_driver_base_refuses_without_panicking() {
    let path = synthetic("driver");
    let mut dev = VirtioMmioBlock::open_read_only_modern(&path).unwrap();
    dev.queue_ready = true;
    dev.queue_num = 1;
    dev.queue_desc = 16;
    dev.queue_driver = u64::MAX;
    std::fs::remove_file(path).unwrap();
    dev.process_queue(&mut Unmapped);
    assert_eq!(dev.last_avail_idx, 0);
}
#[test]
fn overflowing_descriptor_offset_refuses_without_panicking() {
    let mut descriptors = Vec::new();
    assert!(!VirtioMmioBlock::descriptor_chain_into(
        &Unmapped,
        2,
        u64::MAX,
        1,
        &mut descriptors
    ));
    assert!(descriptors.is_empty());
}
#[test]
fn overflowing_used_ring_refuses_without_panicking() {
    let path = synthetic("used");
    let mut dev = VirtioMmioBlock::open_read_only_modern(&path).unwrap();
    dev.queue_num = 1;
    dev.queue_device = u64::MAX;
    std::fs::remove_file(path).unwrap();
    dev.write_used(&mut Unmapped, 0, 0);
}

fn synthetic(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!(
        "bridgevm-blk-overflow-{name}-{}",
        std::process::id()
    ));
    std::fs::write(&path, [0; 512]).unwrap();
    path
}
