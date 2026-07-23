//! Continuation of the `virtio_gpu_f_virgl` impl block, split for the 1000-line rule.

use super::*;

use crate::fwcfg::GuestMemoryMut;
use crate::virtio_gpu_trace::venus_start_trace_enabled;

impl VirtioGpu3d {
    pub(crate) fn ctx_create_into(&mut self, request: &[u8], hdr: CtrlHdr3d, out: &mut Vec<u8>) {
        let Some(backend) = self.backend.as_mut() else {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        };
        if request.len() < CTX_CREATE_LEN
            || hdr.ctx_id == 0
            || self.live_contexts.contains(&hdr.ctx_id)
        {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        }
        let nlen = read_le_u32(request, 24).unwrap_or(0).min(64) as usize;
        let context_init = read_le_u32(request, 28).unwrap_or(0);
        let name = &request[32..32 + nlen];
        if !backend.ctx_create(hdr.ctx_id, context_init, name) {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_UNSPEC, Some(hdr));
            return;
        }
        self.live_contexts.insert(hdr.ctx_id);
        self.ctx_resources.entry(hdr.ctx_id).or_default();
        response_hdr_into(out, VIRTIO_GPU_RESP_OK_NODATA, Some(hdr));
    }

    pub(crate) fn ctx_destroy_into(&mut self, hdr: CtrlHdr3d, out: &mut Vec<u8>) {
        if !self.live_contexts.remove(&hdr.ctx_id) {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        }
        self.ctx_resources.remove(&hdr.ctx_id);
        if let Some(backend) = self.backend.as_mut() {
            backend.ctx_destroy(hdr.ctx_id);
        }
        response_hdr_into(out, VIRTIO_GPU_RESP_OK_NODATA, Some(hdr));
    }

    pub(crate) fn ctx_attach_resource_into(
        &mut self,
        request: &[u8],
        hdr: CtrlHdr3d,
        out: &mut Vec<u8>,
    ) {
        if request.len() < CTX_RESOURCE_LEN {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        }
        let resource_id = read_le_u32(request, 24).unwrap_or(0);
        if !self.resource_exists(resource_id) {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        }
        if let Some(resources) = self.ctx_resources.get_mut(&hdr.ctx_id) {
            resources.insert(resource_id);
        }
        if !self.local_3d_backing.contains_key(&resource_id) {
            if let Some(backend) = self.backend.as_mut() {
                backend.ctx_attach_resource(hdr.ctx_id, resource_id);
            }
        }
        response_hdr_into(out, VIRTIO_GPU_RESP_OK_NODATA, Some(hdr));
    }

    pub(crate) fn ctx_detach_resource_into(
        &mut self,
        request: &[u8],
        hdr: CtrlHdr3d,
        out: &mut Vec<u8>,
    ) {
        if request.len() < CTX_RESOURCE_LEN {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        }
        let resource_id = read_le_u32(request, 24).unwrap_or(0);
        if !self.resource_exists(resource_id) {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        }
        if let Some(resources) = self.ctx_resources.get_mut(&hdr.ctx_id) {
            resources.remove(&resource_id);
        }
        if !self.local_3d_backing.contains_key(&resource_id) {
            if let Some(backend) = self.backend.as_mut() {
                backend.ctx_detach_resource(hdr.ctx_id, resource_id);
            }
        }
        response_hdr_into(out, VIRTIO_GPU_RESP_OK_NODATA, Some(hdr));
    }

    pub(crate) fn resource_create_3d_into(
        &mut self,
        request: &[u8],
        hdr: CtrlHdr3d,
        out: &mut Vec<u8>,
    ) {
        if request.len() < RESOURCE_CREATE_3D_LEN {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        }
        let args = Create3dArgs {
            resource_id: read_le_u32(request, 24).unwrap_or(0),
            target: read_le_u32(request, 28).unwrap_or(0),
            format: read_le_u32(request, 32).unwrap_or(0),
            bind: read_le_u32(request, 36).unwrap_or(0),
            width: read_le_u32(request, 40).unwrap_or(0),
            height: read_le_u32(request, 44).unwrap_or(0),
            depth: read_le_u32(request, 48).unwrap_or(0),
            array_size: read_le_u32(request, 52).unwrap_or(0),
            last_level: read_le_u32(request, 56).unwrap_or(0),
            nr_samples: read_le_u32(request, 60).unwrap_or(0),
            flags: read_le_u32(request, 64).unwrap_or(0),
        };
        if args.resource_id == 0 || self.resource_exists(args.resource_id) {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        }
        // The Venus WDDM KMD creates its shared primary before the UMD has
        // created the context whose numeric id is used by the subsequent
        // CTX_ATTACH_RESOURCE.  Keep that narrowly identified display resource
        // in guest backing even when the renderer also supports legacy virgl
        // resources; otherwise the early attach is lost inside virglrenderer.
        // Non-scanout render targets continue through the renderer below.
        let local_scanout = self.backend.is_some() && is_local_scanout_resource(args);
        let created = local_scanout
            || self
                .backend
                .as_mut()
                .is_some_and(|backend| backend.create_3d(args));
        if !created {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_UNSPEC, Some(hdr));
            return;
        }
        self.resource_3d_ids.insert(args.resource_id);
        self.resource_3d_info.insert(args.resource_id, args);
        if local_scanout {
            self.local_3d_backing.insert(args.resource_id, Vec::new());
            if venus_start_trace_enabled() {
                println!(
                    "venus-start: local display resource_create_3d res={} format={} bind={:#x} size={}x{}",
                    args.resource_id, args.format, args.bind, args.width, args.height
                );
            }
        }
        response_hdr_into(out, VIRTIO_GPU_RESP_OK_NODATA, Some(hdr));
    }

    pub(crate) fn transfer_3d_into(
        &mut self,
        request: &[u8],
        hdr: CtrlHdr3d,
        to_host: bool,
        out: &mut Vec<u8>,
    ) {
        if request.len() < TRANSFER_3D_LEN {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        }
        let args = Transfer3dArgs {
            ctx_id: hdr.ctx_id,
            resource_id: read_le_u32(request, 56).unwrap_or(0),
            x: read_le_u32(request, 24).unwrap_or(0),
            y: read_le_u32(request, 28).unwrap_or(0),
            z: read_le_u32(request, 32).unwrap_or(0),
            width: read_le_u32(request, 36).unwrap_or(0),
            height: read_le_u32(request, 40).unwrap_or(0),
            depth: read_le_u32(request, 44).unwrap_or(0),
            offset: read_le_u64(request, 48).unwrap_or(0),
            level: read_le_u32(request, 60).unwrap_or(0),
            stride: read_le_u32(request, 64).unwrap_or(0),
            layer_stride: read_le_u32(request, 68).unwrap_or(0),
        };
        if !self.resource_3d_ids.contains(&args.resource_id)
            || args.width == 0
            || args.height == 0
            || args.depth == 0
        {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        }
        let transferred = self.local_3d_backing.contains_key(&args.resource_id)
            || self
                .backend
                .as_mut()
                .is_some_and(|backend| backend.transfer_3d(args, to_host));
        response_hdr_into(
            out,
            if transferred {
                VIRTIO_GPU_RESP_OK_NODATA
            } else {
                VIRTIO_GPU_RESP_ERR_UNSPEC
            },
            Some(hdr),
        );
    }

    pub(crate) fn submit_3d_into(
        &mut self,
        mem: Option<&dyn GuestMemoryMut>,
        request: &[u8],
        hdr: CtrlHdr3d,
        out: &mut Vec<u8>,
    ) {
        if self.backend.is_none() {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        }
        if request.len() < SUBMIT_3D_LEN {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        }
        let size = read_le_u32(request, 24).unwrap_or(0) as usize;
        // The Windows VirGL driver uses an empty context-0 submit as a queue
        // synchronization no-op. It has no renderer payload or context state to
        // validate, so acknowledge it without calling virglrenderer.
        if size == 0 && hdr.ctx_id == 0 {
            response_hdr_into(out, VIRTIO_GPU_RESP_OK_NODATA, Some(hdr));
            return;
        }
        if size > MAX_SUBMIT_3D_BYTES || request.len().saturating_sub(SUBMIT_3D_LEN) < size {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        }
        let cmdbuf = &request[SUBMIT_3D_LEN..SUBMIT_3D_LEN + size];
        if !self.live_contexts.contains(&hdr.ctx_id) {
            if let Some(mem) = mem {
                match self.try_local_resource_copies(mem, cmdbuf) {
                    LocalResourceCopyResult::Copied { regions } => {
                        self.local_copy_submits = self.local_copy_submits.saturating_add(1);
                        self.submits = self.submits.saturating_add(1);
                        if venus_start_trace_enabled() && self.local_copy_submits == 1 {
                            println!(
                                "venus-start: local pre-context resource_copy_region ctx={} regions={regions}",
                                hdr.ctx_id
                            );
                        }
                        response_hdr_into(out, VIRTIO_GPU_RESP_OK_NODATA, Some(hdr));
                        return;
                    }
                    LocalResourceCopyResult::Invalid => {
                        response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
                        return;
                    }
                    LocalResourceCopyResult::NotApplicable => {}
                }
            }
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        }
        let Some(backend) = self.backend.as_mut() else {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        };
        if !backend.submit_3d(hdr.ctx_id, cmdbuf) {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_UNSPEC, Some(hdr));
            return;
        }
        self.submits = self.submits.saturating_add(1);
        response_hdr_into(out, VIRTIO_GPU_RESP_OK_NODATA, Some(hdr));
    }

    pub(crate) fn try_local_resource_copies(
        &mut self,
        mem: &dyn GuestMemoryMut,
        cmdbuf: &[u8],
    ) -> LocalResourceCopyResult {
        if cmdbuf.is_empty() || cmdbuf.len() % VIRGL_RESOURCE_COPY_REGION_BYTES != 0 {
            return LocalResourceCopyResult::NotApplicable;
        }

        let mut regions = 0usize;
        for command in cmdbuf.chunks_exact(VIRGL_RESOURCE_COPY_REGION_BYTES) {
            let Some(region) = parse_local_resource_copy_region(command) else {
                return LocalResourceCopyResult::NotApplicable;
            };
            let Some(dst_info) = self.resource_3d_info.get(&region.dst_resource_id) else {
                return LocalResourceCopyResult::NotApplicable;
            };
            let Some(src_info) = self.resource_3d_info.get(&region.src_resource_id) else {
                return LocalResourceCopyResult::NotApplicable;
            };
            let Some(dst_backing) = self.local_3d_backing.get(&region.dst_resource_id) else {
                return LocalResourceCopyResult::NotApplicable;
            };
            let Some(src_backing) = self.local_3d_backing.get(&region.src_resource_id) else {
                return LocalResourceCopyResult::NotApplicable;
            };

            let compatible = region.dst_resource_id != region.src_resource_id
                && dst_info.format == src_info.format
                && matches!(dst_info.format, 1..=4)
                && dst_info.depth == 1
                && src_info.depth == 1
                && region.width != 0
                && region.height != 0
                && region
                    .dst_x
                    .checked_add(region.width)
                    .is_some_and(|right| right <= dst_info.width)
                && region
                    .dst_y
                    .checked_add(region.height)
                    .is_some_and(|bottom| bottom <= dst_info.height)
                && region
                    .src_x
                    .checked_add(region.width)
                    .is_some_and(|right| right <= src_info.width)
                && region
                    .src_y
                    .checked_add(region.height)
                    .is_some_and(|bottom| bottom <= src_info.height)
                && backing_covers_32bpp_resource(dst_backing, *dst_info)
                && backing_covers_32bpp_resource(src_backing, *src_info);
            if !compatible {
                return LocalResourceCopyResult::Invalid;
            }
            regions = regions.saturating_add(1);
        }

        for command in cmdbuf.chunks_exact(VIRGL_RESOURCE_COPY_REGION_BYTES) {
            let region = parse_local_resource_copy_region(command)
                .expect("local copy command was validated in the first pass");
            if !self.copy_local_resource_region(mem, region) {
                return LocalResourceCopyResult::Invalid;
            }
        }
        LocalResourceCopyResult::Copied { regions }
    }

    pub(crate) fn copy_local_resource_region(
        &mut self,
        mem: &dyn GuestMemoryMut,
        region: LocalResourceCopyRegion,
    ) -> bool {
        let Some(dst_info) = self.resource_3d_info.get(&region.dst_resource_id).copied() else {
            return false;
        };
        let Some(src_info) = self.resource_3d_info.get(&region.src_resource_id).copied() else {
            return false;
        };
        let Some(dst_backing) = self.local_3d_backing.get(&region.dst_resource_id) else {
            return false;
        };
        let Some(src_backing) = self.local_3d_backing.get(&region.src_resource_id) else {
            return false;
        };
        let Some(row_bytes) = usize::try_from(region.width)
            .ok()
            .and_then(|width| width.checked_mul(4))
        else {
            return false;
        };
        self.local_copy_scratch.resize(row_bytes, 0);

        for row in 0..region.height {
            let Some(src_offset) = resource_32bpp_offset(
                src_info.width,
                region.src_x,
                region.src_y.saturating_add(row),
            ) else {
                return false;
            };
            let Some(dst_offset) = resource_32bpp_offset(
                dst_info.width,
                region.dst_x,
                region.dst_y.saturating_add(row),
            ) else {
                return false;
            };
            if !read_scattered_backing_into(
                mem,
                src_backing,
                src_offset,
                &mut self.local_copy_scratch,
            ) || !write_scattered_backing(mem, dst_backing, dst_offset, &self.local_copy_scratch)
            {
                return false;
            }
        }
        true
    }

    pub(crate) fn resource_create_blob_into(
        &mut self,
        mem: Option<&dyn GuestMemoryMut>,
        request: &[u8],
        hdr: CtrlHdr3d,
        out: &mut Vec<u8>,
    ) {
        if request.len() < RESOURCE_CREATE_BLOB_LEN {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        }
        if self.backend.is_none() {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        }
        let resource_id = read_le_u32(request, 24).unwrap_or(0);
        let blob_mem = read_le_u32(request, 28).unwrap_or(0);
        let blob_flags = read_le_u32(request, 32).unwrap_or(0);
        let nr_entries = read_le_u32(request, 36).unwrap_or(0);
        let blob_id = read_le_u64(request, 40).unwrap_or(0);
        let size = read_le_u64(request, 48).unwrap_or(0);
        if resource_id == 0 || size == 0 || self.blob_resources.contains_key(&resource_id) {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        }
        if blob_mem == VIRTIO_GPU_BLOB_MEM_HOST3D_GUEST {
            venus_start_trace_reject("create_blob", "blob_mem HOST3D_GUEST unsupported");
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        }
        if blob_mem != VIRTIO_GPU_BLOB_MEM_HOST3D && blob_mem != VIRTIO_GPU_BLOB_MEM_GUEST {
            venus_start_trace_reject("create_blob", "blob_mem invalid");
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        }
        let Some(entries_len) = (nr_entries as usize).checked_mul(MEM_ENTRY_LEN) else {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        };
        if request.len().saturating_sub(RESOURCE_CREATE_BLOB_LEN) < entries_len {
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        }
        let mut backing = Vec::with_capacity(nr_entries as usize);
        let mut offset = RESOURCE_CREATE_BLOB_LEN;
        for _ in 0..nr_entries {
            let Some(addr) = read_le_u64(request, offset) else {
                response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
                return;
            };
            let Some(len) = read_le_u32(request, offset + 8) else {
                response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
                return;
            };
            backing.push(BlobMemEntry { addr, len });
            offset += MEM_ENTRY_LEN;
        }
        self.host_iovecs_scratch.clear();
        if blob_mem == VIRTIO_GPU_BLOB_MEM_GUEST {
            let Some(mem) = mem else {
                response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
                return;
            };
            if !resolve_blob_iovecs_into(mem, &backing, &mut self.host_iovecs_scratch) {
                response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
                return;
            }
        }
        if blob_mem == VIRTIO_GPU_BLOB_MEM_HOST3D || blob_mem == VIRTIO_GPU_BLOB_MEM_GUEST {
            let args = CreateBlobArgs {
                ctx_id: hdr.ctx_id,
                resource_id,
                blob_mem,
                blob_flags,
                blob_id,
                size,
                iovecs: &self.host_iovecs_scratch,
            };
            let created = self.backend.as_mut().unwrap().create_blob(args);
            self.host_iovecs_scratch.clear();
            if !created {
                venus_start_trace_reject("create_blob", "backend create_blob failed");
                response_hdr_into(out, VIRTIO_GPU_RESP_ERR_UNSPEC, Some(hdr));
                return;
            }
        }
        // A reused id starts a new lifecycle; stale destroyed-id classification
        // must not label its future late unmaps.
        self.destroyed_blob_mapped_ids.remove(&resource_id);
        self.destroyed_blob_unmapped_ids.remove(&resource_id);
        self.blob_resources.insert(
            resource_id,
            BlobResource {
                blob_mem,
                size,
                mapped: None,
                backing,
            },
        );
        response_hdr_into(out, VIRTIO_GPU_RESP_OK_NODATA, Some(hdr));
    }

    pub(crate) fn resource_map_blob_into(
        &mut self,
        request: &[u8],
        hdr: CtrlHdr3d,
        out: &mut Vec<u8>,
    ) {
        if request.len() < RESOURCE_MAP_BLOB_LEN {
            venus_start_trace_map_blob_reject(
                0,
                u64::MAX,
                0,
                self.shm_window_size,
                "short request",
            );
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        }
        let resource_id = read_le_u32(request, 24).unwrap_or(0);
        let shm_offset = read_le_u64(request, 32).unwrap_or(u64::MAX);
        let Some(resource) = self.blob_resources.get(&resource_id) else {
            venus_start_trace_map_blob_reject(
                resource_id,
                shm_offset,
                0,
                self.shm_window_size,
                "unknown blob resource",
            );
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        };
        if resource.mapped.is_some() || resource.blob_mem != VIRTIO_GPU_BLOB_MEM_HOST3D {
            venus_start_trace_map_blob_reject(
                resource_id,
                shm_offset,
                resource.size,
                self.shm_window_size,
                if resource.mapped.is_some() {
                    "already mapped"
                } else {
                    "blob_mem not HOST3D"
                },
            );
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        }
        let size = resource.size;
        // Validate against the page-rounded footprint the mapping will occupy.
        let rounded_size = round_up_usize(size as usize, HVF_PAGE_SIZE as usize) as u64;
        if !aligned_u64(shm_offset, HVF_PAGE_SIZE)
            || shm_offset
                .checked_add(rounded_size)
                .map_or(true, |end| end > self.shm_window_size)
            || self.interval_overlaps(shm_offset, rounded_size)
        {
            let reason = if !aligned_u64(shm_offset, HVF_PAGE_SIZE) {
                "shm_offset not 16KiB aligned"
            } else if shm_offset
                .checked_add(rounded_size)
                .map_or(true, |end| end > self.shm_window_size)
            {
                "exceeds shm window"
            } else {
                "overlaps mapped interval"
            };
            venus_start_trace_map_blob_reject(
                resource_id,
                shm_offset,
                size,
                self.shm_window_size,
                reason,
            );
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        }
        let Some(backend) = self.backend.as_mut() else {
            venus_start_trace_map_blob_reject(
                resource_id,
                shm_offset,
                size,
                self.shm_window_size,
                "no 3D backend",
            );
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        };
        let Some(mapped) = backend.map_blob(resource_id) else {
            venus_start_trace_map_blob_reject(
                resource_id,
                shm_offset,
                size,
                self.shm_window_size,
                "backend map_blob failed",
            );
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_OUT_OF_MEMORY, Some(hdr));
            return;
        };
        // Guests may create blobs at their own (4 KiB) page granularity while
        // hv_vm_map needs 16 KiB pages. The host allocation backing a Vulkan
        // mapping is vm-page (16 KiB) granular on macOS, so it is safe to map
        // the blob's pages rounded up to the HVF page size as long as the host
        // pointer itself is page-aligned; the guest-visible blob size stays
        // `size`.
        let map_size = rounded_size as usize;
        if mapped.host_ptr.is_null()
            || !aligned_usize(mapped.host_ptr as usize, HVF_PAGE_SIZE as usize)
            || (mapped.size as u64) < size
        {
            venus_start_trace_map_blob_reject(
                resource_id,
                shm_offset,
                size,
                self.shm_window_size,
                if mapped.host_ptr.is_null() {
                    "backend host_ptr null"
                } else if !aligned_usize(mapped.host_ptr as usize, HVF_PAGE_SIZE as usize) {
                    "backend host_ptr unaligned"
                } else {
                    "backend mapping smaller than blob"
                },
            );
            backend.unmap_blob(resource_id);
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_OUT_OF_MEMORY, Some(hdr));
            return;
        }
        let Some(port) = self.shm_port.as_mut() else {
            venus_start_trace_map_blob_reject(
                resource_id,
                shm_offset,
                size,
                self.shm_window_size,
                "no shm map port",
            );
            backend.unmap_blob(resource_id);
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_OUT_OF_MEMORY, Some(hdr));
            return;
        };
        if port.map(mapped.host_ptr, map_size, shm_offset).is_err() {
            venus_start_trace_map_blob_reject(
                resource_id,
                shm_offset,
                size,
                self.shm_window_size,
                "shm port map failed",
            );
            backend.unmap_blob(resource_id);
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_OUT_OF_MEMORY, Some(hdr));
            return;
        }
        if let Some(resource) = self.blob_resources.get_mut(&resource_id) {
            resource.mapped = Some((shm_offset, map_size));
        }
        self.mapped_intervals
            .insert(shm_offset, (map_size as u64, resource_id));
        if venus_start_trace_enabled() {
            println!(
                "venus-start: map_blob OK resource={resource_id} shm_offset={shm_offset:#x} size={size} map_size={map_size} map_info={:#x}",
                mapped.map_info & VIRTIO_GPU_MAP_CACHE_MASK
            );
        }
        response_hdr_into(out, VIRTIO_GPU_RESP_OK_MAP_INFO, Some(hdr));
        out.extend_from_slice(&(mapped.map_info & VIRTIO_GPU_MAP_CACHE_MASK).to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
    }

    pub(crate) fn resource_unmap_blob_into(
        &mut self,
        request: &[u8],
        hdr: CtrlHdr3d,
        out: &mut Vec<u8>,
    ) {
        if request.len() < RESOURCE_UNMAP_BLOB_LEN {
            self.unmap_blob_reject_counts.short_request += 1;
            venus_start_trace_unmap_blob_reject(0, "short_request");
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        }
        let resource_id = read_le_u32(request, 24).unwrap_or(0);
        if !self.blob_resources.contains_key(&resource_id) {
            let reason = if self.destroyed_blob_mapped_ids.contains(&resource_id) {
                self.unmap_blob_reject_counts.destroyed_while_mapped += 1;
                "already_destroyed_was_mapped"
            } else if self.destroyed_blob_unmapped_ids.contains(&resource_id) {
                self.unmap_blob_reject_counts.destroyed_after_unmap += 1;
                "already_destroyed_was_unmapped"
            } else {
                self.unmap_blob_reject_counts.never_created += 1;
                "never_created"
            };
            venus_start_trace_unmap_blob_reject(resource_id, reason);
            response_hdr_into(out, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER, Some(hdr));
            return;
        }
        self.unmap_blob_resource(resource_id);
        response_hdr_into(out, VIRTIO_GPU_RESP_OK_NODATA, Some(hdr));
    }

    pub fn unmap_blob_reject_counts(&self) -> UnmapBlobRejectCounts {
        self.unmap_blob_reject_counts
    }

    pub(crate) fn unmap_blob_resource(&mut self, resource_id: u32) {
        let Some((shm_offset, mapped_size)) = self
            .blob_resources
            .get_mut(&resource_id)
            .and_then(|resource| resource.mapped.take())
        else {
            return;
        };
        if let Some(port) = self.shm_port.as_mut() {
            let _ = port.unmap(shm_offset, mapped_size);
        }
        if let Some(backend) = self.backend.as_mut() {
            backend.unmap_blob(resource_id);
        }
        self.mapped_intervals.remove(&shm_offset);
    }

    pub(crate) fn unmap_all_blobs(&mut self) {
        self.blob_unmap_ids_scratch.clear();
        self.blob_unmap_ids_scratch
            .extend(self.blob_resources.keys().copied());
        let mut ids = std::mem::take(&mut self.blob_unmap_ids_scratch);
        for resource_id in ids.drain(..) {
            self.unmap_blob_resource(resource_id);
        }
        self.blob_unmap_ids_scratch = ids;
    }

    pub(crate) fn interval_overlaps(&self, start: u64, size: u64) -> bool {
        let Some(end) = start.checked_add(size) else {
            return true;
        };
        self.mapped_intervals
            .iter()
            .any(|(other_start, (other_size, _))| {
                let other_end = other_start.saturating_add(*other_size);
                start < other_end && *other_start < end
            })
    }

    pub(crate) fn resource_exists(&self, resource_id: u32) -> bool {
        resource_id != 0
            && (self.resource_2d_ids.contains(&resource_id)
                || self.resource_3d_ids.contains(&resource_id)
                || self.blob_resources.contains_key(&resource_id))
    }
}
