//! Checkpoint save and restore of the PCI console device.

use super::*;

impl VirtioPciConsole {
    pub fn snapshot_state(&self) -> Vec<u8> {
        let console = &self.console;
        let mut out = crate::checkpoint::StateWriter::new();
        out.write_u32(1);
        out.write_u32(console.device_features_sel);
        out.write_u32(console.driver_features_sel);
        out.write_u32(console.driver_features[0]);
        out.write_u32(console.driver_features[1]);
        out.write_u16(console.config_msix_vector);
        out.write_u16(0);
        out.write_u32(console.queue_sel);
        out.write_u8(console.pending_msix_queue_bits);
        out.write_u8(0);
        out.write_u16(0);
        out.write_u32(console.status);
        out.write_u32(console.interrupt_status);
        out.write_u32(console.emerg_wr);

        for queue in &console.queues {
            out.write_u16(queue.size);
            out.write_bool(queue.ready);
            out.write_bool(queue.pending_msix);
            out.write_u64(queue.desc);
            out.write_u64(queue.driver);
            out.write_u64(queue.device);
            out.write_u16(queue.msix_vector);
            out.write_u16(queue.notify_off);
            out.write_u16(queue.last_avail_idx);
            out.write_u16(queue.last_avail_seen);
            out.write_u64(queue.notify_count);
            out.write_u64(queue.used_produced);
            out.write_u64(queue.rx_no_buffers);
        }

        for port in &console.ports {
            out.write_bool(port.ready);
            out.write_bool(port.guest_open);
            out.write_bool(port.host_open);
            out.write_u8(0);
        }

        out.write_u32(console.pending_control.len() as u32);
        for message in &console.pending_control {
            out.write_blob(message.as_slice());
        }

        out.write_blob(&console.host_to_guest.iter().copied().collect::<Vec<_>>());
        out.write_blob(&console.host_inbound);
        out.write_bool(console.agent_connected_confirmed);
        out.write_blob(&self.msix.snapshot_state());
        out.into_inner()
    }

    pub fn restore_state(&mut self, data: &[u8]) {
        let mut input = crate::checkpoint::StateReader::new(data);
        assert_eq!(
            input.read_u32(),
            1,
            "unsupported virtio-console snapshot version"
        );

        let console = &mut self.console;
        console.device_features_sel = input.read_u32();
        console.driver_features_sel = input.read_u32();
        console.driver_features = [input.read_u32(), input.read_u32()];
        console.config_msix_vector = input.read_u16();
        assert_eq!(input.read_u16(), 0, "invalid virtio-console snapshot");
        console.queue_sel = input.read_u32();
        console.pending_msix_queue_bits = input.read_u8();
        assert_eq!(input.read_u8(), 0, "invalid virtio-console snapshot");
        assert_eq!(input.read_u16(), 0, "invalid virtio-console snapshot");
        console.status = input.read_u32();
        console.interrupt_status = input.read_u32();
        console.emerg_wr = input.read_u32();

        for queue in &mut console.queues {
            queue.size = input.read_u16();
            queue.ready = input.read_bool();
            queue.pending_msix = input.read_bool();
            queue.desc = input.read_u64();
            queue.driver = input.read_u64();
            queue.device = input.read_u64();
            queue.msix_vector = input.read_u16();
            queue.notify_off = input.read_u16();
            queue.last_avail_idx = input.read_u16();
            queue.last_avail_seen = input.read_u16();
            queue.notify_count = input.read_u64();
            queue.used_produced = input.read_u64();
            queue.rx_no_buffers = input.read_u64();
        }

        for port in &mut console.ports {
            port.ready = input.read_bool();
            port.guest_open = input.read_bool();
            port.host_open = input.read_bool();
            assert_eq!(input.read_u8(), 0, "invalid console port snapshot");
        }

        console.pending_control.clear();
        let pending_count = input.read_u32() as usize;
        for _ in 0..pending_count {
            let bytes = input.read_blob();
            assert!(
                bytes.len() <= MAX_CONTROL_MESSAGE_LEN,
                "oversized restored console control message"
            );
            console
                .pending_control
                .push_back(PendingControlMessage::from_slice(&bytes));
        }

        console.host_to_guest = input.read_blob().into();
        console.host_inbound = input.read_blob();
        console.agent_connected_confirmed = input.read_bool();
        console.descriptor_scratch.clear();
        console.read_scratch.clear();

        self.msix.restore_state(&input.read_blob());
        input.finish();
    }
}
