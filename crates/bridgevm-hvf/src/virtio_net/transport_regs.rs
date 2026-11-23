//! virtio-net on the shared virtio-PCI transport: features, device config, doorbell, reset.

use super::*;
use crate::fwcfg::GuestMemoryMut;

impl<B: NetBackend> VirtioPciDevice<(), QUEUE_COUNT> for VirtioNet<B> {
    const DEVICE_ID: u32 = DEVICE_ID_NET;
    const QUEUE_MAX: u16 = QUEUE_MAX;

    fn pci(&self) -> &VirtioPciState<(), QUEUE_COUNT> {
        &self.pci
    }

    fn pci_mut(&mut self) -> &mut VirtioPciState<(), QUEUE_COUNT> {
        &mut self.pci
    }

    fn offered_features_word(&self, select: u32) -> u32 {
        match select {
            0 => VIRTIO_NET_F_MAC | VIRTIO_NET_F_STATUS,
            1 => VIRTIO_F_VERSION_1,
            _ => 0,
        }
    }

    fn reset_device(&mut self) {
        self.reset_runtime_state();
    }

    fn notify(&mut self, queue_index: u16, mem: &mut dyn GuestMemoryMut) {
        self.notify_queue(queue_index, mem);
    }

    fn config_read(&self, offset: u64, size: u8) -> u64 {
        let mut config = [0u8; 0x40];
        config[0..6].copy_from_slice(&self.mac);
        config[6..8].copy_from_slice(&VIRTIO_NET_S_LINK_UP.to_le_bytes());
        read_le_from_bytes(&config, offset, size).unwrap_or(0)
    }
}
