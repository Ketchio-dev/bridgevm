//! virtio-console on the shared virtio-PCI transport: features, device config, doorbell, reset.

use super::*;
use crate::fwcfg::GuestMemoryMut;

impl VirtioPciDevice<QueueCounters, QUEUE_COUNT> for VirtioConsole {
    const DEVICE_ID: u32 = DEVICE_ID_CONSOLE;
    const QUEUE_MAX: u16 = QUEUE_MAX;

    fn pci(&self) -> &VirtioPciState<QueueCounters, QUEUE_COUNT> {
        &self.pci
    }

    fn pci_mut(&mut self) -> &mut VirtioPciState<QueueCounters, QUEUE_COUNT> {
        &mut self.pci
    }

    fn offered_features_word(&self, select: u32) -> u32 {
        match select {
            0 => VIRTIO_CONSOLE_F_MULTIPORT,
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
        let mut config = [0u8; 12];
        config[4..8].copy_from_slice(&(PORT_COUNT as u32).to_le_bytes());
        config[8..12].copy_from_slice(&self.emerg_wr.to_le_bytes());
        read_le_from_bytes(&config, offset, size).unwrap_or(0)
    }

    fn config_write(&mut self, offset: u64, size: u8, value: u64) {
        if (8..12).contains(&offset) && size <= 4 {
            let current = self.emerg_wr;
            let merged = insert_u32(current, offset - 8, size, value);
            self.emerg_wr = merged;
        }
    }
}

pub(crate) fn insert_u32(current: u32, offset: u64, size: u8, value: u64) -> u32 {
    let shift = u32::try_from(offset).unwrap_or(0) * 8;
    let width_mask: u32 = match size {
        1 => 0xff,
        2 => 0xffff,
        4 => 0xffff_ffff,
        _ => 0xffff_ffff,
    };
    let field_mask = width_mask.checked_shl(shift).unwrap_or(0);
    let placed = ((value as u32) & width_mask)
        .checked_shl(shift)
        .unwrap_or(0);
    (current & !field_mask) | placed
}
