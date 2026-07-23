//! Checkpoint save and restore of device, queue and resource state.

use super::*;

impl VirtioPciGpu {
    pub fn snapshot_state(&self) -> Vec<u8> {
        let gpu = &self.gpu;
        let mut out = crate::checkpoint::StateWriter::new();
        out.write_u32(1);
        out.write_u32(gpu.width);
        out.write_u32(gpu.height);
        out.write_u32(gpu.device_features_sel);
        out.write_u32(gpu.driver_features_sel);
        out.write_u32(gpu.driver_features[0]);
        out.write_u32(gpu.driver_features[1]);
        out.write_u16(gpu.config_msix_vector);
        out.write_u16(0);
        out.write_u32(gpu.queue_sel);
        out.write_u8(gpu.pending_msix_queue_bits);
        out.write_u8(0);
        out.write_u16(0);
        out.write_u32(gpu.status);
        out.write_u32(gpu.interrupt_status);
        out.write_u32(gpu.events_clear);

        for queue in &gpu.queues {
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

        out.write_u32(gpu.resources.len() as u32);
        for (&resource_id, resource) in &gpu.resources {
            out.write_u32(resource_id);
            out.write_u32(resource.format);
            out.write_u32(resource.width);
            out.write_u32(resource.height);
            out.write_blob(&resource.host_pixels);
            out.write_u32(resource.backing.len() as u32);
            for backing in &resource.backing {
                out.write_u64(backing.addr);
                out.write_u32(backing.len);
                out.write_u32(0);
            }
        }

        out.write_bool(gpu.scanout_resource.is_some());
        if let Some(resource_id) = gpu.scanout_resource {
            out.write_u32(resource_id);
        }
        out.write_blob(&gpu.scanout);
        out.write_blob(&self.msix.snapshot_state());
        out.into_inner()
    }

    pub fn restore_state(&mut self, data: &[u8]) {
        let mut input = crate::checkpoint::StateReader::new(data);
        assert_eq!(
            input.read_u32(),
            1,
            "unsupported virtio-gpu snapshot version"
        );

        let width = input.read_u32();
        let height = input.read_u32();
        assert_eq!(
            (width, height),
            (self.gpu.width, self.gpu.height),
            "virtio-gpu resolution mismatch on restore"
        );

        self.gpu.device_features_sel = input.read_u32();
        self.gpu.driver_features_sel = input.read_u32();
        self.gpu.driver_features = [input.read_u32(), input.read_u32()];
        self.gpu.config_msix_vector = input.read_u16();
        assert_eq!(input.read_u16(), 0, "invalid virtio-gpu snapshot");
        self.gpu.queue_sel = input.read_u32();
        self.gpu.pending_msix_queue_bits = input.read_u8();
        assert_eq!(input.read_u8(), 0, "invalid virtio-gpu snapshot");
        assert_eq!(input.read_u16(), 0, "invalid virtio-gpu snapshot");
        self.gpu.status = input.read_u32();
        self.gpu.interrupt_status = input.read_u32();
        self.gpu.events_clear = input.read_u32();

        for queue in &mut self.gpu.queues {
            queue.size = input.read_u16();
            queue.ready = input.read_bool();
            queue.pending_msix = input.read_bool();
            queue.desc = input.read_u64();
            queue.driver = input.read_u64();
            queue.device = input.read_u64();
            queue.msix_vector = input.read_u16();
            queue.notify_off = input.read_u16();
            queue.last_avail_idx = input.read_u16();
            assert_eq!(input.read_u16(), 0, "invalid virtio-gpu queue snapshot");
        }

        self.gpu.resources.clear();
        let resource_count = input.read_u32() as usize;
        for _ in 0..resource_count {
            let resource_id = input.read_u32();
            let format = input.read_u32();
            let width = input.read_u32();
            let height = input.read_u32();
            let host_pixels = input.read_blob();

            let backing_count = input.read_u32() as usize;
            let mut backing = Vec::with_capacity(backing_count);
            for _ in 0..backing_count {
                backing.push(BackingEntry {
                    addr: input.read_u64(),
                    len: input.read_u32(),
                });
                assert_eq!(input.read_u32(), 0, "invalid GPU backing snapshot");
            }

            self.gpu.resources.insert(
                resource_id,
                GpuResource {
                    format,
                    width,
                    height,
                    host_pixels,
                    backing,
                },
            );
        }

        self.gpu.scanout_resource = if input.read_bool() {
            Some(input.read_u32())
        } else {
            None
        };
        if let Some(resource_id) = self.gpu.scanout_resource {
            if !self.gpu.resources.contains_key(&resource_id) {
                // The active desktop scanout is normally backed by a 3D/blob resource
                // whose pixels live in the (non-serializable) virglrenderer host
                // context, so it is absent from the restored 2D resource map. Drop the
                // dangling reference rather than panicking; on resume the guest WDDM
                // driver detects the lost adapter, TDR-resets, and re-establishes the
                // scanout (the documented "3D contexts lost on restore" behavior).
                self.gpu.scanout_resource = None;
            }
        }
        self.gpu.scanout = input.read_blob();

        self.gpu.unbind_blob_scanout();
        self.gpu.three_d.reset();
        self.gpu.pending_fenced.clear();
        self.gpu.pending_vblank.clear();
        self.gpu.completed_fences_scratch.clear();
        self.gpu.descriptor_scratch.clear();
        self.gpu.parked_descriptor_scratch.clear();
        self.gpu.request_scratch.clear();
        self.gpu.response_scratch.clear();
        self.gpu.parked_response_scratch.clear();
        self.gpu.blob_row_scratch.clear();
        self.gpu.last_vblank = None;
        self.gpu.last_3d_scanout_readback = None;
        self.gpu.publish_vblank_wake();
        self.gpu.publish_scanout_fb_unconditionally();

        self.msix.restore_state(&input.read_blob());
        input.finish();
    }
}
