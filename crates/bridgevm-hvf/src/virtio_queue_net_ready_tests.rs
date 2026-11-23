//! Overflow refusal remains exercised after DRIVER_OK gating.
use super::Unmapped;
use crate::virtio_net::{LoopbackTestBackend, VirtioNet};

#[test]
fn net_overflowing_driver_base_refuses_without_panicking() {
    let mut dev = VirtioNet::new(LoopbackTestBackend::default());
    dev.pci.status = 4;
    dev.pci.queues[1].ready = true;
    dev.pci.queues[1].size = 1;
    dev.pci.queues[1].desc = 16;
    dev.pci.queues[1].driver = u64::MAX;
    dev.process_tx_queue(&mut Unmapped);
    assert_eq!(dev.pci.queues[1].last_avail_idx, 0);
}
