//! Split out of venus_backend.rs to keep files under 500 lines.

#![allow(non_camel_case_types)]
#![allow(dead_code)]
use super::*;

use std::{
    collections::BTreeMap,
    ffi::CString,
    os::raw::{c_int, c_void},
    ptr,
    sync::{Mutex, OnceLock},
};

use crate::virtio_gpu_3d::{
    CapsetInfo, CompletedFence, Create3dArgs, CreateBlobArgs, MappedBlob, ScanoutMappedBlob,
    Transfer3dArgs, VirtioGpu3dBackend,
};

/// `BRIDGEVM_TRACE_VENUS_START=1`: print an FFI capset probe only when its
/// result changed since the last probe of that capset id. `capset_count()`
/// runs on every guest config read, so the interesting signal is the value
/// (especially the venus capset flipping 0 -> nonzero after renderer init, or
/// staying 0 at KMD config-read time), not the poll itself.
pub(crate) fn venus_start_trace_capset_count_changed(
    capset_id: u32,
    max_version: u32,
    max_size: u32,
) -> bool {
    use std::sync::atomic::{AtomicU64, Ordering};
    if !crate::virtio_gpu_trace::venus_start_trace_enabled() {
        return false;
    }
    static LAST: OnceLock<Mutex<BTreeMap<u32, (u32, u32)>>> = OnceLock::new();
    let last = LAST.get_or_init(|| Mutex::new(BTreeMap::new()));
    static PROBES: AtomicU64 = AtomicU64::new(0);
    PROBES.fetch_add(1, Ordering::Relaxed);
    let mut last = match last.lock() {
        Ok(guard) => guard,
        Err(_) => return false,
    };
    let value = (max_version, max_size);
    if last.get(&capset_id) == Some(&value) {
        return false;
    }
    last.insert(capset_id, value);
    true
}

pub(crate) fn venus_start_trace_ffi_reject(what: &str, capset_id: u32, version: u32, reason: &str) {
    if crate::virtio_gpu_trace::venus_start_trace_enabled() {
        println!(
            "venus-start: ffi {what} capset_id={capset_id} version={version} REJECT: {reason}"
        );
    }
}

impl VirtioGpu3dBackend for VenusBackend {
    fn capset_count(&self) -> u32 {
        host_gl::rebind_last_context();
        let capset_ids: &[u32] = match self.protocol {
            VirtioGpuRendererProtocol::Venus => &[
                VIRTIO_GPU_CAPSET_VENUS,
                VIRTIO_GPU_CAPSET_VIRGL,
                VIRTIO_GPU_CAPSET_VIRGL2,
            ],
            VirtioGpuRendererProtocol::Virgl => {
                &[VIRTIO_GPU_CAPSET_VIRGL, VIRTIO_GPU_CAPSET_VIRGL2]
            }
        };
        let count = capset_ids
            .iter()
            .filter(|capset_id| {
                let mut max_version = 0u32;
                let mut max_size = 0u32;
                unsafe {
                    virgl_renderer_get_cap_set(**capset_id, &mut max_version, &mut max_size);
                }
                if venus_start_trace_capset_count_changed(**capset_id, max_version, max_size) {
                    println!(
                        "venus-start: ffi get_cap_set id={} max_version={max_version} max_size={max_size} (capset_count probe, value changed)",
                        **capset_id
                    );
                }
                max_size != 0
            })
            .count() as u32;
        count
    }

    fn capset_info(&mut self, capset_index: u32) -> Option<CapsetInfo> {
        host_gl::rebind_last_context();
        let capset_id = self.protocol.capset_id_for_index(capset_index)?;
        let mut max_version = 0u32;
        let mut max_size = 0u32;
        unsafe {
            virgl_renderer_get_cap_set(capset_id, &mut max_version, &mut max_size);
        }
        if crate::virtio_gpu_trace::venus_start_trace_enabled() {
            println!(
                "venus-start: ffi capset_info index={capset_index} -> id={capset_id} max_version={max_version} max_size={max_size}"
            );
        }
        (max_size != 0).then_some(CapsetInfo {
            capset_id,
            max_version,
            max_size,
        })
    }

    fn capset(&mut self, capset_id: u32, version: u32) -> Option<Vec<u8>> {
        let mut capset = Vec::new();
        self.capset_into(capset_id, version, &mut capset)
            .then_some(capset)
    }

    fn capset_into(&mut self, capset_id: u32, version: u32, out: &mut Vec<u8>) -> bool {
        host_gl::rebind_last_context();
        if !self.protocol.supports_capset_id(capset_id) {
            venus_start_trace_ffi_reject(
                "capset_into",
                capset_id,
                version,
                "unsupported capset_id",
            );
            return false;
        }
        let mut max_version = 0u32;
        let mut max_size = 0u32;
        unsafe {
            virgl_renderer_get_cap_set(capset_id, &mut max_version, &mut max_size);
        }
        if max_size == 0 {
            venus_start_trace_ffi_reject("capset_into", capset_id, version, "max_size == 0");
            return false;
        }
        if version > max_version {
            venus_start_trace_ffi_reject(
                "capset_into",
                capset_id,
                version,
                "version > max_version",
            );
            return false;
        }
        let start = out.len();
        out.resize(start + max_size as usize, 0);
        unsafe {
            virgl_renderer_fill_caps(
                capset_id,
                version,
                out[start..].as_mut_ptr().cast::<c_void>(),
            );
        }
        true
    }

    fn ctx_create(&mut self, ctx_id: u32, context_init: u32, name: &[u8]) -> bool {
        host_gl::rebind_last_context();
        let default_name = format!("bridgevm-{}", self.protocol.label());
        let name = CString::new(name).unwrap_or_else(|_| CString::new(default_name).unwrap());
        let requested = context_init & VIRGL_RENDERER_CONTEXT_FLAG_CAPSET_ID_MASK;
        // When CONTEXT_INIT was not negotiated, virtio-gpu defines the context
        // as the renderer's default VirGL context. virglrenderer expresses that
        // default as VIRGL2; passing a literal zero to the newer flags API is an
        // invalid capset and returns EINVAL.
        let flags = if self.protocol == VirtioGpuRendererProtocol::Virgl && requested == 0 {
            VIRTIO_GPU_CAPSET_VIRGL2
        } else {
            requested
        };
        let ret = unsafe {
            virgl_renderer_context_create_with_flags(
                ctx_id,
                flags,
                name.as_bytes().len() as u32,
                name.as_ptr(),
            )
        };
        if ret == 0 {
            self.contexts.push(ctx_id);
            self.outstanding_fences.entry(ctx_id).or_default();
            true
        } else {
            eprintln!(
                "{}: context_create_with_flags ctx={ctx_id} ret={ret}",
                self.protocol.label()
            );
            false
        }
    }

    fn ctx_destroy(&mut self, ctx_id: u32) {
        host_gl::rebind_last_context();
        unsafe {
            virgl_renderer_context_destroy(ctx_id);
        }
        self.contexts.retain(|ctx| *ctx != ctx_id);
        self.outstanding_fences.remove(&ctx_id);
    }

    fn ctx_attach_resource(&mut self, ctx_id: u32, resource_id: u32) {
        host_gl::rebind_last_context();
        unsafe {
            virgl_renderer_ctx_attach_resource(ctx_id as c_int, resource_id as c_int);
        }
    }

    fn ctx_detach_resource(&mut self, ctx_id: u32, resource_id: u32) {
        host_gl::rebind_last_context();
        unsafe {
            virgl_renderer_ctx_detach_resource(ctx_id as c_int, resource_id as c_int);
        }
    }

    fn supports_legacy_3d_resources(&self) -> bool {
        true
    }

    fn create_3d(&mut self, args: Create3dArgs) -> bool {
        host_gl::rebind_last_context();
        // Resource creation is not tied to a guest renderer context.  On CGL,
        // the current context is thread-local while a serialized virtio-gpu
        // notification may arrive on any vCPU thread, so make ctx0 current
        // before virglrenderer issues glGenBuffers/glBufferData or texture
        // allocation calls.
        unsafe {
            virgl_renderer_force_ctx_0();
        }
        let mut create = virgl_renderer_resource_create_args {
            handle: args.resource_id,
            target: args.target,
            format: args.format,
            bind: args.bind,
            width: args.width,
            height: args.height,
            depth: args.depth,
            array_size: args.array_size,
            last_level: args.last_level,
            nr_samples: args.nr_samples,
            flags: args.flags,
        };
        let ret = unsafe { virgl_renderer_resource_create(&mut create, ptr::null_mut(), 0) };
        if ret == 0 {
            self.resources.insert(args.resource_id);
        } else {
            eprintln!(
                "{}: resource_create_3d res={} target={} format={} bind={:#x} size={}x{}x{} ret={ret}",
                self.protocol.label(),
                args.resource_id,
                args.target,
                args.format,
                args.bind,
                args.width,
                args.height,
                args.depth
            );
        }
        ret == 0
    }

    fn attach_backing(
        &mut self,
        resource_id: u32,
        host_iovecs: &[crate::virtio_gpu_3d::BlobHostIovec],
    ) -> bool {
        host_gl::rebind_last_context();
        if !self.resources.contains(&resource_id)
            || self.resource_iovecs.contains_key(&resource_id)
            || host_iovecs.is_empty()
        {
            return false;
        }
        let iovecs = host_iovecs
            .iter()
            .map(|entry| iovec {
                iov_base: entry.host_ptr.cast::<c_void>(),
                iov_len: entry.len,
            })
            .collect::<Vec<_>>();
        self.resource_iovecs.insert(resource_id, iovecs);
        let stored = self.resource_iovecs.get_mut(&resource_id).unwrap();
        let ret = unsafe {
            virgl_renderer_resource_attach_iov(
                resource_id as c_int,
                stored.as_mut_ptr(),
                stored.len() as c_int,
            )
        };
        if ret != 0 {
            self.resource_iovecs.remove(&resource_id);
            eprintln!(
                "{}: resource_attach_iov res={resource_id} count={} ret={ret}",
                self.protocol.label(),
                host_iovecs.len()
            );
        }
        ret == 0
    }

    fn detach_backing(&mut self, resource_id: u32) -> bool {
        host_gl::rebind_last_context();
        if !self.resource_iovecs.contains_key(&resource_id) {
            return false;
        }
        let mut detached = ptr::null_mut();
        let mut count = 0;
        unsafe {
            virgl_renderer_resource_detach_iov(resource_id as c_int, &mut detached, &mut count);
        }
        self.resource_iovecs.remove(&resource_id);
        true
    }

    fn transfer_3d(&mut self, args: Transfer3dArgs, to_host: bool) -> bool {
        host_gl::rebind_last_context();
        let mut transfer_box = virgl_box {
            x: args.x,
            y: args.y,
            z: args.z,
            width: args.width,
            height: args.height,
            depth: args.depth,
        };
        let ret = unsafe {
            if to_host {
                virgl_renderer_transfer_write_iov(
                    args.resource_id,
                    args.ctx_id,
                    args.level as c_int,
                    args.stride,
                    args.layer_stride,
                    &mut transfer_box,
                    args.offset,
                    ptr::null_mut(),
                    0,
                )
            } else {
                virgl_renderer_transfer_read_iov(
                    args.resource_id,
                    args.ctx_id,
                    args.level,
                    args.stride,
                    args.layer_stride,
                    &mut transfer_box,
                    args.offset,
                    ptr::null_mut(),
                    0,
                )
            }
        };
        if ret != 0 {
            eprintln!(
                "{}: transfer_3d direction={} ctx={} res={} level={} box={}x{}x{}@{},{},{} ret={ret}",
                self.protocol.label(),
                if to_host { "to-host" } else { "from-host" },
                args.ctx_id,
                args.resource_id,
                args.level,
                args.width,
                args.height,
                args.depth,
                args.x,
                args.y,
                args.z
            );
        }
        ret == 0
    }

    fn submit_3d(&mut self, ctx_id: u32, cmdbuf: &[u8]) -> bool {
        host_gl::rebind_last_context();
        let ndw = cmdbuf.len().div_ceil(4);
        let ret = if cmdbuf.is_empty() {
            unsafe { virgl_renderer_submit_cmd(ptr::null_mut(), ctx_id as c_int, 0) }
        } else {
            unsafe {
                virgl_renderer_submit_cmd(
                    cmdbuf.as_ptr() as *mut c_void,
                    ctx_id as c_int,
                    ndw as c_int,
                )
            }
        };
        if ret != 0 {
            eprintln!(
                "{}: submit_cmd ctx={ctx_id} bytes={} ret={ret}",
                self.protocol.label(),
                cmdbuf.len()
            );
        }
        // QEMU's legacy virgl command path submits the buffer for renderer
        // diagnostics but does not turn a vrend context error into a virtio
        // command error. Windows relies on that tolerant wire contract while
        // probing optional draw paths. Keep Venus strict; match QEMU for the
        // legacy VirGL protocol and retain the host-side error above.
        ret == 0 || self.protocol == VirtioGpuRendererProtocol::Virgl
    }

    fn create_blob(&mut self, args: CreateBlobArgs<'_>) -> bool {
        host_gl::rebind_last_context();
        let iovecs = args
            .iovecs
            .iter()
            .map(|entry| iovec {
                iov_base: entry.host_ptr.cast::<c_void>(),
                iov_len: entry.len,
            })
            .collect::<Vec<_>>();
        if !iovecs.is_empty() {
            self.resource_iovecs.insert(args.resource_id, iovecs);
        }
        let (iovecs, num_iovs) = self
            .resource_iovecs
            .get(&args.resource_id)
            .map_or((ptr::null(), 0), |iovecs| {
                (iovecs.as_ptr(), iovecs.len() as u32)
            });
        let create = virgl_renderer_resource_create_blob_args {
            res_handle: args.resource_id,
            ctx_id: args.ctx_id,
            blob_mem: args.blob_mem,
            blob_flags: args.blob_flags,
            blob_id: args.blob_id,
            size: args.size,
            iovecs,
            num_iovs,
        };
        let ret = unsafe { virgl_renderer_resource_create_blob(&create) };
        if ret != 0 {
            self.resource_iovecs.remove(&args.resource_id);
            eprintln!(
                "{}: resource_create_blob ctx={} res={} blob_mem={} blob_id={} size={} ret={ret}",
                self.protocol.label(),
                args.ctx_id,
                args.resource_id,
                args.blob_mem,
                args.blob_id,
                args.size
            );
        }
        if ret == 0 {
            self.resources.insert(args.resource_id);
        }
        ret == 0
    }

    fn map_blob(&mut self, resource_id: u32) -> Option<MappedBlob> {
        host_gl::rebind_last_context();
        let mapped = self.map_resource_ref(resource_id)?;
        Some(MappedBlob {
            host_ptr: mapped.host_ptr,
            size: mapped.size,
            map_info: mapped.map_info,
        })
    }

    fn unmap_blob(&mut self, resource_id: u32) {
        host_gl::rebind_last_context();
        self.unmap_resource_ref(resource_id);
    }

    fn scanout_map(&mut self, resource_id: u32) -> Option<ScanoutMappedBlob> {
        host_gl::rebind_last_context();
        let mapped = self.map_resource_ref(resource_id)?;
        Some(ScanoutMappedBlob {
            host_ptr: mapped.host_ptr.cast_const(),
            size: mapped.size,
        })
    }

    fn scanout_unmap(&mut self, resource_id: u32) {
        host_gl::rebind_last_context();
        self.unmap_resource_ref(resource_id);
    }

    fn scanout_read(&mut self, resource_id: u32, width: u32, height: u32, out: &mut [u8]) -> bool {
        host_gl::rebind_last_context();
        let Some(required) = (width as usize)
            .checked_mul(height as usize)
            .and_then(|pixels| pixels.checked_mul(4))
        else {
            return false;
        };
        if out.len() < required {
            return false;
        }
        let mut transfer_box = virgl_box {
            x: 0,
            y: 0,
            z: 0,
            width,
            height,
            depth: 1,
        };
        let mut iov = iovec {
            iov_base: out.as_mut_ptr().cast(),
            iov_len: required,
        };
        // scanout_read is only used for legacy RESOURCE_CREATE_3D objects;
        // select vrend's CGL context even when the same renderer instance also
        // hosts Venus blob resources.
        unsafe { virgl_renderer_force_ctx_0() };
        let ret = unsafe {
            virgl_renderer_transfer_read_iov(
                resource_id,
                0,
                0,
                width.saturating_mul(4),
                width.saturating_mul(height).saturating_mul(4),
                &mut transfer_box,
                0,
                &mut iov,
                1,
            )
        };
        if ret != 0 {
            eprintln!(
                "{}: scanout_read res={resource_id} size={width}x{height} ret={ret}",
                self.protocol.label()
            );
        }
        ret == 0
    }

    fn scanout_blit_iosurface(&mut self, resource_id: u32, width: u32, height: u32) -> Option<u32> {
        host_gl::rebind_last_context();
        // Same ctx0/CGL discipline as scanout_read: the blit runs in vrend's
        // GL context on whichever vCPU thread services the flush.
        unsafe { virgl_renderer_force_ctx_0() };
        let mut surface_id: u32 = 0;
        let ret = unsafe {
            virgl_renderer_bridgevm_scanout_blit_iosurface(
                resource_id,
                width,
                height,
                &mut surface_id,
            )
        };
        (ret == 0).then_some(surface_id)
    }

    fn scanout_iosurface_checksum(&mut self) -> Option<u64> {
        host_gl::rebind_last_context();
        unsafe { virgl_renderer_force_ctx_0() };
        let mut checksum: u64 = 0;
        let ret = unsafe { virgl_renderer_bridgevm_scanout_iosurface_checksum(&mut checksum) };
        (ret == 0).then_some(checksum)
    }

    fn scanout_iosurface_dump(&mut self, path: &std::path::Path) -> bool {
        host_gl::rebind_last_context();
        let Ok(cpath) = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()) else {
            return false;
        };
        unsafe { virgl_renderer_force_ctx_0() };
        unsafe { virgl_renderer_bridgevm_scanout_iosurface_dump(cpath.as_ptr()) == 0 }
    }

    fn destroy_resource(&mut self, resource_id: u32) {
        host_gl::rebind_last_context();
        while self.mapped_resources.contains_key(&resource_id) {
            self.unmap_resource_ref(resource_id);
        }
        // Resource destruction is global too and may delete a shared GL object.
        // Rebind ctx0 for the same thread-local CGL reason as create_3d().
        unsafe {
            virgl_renderer_force_ctx_0();
        }
        unsafe {
            virgl_renderer_resource_unref(resource_id);
        }
        self.resource_iovecs.remove(&resource_id);
        self.resources.remove(&resource_id);
    }

    fn create_fence(&mut self, ctx_id: u32, ring_idx: u8, fence_id: u64) -> bool {
        host_gl::rebind_last_context();
        if ring_idx != 0 {
            eprintln!(
                "{}: rejecting unbound fence ring ctx={ctx_id} ring={ring_idx} fence={fence_id}",
                self.protocol.label()
            );
            return false;
        }
        let ret =
            unsafe { virgl_renderer_context_create_fence(ctx_id, 0, ring_idx.into(), fence_id) };
        if ret != 0 {
            eprintln!(
                "{}: context_create_fence ctx={ctx_id} ring={ring_idx} fence={fence_id} ret={ret}",
                self.protocol.label()
            );
        }
        if ret == 0 {
            *self.outstanding_fences.entry(ctx_id).or_default() += 1;
        }
        ret == 0
    }

    fn poll_fences(&mut self) {
        host_gl::rebind_last_context();
        // Poll EVERY live context, not just those with outstanding virtqueue
        // fences: venus guests mostly synchronize via renderer-side fence
        // FEEDBACK slots (Mesa spins on a shmem slot the renderer writes at
        // retire time), which never involve virtqueue fences at all. On macOS
        // there is no sync thread (no eventfd), so this poll is the only
        // thing that retires renderer fences and writes those slots — gating
        // it on outstanding_fences left vkWaitForFences spinning forever.
        for &ctx_id in &self.contexts {
            unsafe {
                virgl_renderer_context_poll(ctx_id);
            }
        }
    }

    fn drain_completed_fences_into(&mut self, out: &mut Vec<CompletedFence>) {
        let start = out.len();
        out.append(&mut self.shared.lock().unwrap().completed);
        for fence in &out[start..] {
            if let Some(outstanding) = self.outstanding_fences.get_mut(&fence.ctx_id) {
                *outstanding = outstanding.saturating_sub(1);
            }
        }
    }

    fn reset(&mut self) {
        host_gl::rebind_last_context();
        self.resource_ids_scratch.clear();
        self.resource_ids_scratch
            .extend(self.mapped_resources.keys().copied());
        let mut resource_ids = std::mem::take(&mut self.resource_ids_scratch);
        for resource_id in resource_ids.drain(..) {
            while self.mapped_resources.contains_key(&resource_id) {
                self.unmap_resource_ref(resource_id);
            }
        }
        self.resource_ids_scratch = resource_ids;
        self.resource_ids_scratch.clear();
        self.resource_ids_scratch
            .extend(self.resources.iter().copied());
        let mut resource_ids = std::mem::take(&mut self.resource_ids_scratch);
        for resource_id in resource_ids.drain(..) {
            unsafe {
                virgl_renderer_resource_unref(resource_id);
            }
            self.resource_iovecs.remove(&resource_id);
        }
        self.resource_ids_scratch = resource_ids;
        self.resources.clear();
        for ctx_id in std::mem::take(&mut self.contexts) {
            unsafe {
                virgl_renderer_context_destroy(ctx_id);
            }
        }
        self.outstanding_fences.clear();
        self.shared.lock().unwrap().completed.clear();
    }
}
