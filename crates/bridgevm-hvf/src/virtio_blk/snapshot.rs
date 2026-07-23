//! Checkpoint save and restore for the MMIO core and the PCI wrapper.

use super::*;
use trace::RecentVirtioBlockRequests;

impl VirtioPciBlock {
    pub fn snapshot_state(&self) -> Vec<u8> {
        let mut out = crate::checkpoint::StateWriter::new();
        out.write_u32(1);
        out.write_blob(&self.block.snapshot_state());
        out.write_blob(&self.msix.snapshot_state());
        out.into_inner()
    }

    pub fn restore_state(&mut self, data: &[u8]) {
        let mut input = crate::checkpoint::StateReader::new(data);
        assert_eq!(
            input.read_u32(),
            1,
            "unsupported virtio-pci block snapshot version"
        );
        self.block.restore_state(&input.read_blob());
        self.msix.restore_state(&input.read_blob());
        input.finish();
    }
}

impl VirtioMmioBlock {
    pub fn snapshot_state(&self) -> Vec<u8> {
        let mut out = crate::checkpoint::StateWriter::new();
        out.write_u32(1);
        out.write_u32(self.transport.version());
        out.write_u32(self.device_features_sel);
        out.write_u32(self.driver_features_sel);
        out.write_u32(self.driver_features[0]);
        out.write_u32(self.driver_features[1]);
        out.write_u32(self.guest_page_size);
        out.write_u32(self.queue_sel);
        out.write_u16(self.queue_num);
        out.write_u16(0);
        out.write_u32(self.queue_align);
        out.write_bool(self.queue_ready);
        out.write_u8(0);
        out.write_u16(0);
        out.write_u64(self.queue_desc);
        out.write_u64(self.queue_driver);
        out.write_u64(self.queue_device);
        out.write_u32(self.status);
        out.write_u32(self.interrupt_status);
        out.write_u16(self.last_avail_idx);
        out.write_u16(0);
        out.write_u64(self.request_sequence);
        out.into_inner()
    }

    pub fn restore_state(&mut self, data: &[u8]) {
        let mut input = crate::checkpoint::StateReader::new(data);
        assert_eq!(
            input.read_u32(),
            1,
            "unsupported virtio-block snapshot version"
        );
        assert_eq!(
            input.read_u32(),
            self.transport.version(),
            "virtio-block transport mismatch on restore"
        );

        self.device_features_sel = input.read_u32();
        self.driver_features_sel = input.read_u32();
        self.driver_features = [input.read_u32(), input.read_u32()];
        self.guest_page_size = input.read_u32();
        self.queue_sel = input.read_u32();
        self.queue_num = input.read_u16();
        assert_eq!(input.read_u16(), 0, "invalid virtio-block snapshot");
        self.queue_align = input.read_u32();
        self.queue_ready = input.read_bool();
        assert_eq!(input.read_u8(), 0, "invalid virtio-block snapshot");
        assert_eq!(input.read_u16(), 0, "invalid virtio-block snapshot");
        self.queue_desc = input.read_u64();
        self.queue_driver = input.read_u64();
        self.queue_device = input.read_u64();
        self.status = input.read_u32();
        self.interrupt_status = input.read_u32();
        self.last_avail_idx = input.read_u16();
        assert_eq!(input.read_u16(), 0, "invalid virtio-block snapshot");
        self.request_sequence = input.read_u64();

        self.request_trace = RecentVirtioBlockRequests::default();
        self.descriptor_scratch.clear();
        self.read_scratch.clear();
        input.finish();
    }
}
