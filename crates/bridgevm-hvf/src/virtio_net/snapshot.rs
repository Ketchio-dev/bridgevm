//! Checkpoint save and restore of the PCI net device.

use super::*;

impl<B: NetBackend> VirtioPciNet<B> {
    pub fn snapshot_state(&self) -> Vec<u8> {
        let net = &self.net;
        let mut out = crate::checkpoint::StateWriter::new();
        out.write_u32(1);
        out.write_blob(&net.mac);
        out.write_u32(net.device_features_sel);
        out.write_u32(net.driver_features_sel);
        out.write_u32(net.driver_features[0]);
        out.write_u32(net.driver_features[1]);
        out.write_u16(net.config_msix_vector);
        out.write_u16(0);
        out.write_u32(net.queue_sel);
        out.write_u8(net.pending_msix_queue_bits);
        out.write_u8(0);
        out.write_u16(0);
        out.write_u32(net.status);
        out.write_u32(net.interrupt_status);

        for queue in &net.queues {
            out.write_u16(queue.size);
            out.write_bool(queue.ready);
            out.write_bool(queue.pending_msix);
            out.write_u64(queue.desc);
            out.write_u64(queue.driver);
            out.write_u64(queue.device);
            out.write_u16(queue.msix_vector);
            out.write_u16(queue.notify_off);
            out.write_u16(queue.last_avail_idx);
            out.write_u16(0);
        }

        out.write_bool(net.pending_rx_frame.is_some());
        if let Some(frame) = &net.pending_rx_frame {
            out.write_blob(frame);
        }

        out.write_blob(&self.msix.snapshot_state());
        out.into_inner()
    }

    pub fn restore_state(&mut self, data: &[u8]) {
        let mut input = crate::checkpoint::StateReader::new(data);
        assert_eq!(
            input.read_u32(),
            1,
            "unsupported virtio-net snapshot version"
        );

        let mac = input.read_blob();
        assert_eq!(mac.len(), 6, "invalid restored virtio-net MAC");
        self.net.mac.copy_from_slice(&mac);
        self.net.device_features_sel = input.read_u32();
        self.net.driver_features_sel = input.read_u32();
        self.net.driver_features = [input.read_u32(), input.read_u32()];
        self.net.config_msix_vector = input.read_u16();
        assert_eq!(input.read_u16(), 0, "invalid virtio-net snapshot");
        self.net.queue_sel = input.read_u32();
        self.net.pending_msix_queue_bits = input.read_u8();
        assert_eq!(input.read_u8(), 0, "invalid virtio-net snapshot");
        assert_eq!(input.read_u16(), 0, "invalid virtio-net snapshot");
        self.net.status = input.read_u32();
        self.net.interrupt_status = input.read_u32();

        for queue in &mut self.net.queues {
            queue.size = input.read_u16();
            queue.ready = input.read_bool();
            queue.pending_msix = input.read_bool();
            queue.desc = input.read_u64();
            queue.driver = input.read_u64();
            queue.device = input.read_u64();
            queue.msix_vector = input.read_u16();
            queue.notify_off = input.read_u16();
            queue.last_avail_idx = input.read_u16();
            assert_eq!(input.read_u16(), 0, "invalid virtio-net queue snapshot");
        }

        self.net.pending_rx_frame = if input.read_bool() {
            Some(input.read_blob())
        } else {
            None
        };
        self.net.descriptor_scratch.clear();
        self.net.tx_packet_scratch.clear();
        self.net.rx_frame_scratch.clear();

        self.msix.restore_state(&input.read_blob());
        input.finish();
    }
}
