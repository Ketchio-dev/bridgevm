//! Continuation of the `magic_value` impl block, split for the 1000-line rule.

use super::*;
use crate::fwcfg::GuestMemoryMut;
use crate::pcie::VIRTIO_GPU_MSIX_VECTOR_COUNT;
use crate::ramfb::DRM_FORMAT_XRGB8888;
use crate::virtio_gpu_3d::BlobMemEntry;
use crate::virtio_gpu_3d::VIRTIO_GPU_BLOB_MEM_GUEST;
use crate::virtio_gpu_3d::VIRTIO_GPU_BLOB_MEM_HOST3D;
use crate::virtio_gpu_trace::write_json_string;
use std::fmt::Write as _;
use std::time::Instant;

impl VirtioGpu {
    pub(crate) fn resource_create_2d_into(
        &mut self,
        request: &[u8],
        hdr: Option<CtrlHdr>,
        out: &mut Vec<u8>,
    ) {
        let Some(resource_id) = read_le_u32(request, 24) else {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_UNSPEC, hdr);
            return;
        };
        let format = read_le_u32(request, 28).unwrap_or(0);
        let width = read_le_u32(request, 32).unwrap_or(0);
        let height = read_le_u32(request, 36).unwrap_or(0);
        if resource_id == 0 || width == 0 || height == 0 || !format_supported(format) {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_UNSPEC, hdr);
            return;
        }
        let Some(len) = u64::from(width)
            .checked_mul(u64::from(height))
            .and_then(|pixels| pixels.checked_mul(4))
            .and_then(|bytes| usize::try_from(bytes).ok())
        else {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_UNSPEC, hdr);
            return;
        };
        self.resources.insert(
            resource_id,
            GpuResource {
                format,
                width,
                height,
                host_pixels: vec![0; len],
                backing: Vec::new(),
            },
        );
        self.three_d.register_2d_resource(resource_id);
        response_hdr_into(out, VIRTIO_GPU_RESP_OK_NODATA, hdr);
    }

    pub(crate) fn resource_unref_into(
        &mut self,
        request: &[u8],
        hdr: Option<CtrlHdr>,
        out: &mut Vec<u8>,
    ) {
        if let Some(resource_id) = read_le_u32(request, 24) {
            if self
                .blob_scanout
                .as_ref()
                .map(|scanout| scanout.resource_id)
                == Some(resource_id)
            {
                self.unbind_blob_scanout();
            }
            self.resources.remove(&resource_id);
            self.three_d.unref_resource(resource_id);
            if self.scanout_resource == Some(resource_id) {
                self.scanout_resource = None;
            }
        }
        response_hdr_into(out, VIRTIO_GPU_RESP_OK_NODATA, hdr);
    }

    pub(crate) fn attach_backing_into(
        &mut self,
        mem: &dyn GuestMemoryMut,
        request: &[u8],
        hdr: Option<CtrlHdr>,
        out: &mut Vec<u8>,
    ) {
        let Some(resource_id) = read_le_u32(request, 24) else {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_UNSPEC, hdr);
            return;
        };
        let nr_entries = read_le_u32(request, 28).unwrap_or(0);
        let Some(entries_len) = (nr_entries as usize).checked_mul(16) else {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_UNSPEC, hdr);
            return;
        };
        if request.len().saturating_sub(32) < entries_len {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_UNSPEC, hdr);
            return;
        }
        let mut backing = Vec::with_capacity(nr_entries as usize);
        let mut offset = 32usize;
        for _ in 0..nr_entries {
            let addr = read_le_u64(request, offset).unwrap();
            let len = read_le_u32(request, offset + 8).unwrap();
            backing.push(BlobMemEntry { addr, len });
            offset += 16;
        }
        if let Some(resource) = self.resources.get_mut(&resource_id) {
            resource.backing.clear();
            resource
                .backing
                .extend(backing.iter().map(|entry| BackingEntry {
                    addr: entry.addr,
                    len: entry.len,
                }));
        } else if self.three_d.is_3d_resource(resource_id) {
            if !self.three_d.attach_3d_backing(mem, resource_id, &backing) {
                response_hdr_into(out, VIRTIO_GPU_RESP_ERR_UNSPEC, hdr);
                return;
            }
        } else {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_UNSPEC, hdr);
            return;
        }
        response_hdr_into(out, VIRTIO_GPU_RESP_OK_NODATA, hdr);
    }

    pub(crate) fn detach_backing_into(
        &mut self,
        request: &[u8],
        hdr: Option<CtrlHdr>,
        out: &mut Vec<u8>,
    ) {
        if let Some(resource_id) = read_le_u32(request, 24) {
            if let Some(resource) = self.resources.get_mut(&resource_id) {
                resource.backing.clear();
            } else if self.three_d.is_3d_resource(resource_id)
                && !self.three_d.detach_3d_backing(resource_id)
            {
                response_hdr_into(out, VIRTIO_GPU_RESP_ERR_UNSPEC, hdr);
                return;
            }
        }
        response_hdr_into(out, VIRTIO_GPU_RESP_OK_NODATA, hdr);
    }

    pub(crate) fn set_scanout_into(
        &mut self,
        request: &[u8],
        hdr: Option<CtrlHdr>,
        out: &mut Vec<u8>,
    ) {
        let rect = read_rect(request, 24).unwrap_or(Rect {
            x: 0,
            y: 0,
            width: 0,
            height: 0,
        });
        let scanout_id = read_le_u32(request, 40).unwrap_or(u32::MAX);
        let resource_id = read_le_u32(request, 44).unwrap_or(0);
        if scanout_id != 0 {
            response_hdr_into(out, VIRTIO_GPU_RESP_OK_NODATA, hdr);
            return;
        }
        if resource_id == 0 {
            self.scanout_resource = None;
            self.unbind_blob_scanout();
        } else {
            let valid_resource = self.resources.contains_key(&resource_id)
                || self
                    .three_d
                    .scanout_3d_info(resource_id)
                    .is_some_and(|info| {
                        format_supported(info.format)
                            && rect.width > 0
                            && rect.height > 0
                            && rect.width <= self.width
                            && rect.height <= self.height
                            && rect
                                .x
                                .checked_add(rect.width)
                                .is_some_and(|end| end <= info.width)
                            && rect
                                .y
                                .checked_add(rect.height)
                                .is_some_and(|end| end <= info.height)
                    });
            if !valid_resource {
                response_hdr_into(out, VIRTIO_GPU_RESP_ERR_UNSPEC, hdr);
                return;
            }
            self.unbind_blob_scanout();
            self.scanout_resource = Some(resource_id);
        }
        response_hdr_into(out, VIRTIO_GPU_RESP_OK_NODATA, hdr);
    }

    pub(crate) fn set_scanout_blob_into(
        &mut self,
        request: &[u8],
        hdr: Option<CtrlHdr>,
        out: &mut Vec<u8>,
    ) {
        if request.len() < SET_SCANOUT_BLOB_LEN {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_UNSPEC, hdr);
            return;
        }
        let scanout_id = read_le_u32(request, 40).unwrap_or(u32::MAX);
        let resource_id = read_le_u32(request, 44).unwrap_or(0);
        if scanout_id != 0 {
            response_hdr_into(out, VIRTIO_GPU_RESP_OK_NODATA, hdr);
            return;
        }
        if resource_id == 0 {
            self.unbind_blob_scanout();
            self.scanout_resource = None;
            response_hdr_into(out, VIRTIO_GPU_RESP_OK_NODATA, hdr);
            return;
        }

        let width = read_le_u32(request, 48).unwrap_or(0);
        let height = read_le_u32(request, 52).unwrap_or(0);
        let format = read_le_u32(request, 56).unwrap_or(0);
        let stride = read_le_u32(request, 64).unwrap_or(0);
        let offset = read_le_u32(request, 80).unwrap_or(0);
        if width == 0
            || height == 0
            || width > self.width
            || height > self.height
            || !format_supported(format)
            || stride < width.saturating_mul(4)
        {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_UNSPEC, hdr);
            return;
        }

        let Some(info) = self.three_d.blob_resource_info_ref(resource_id) else {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_UNSPEC, hdr);
            return;
        };
        let blob_mem = info.blob_mem;
        let blob_size = info.size;
        if blob_mem != VIRTIO_GPU_BLOB_MEM_GUEST && blob_mem != VIRTIO_GPU_BLOB_MEM_HOST3D {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_UNSPEC, hdr);
            return;
        }
        let Some(footprint) = blob_surface_footprint(width, height, stride, offset) else {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_UNSPEC, hdr);
            return;
        };
        if footprint > blob_size {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_UNSPEC, hdr);
            return;
        }

        self.unbind_blob_scanout();
        let mapping = if blob_mem == VIRTIO_GPU_BLOB_MEM_HOST3D {
            let Some(mapped) = self.three_d.scanout_map_blob(resource_id) else {
                response_hdr_into(out, VIRTIO_GPU_RESP_ERR_UNSPEC, hdr);
                return;
            };
            if mapped.host_ptr.is_null() || (mapped.size as u64) < footprint {
                self.three_d.scanout_unmap_blob(resource_id);
                response_hdr_into(out, VIRTIO_GPU_RESP_ERR_UNSPEC, hdr);
                return;
            }
            Some(BlobScanoutMapping {
                ptr: mapped.host_ptr,
                len: mapped.size,
            })
        } else {
            None
        };
        self.scanout_resource = None;
        self.blob_scanout = Some(BlobScanout {
            resource_id,
            width,
            height,
            format,
            stride,
            offset,
            mapping,
        });
        response_hdr_into(out, VIRTIO_GPU_RESP_OK_NODATA, hdr);
    }

    pub(crate) fn transfer_to_host_2d_into(
        &mut self,
        mem: &dyn GuestMemoryMut,
        request: &[u8],
        hdr: Option<CtrlHdr>,
        out: &mut Vec<u8>,
    ) {
        let rect = read_rect(request, 24).unwrap_or(Rect {
            x: 0,
            y: 0,
            width: 0,
            height: 0,
        });
        let offset = read_le_u64(request, 40).unwrap_or(0);
        let resource_id = read_le_u32(request, 48).unwrap_or(0);
        let Some(resource) = self.resources.get_mut(&resource_id) else {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_UNSPEC, hdr);
            return;
        };
        copy_backing_to_resource(mem, resource, rect, offset);
        response_hdr_into(out, VIRTIO_GPU_RESP_OK_NODATA, hdr);
    }

    pub(crate) fn publish_scanout_fb(&mut self) {
        if self.scanout_resource.is_none() && self.blob_scanout.is_none() {
            return;
        }
        self.publish_scanout_fb_unconditionally();
    }

    /// Write the current `scanout` pixels to the export sink even without an
    /// active scanout binding. Restore uses this: the checkpointed pixels are
    /// the last frame the guest presented, but the blob scanout that produced
    /// them is not serializable, so without this one-shot publish the display
    /// export stays black until the guest's WDDM TDR re-establishes the
    /// scanout and presents fresh.
    pub(crate) fn publish_scanout_fb_unconditionally(&mut self) {
        let width = self.width;
        let height = self.height;
        let stride = width * 4;
        if self.scanout.len() < (stride as usize) * (height as usize) {
            return;
        }
        let (fb_sink, scanout) = (&mut self.fb_sink, &self.scanout);
        if let Some(sink) = fb_sink.as_mut() {
            sink.write(width, height, stride, DRM_FORMAT_XRGB8888, scanout);
        }
    }

    pub(crate) fn resource_flush_into(
        &mut self,
        mem: &dyn GuestMemoryMut,
        request: &[u8],
        hdr: Option<CtrlHdr>,
        out: &mut Vec<u8>,
    ) {
        let rect = read_rect(request, 24).unwrap_or(Rect {
            x: 0,
            y: 0,
            width: 0,
            height: 0,
        });
        let resource_id = read_le_u32(request, 40).unwrap_or(0);
        if self.scanout_resource == Some(resource_id) {
            if let Some(resource) = self.resources.get(&resource_id) {
                composite_resource_to_scanout(
                    &mut self.scanout,
                    self.width,
                    self.height,
                    resource,
                    rect,
                );
            } else if self.three_d.is_3d_resource(resource_id) {
                self.scanout_3d_flush_count = self.scanout_3d_flush_count.saturating_add(1);
                let flush_count = self.scanout_3d_flush_count;
                if flush_count <= 8 {
                    let info = self.three_d.scanout_3d_info(resource_id);
                    let local_backing = self.three_d.local_3d_backing(resource_id).is_some();
                    let display_width = self.width;
                    let display_height = self.height;
                    self.record_trace_fields("scanout_3d_flush", |fields| {
                        let _ = write!(
                            fields,
                            ",\"resource_id\":{resource_id},\"resource_width\":{},\"resource_height\":{},\"display_width\":{},\"display_height\":{},\"local_backing\":{local_backing},\"count\":{flush_count}",
                            info.map_or(0, |info| info.width),
                            info.map_or(0, |info| info.height),
                            display_width,
                            display_height
                        );
                    });
                }
                let local_readback = self
                    .three_d
                    .scanout_3d_info(resource_id)
                    .zip(self.three_d.local_3d_backing(resource_id))
                    .map(|(info, backing)| {
                        let started = Instant::now();
                        let copied = composite_local_3d_to_scanout(
                            mem,
                            backing,
                            info,
                            &mut self.scanout,
                            self.width,
                            self.height,
                            rect,
                            &mut self.blob_row_scratch,
                        );
                        (copied, started.elapsed())
                    });
                if let Some((readback_ok, elapsed)) = local_readback {
                    self.scanout_readback_attempt_count =
                        self.scanout_readback_attempt_count.saturating_add(1);
                    let duration_ns = elapsed.as_nanos().min(u128::from(u64::MAX)) as u64;
                    self.scanout_readback_nanoseconds = self
                        .scanout_readback_nanoseconds
                        .saturating_add(duration_ns);
                    if readback_ok {
                        self.scanout_readback_count = self.scanout_readback_count.saturating_add(1);
                        let bytes = u64::from(rect.width)
                            .saturating_mul(u64::from(rect.height))
                            .saturating_mul(4);
                        self.scanout_readback_bytes =
                            self.scanout_readback_bytes.saturating_add(bytes);
                        let count = self.scanout_readback_count;
                        self.record_trace_fields("scanout_guest_backing", |fields| {
                            let _ = write!(
                                fields,
                                ",\"resource_id\":{resource_id},\"width\":{},\"height\":{},\"bytes\":{bytes},\"duration_ns\":{duration_ns},\"count\":{count}",
                                rect.width, rect.height
                            );
                        });
                    }
                    self.publish_scanout_fb();
                    response_hdr_into(out, VIRTIO_GPU_RESP_OK_NODATA, hdr);
                    return;
                }
                if self.scanout_3d_deferred {
                    // Decouple the GL readback from the guest's flush: arm a
                    // pending readback and respond OK now; the per-exit drain
                    // services it after the vCPU has resumed at least once.
                    self.defer_3d_scanout(resource_id, rect);
                    response_hdr_into(out, VIRTIO_GPU_RESP_OK_NODATA, hdr);
                    return;
                }
                self.blit_3d_scanout_iosurface(resource_id);
                match self.try_3d_scanout_readback(resource_id, rect, false) {
                    ScanoutReadbackOutcome::Done | ScanoutReadbackOutcome::Gone => {}
                    ScanoutReadbackOutcome::NotDue => {
                        self.scanout_readback_throttled_count =
                            self.scanout_readback_throttled_count.saturating_add(1);
                        let throttled = self.scanout_readback_throttled_count;
                        let width = self.width;
                        let height = self.height;
                        self.record_trace_fields("scanout_readback_throttled", |fields| {
                            let _ = write!(
                                fields,
                                ",\"resource_id\":{resource_id},\"width\":{width},\"height\":{height},\"count\":{throttled}"
                            );
                        });
                    }
                }
            }
        } else if self
            .blob_scanout
            .as_ref()
            .is_some_and(|scanout| scanout.resource_id == resource_id)
        {
            self.composite_blob_scanout(mem, rect);
        }
        self.publish_scanout_fb();
        response_hdr_into(out, VIRTIO_GPU_RESP_OK_NODATA, hdr);
    }

    pub(crate) fn composite_blob_scanout(&mut self, mem: &dyn GuestMemoryMut, rect: Rect) {
        let Some(scanout) = self.blob_scanout.as_ref() else {
            return;
        };
        let Some(info) = self.three_d.blob_resource_info_ref(scanout.resource_id) else {
            return;
        };
        let x_end = rect
            .x
            .saturating_add(rect.width)
            .min(self.width)
            .min(scanout.width);
        let y_end = rect
            .y
            .saturating_add(rect.height)
            .min(self.height)
            .min(scanout.height);

        match info.blob_mem {
            VIRTIO_GPU_BLOB_MEM_GUEST => composite_guest_blob_to_scanout(
                GuestBlobComposite {
                    mem,
                    backing: info.backing,
                    scanout: &mut self.scanout,
                    scanout_width: self.width,
                    blob: scanout,
                    row_pixels: &mut self.blob_row_scratch,
                },
                rect,
                x_end,
                y_end,
            ),
            VIRTIO_GPU_BLOB_MEM_HOST3D => {
                let Some(mapping) = scanout.mapping else {
                    return;
                };
                let pixels = unsafe { std::slice::from_raw_parts(mapping.ptr, mapping.len) };
                composite_host_blob_to_scanout(
                    pixels,
                    &mut self.scanout,
                    self.width,
                    scanout,
                    rect,
                    x_end,
                    y_end,
                );
            }
            _ => {}
        }
    }

    pub(crate) fn unbind_blob_scanout(&mut self) {
        if let Some(scanout) = self.blob_scanout.take() {
            if scanout.mapping.is_some() {
                self.three_d.scanout_unmap_blob(scanout.resource_id);
            }
        }
    }

    pub(crate) fn trace_device_init(&mut self, backend_3d: bool) {
        let width = self.width;
        let height = self.height;
        self.record_trace_fields("device_init", |fields| {
            let _ = write!(
                fields,
                ",\"width\":{},\"height\":{},\"device_id\":{},\"vendor_id\":{},\"queue_count\":{},\"queue_max\":{},\"msix_vectors\":{},\"backend_3d\":{},\"common_cfg_offset\":{},\"device_cfg_offset\":{},\"notify_cfg_offset\":{}",
                width,
                height,
                DEVICE_ID_GPU,
                VENDOR_ID_QEMU,
                QUEUE_COUNT,
                QUEUE_MAX,
                VIRTIO_GPU_MSIX_VECTOR_COUNT,
                backend_3d,
                PCI_COMMON_CFG_OFFSET,
                PCI_DEVICE_CFG_OFFSET,
                PCI_NOTIFY_CFG_OFFSET
            );
        });
    }

    pub(crate) fn trace_common_read(&mut self, offset: u64, size: u8, value: u64) {
        if !self.trace.enabled() {
            return;
        }
        let field = match offset {
            COMMON_DEVICE_FEATURE | REG_DEVICE_FEATURES => "device_features",
            COMMON_DRIVER_FEATURE | REG_DRIVER_FEATURES => "driver_features",
            COMMON_DEVICE_STATUS | REG_STATUS => "device_status",
            COMMON_QUEUE_SIZE | REG_QUEUE_NUM => "queue_size",
            COMMON_QUEUE_ENABLE | REG_QUEUE_READY => "queue_enable",
            _ => return,
        };
        let device_features_sel = self.device_features_sel;
        let driver_features_sel = self.driver_features_sel;
        let queue_sel = self.queue_sel;
        self.record_trace_fields("common_read", |fields| {
            fields.push_str(",\"field\":");
            write_json_string(fields, field);
            let _ = write!(
                fields,
                ",\"offset\":{},\"size\":{},\"value\":{},\"value_hex\":\"{:#x}\",\"device_features_sel\":{},\"driver_features_sel\":{},\"queue_sel\":{}",
                offset,
                size,
                value,
                value,
                device_features_sel,
                driver_features_sel,
                queue_sel
            );
        });
    }
}
