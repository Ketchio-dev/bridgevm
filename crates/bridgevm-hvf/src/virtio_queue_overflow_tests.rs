use crate::{
    fwcfg::GuestMemoryMut,
    virtio_console::VirtioConsole,
    virtio_gpu::VirtioGpu,
    virtio_net::{LoopbackTestBackend, VirtioNet},
};
struct Unmapped;
impl GuestMemoryMut for Unmapped {
    fn write_bytes(&mut self, _: u64, _: &[u8]) -> bool {
        false
    }
    fn read_bytes(&self, _: u64, _: usize) -> Option<Vec<u8>> {
        None
    }
}
#[test]
fn net_overflowing_driver_base_refuses_without_panicking() {
    let mut dev = VirtioNet::new(LoopbackTestBackend::default());
    dev.queues[1].ready = true;
    dev.queues[1].size = 1;
    dev.queues[1].desc = 16;
    dev.queues[1].driver = u64::MAX;
    dev.process_tx_queue(&mut Unmapped);
    assert_eq!(dev.queues[1].last_avail_idx, 0);
}
#[test]
fn console_overflowing_driver_base_refuses_without_panicking() {
    let mut dev = VirtioConsole::new();
    dev.queues[3].ready = true;
    dev.queues[3].size = 1;
    dev.queues[3].desc = 16;
    dev.queues[3].driver = u64::MAX;
    dev.poll(&mut Unmapped);
    assert_eq!(dev.queues[3].last_avail_idx, 0);
}
#[test]
fn gpu_overflowing_driver_base_refuses_without_panicking() {
    let mut dev = VirtioGpu::new(1, 1);
    dev.queues[0].ready = true;
    dev.queues[0].size = 1;
    dev.queues[0].desc = 16;
    dev.queues[0].driver = u64::MAX;
    dev.process_control_queue(&mut Unmapped);
    assert_eq!(dev.queues[0].last_avail_idx, 0);
}
#[derive(Default)]
struct Recording {
    reads: std::cell::Cell<usize>,
    writes: usize,
}
impl GuestMemoryMut for Recording {
    fn read_bytes(&self, _: u64, _: usize) -> Option<Vec<u8>> {
        panic!("unexpected allocating read")
    }
    fn read_into(&self, _: u64, bytes: &mut [u8]) -> bool {
        self.reads.set(self.reads.get() + 1);
        bytes.fill(0);
        true
    }
    fn write_bytes(&mut self, _: u64, _: &[u8]) -> bool {
        self.writes += 1;
        true
    }
}
#[test]
fn overflowed_descriptor_offset_refuses_before_memory_or_decoder_access() {
    for (base, index) in [(u64::MAX, 1), (u64::MAX - 8, 0)] {
        let mem = Recording::default();
        let result = super::address::read_descriptor::<()>(&mem, base, index, |_, _| {
            panic!("overflow reached decoder")
        });
        assert!(result.is_none());
        assert_eq!(mem.reads.get(), 0);
    }
}
#[test]
fn overflowed_used_ring_refuses_before_first_write() {
    for base in [u64::MAX, u64::MAX - 7, u64::MAX - 10] {
        let mut mem = Recording::default();
        super::address::write_used(&mut mem, base, 1, 0, 0);
        assert_eq!(mem.writes, 0);
    }
}
#[test]
fn ordinary_used_ring_still_publishes_element_and_index() {
    let mut mem = Recording::default();
    super::address::write_used(&mut mem, 0x1000, 1, 0, 4);
    assert_eq!(mem.writes, 3);
    assert_eq!(mem.reads.get(), 1);
}
#[test]
fn queue_size_clamps_before_narrowing() {
    let max = 256;
    for (value, expected) in [
        (u64::from(max), max),
        (u64::from(max) + 1, max),
        (65_535, max),
        (65_536, max),
        (u64::MAX, max),
    ] {
        assert_eq!(super::clamp_u16(value, max), expected);
    }
}

#[path = "virtio_queue_publication_tests.rs"]
mod publication_tests;
