//! Split out of venus_backend.rs to keep files under 500 lines.

#![allow(non_camel_case_types)]
#![allow(dead_code)]
use super::*;

use std::{
    collections::{BTreeMap, BTreeSet},
    env,
    os::raw::{c_char, c_int, c_uint, c_void},
    sync::{
        mpsc::{self, Sender},
        Arc, Mutex, OnceLock,
    },
    thread::{self, JoinHandle},
};

use crate::virtio_gpu_3d::{
    CapsetInfo, CompletedFence, Create3dArgs, CreateBlobArgs, MappedBlob, ScanoutMappedBlob,
    Transfer3dArgs, VirtioGpu3dBackend,
};

pub(crate) type virgl_renderer_gl_context = *mut c_void;

#[repr(C)]
pub struct virgl_renderer_gl_ctx_param {
    pub version: c_int,
    pub shared: bool,
    pub major_ver: c_int,
    pub minor_ver: c_int,
    pub compat_ctx: c_int,
}

pub(crate) const VIRGL_RENDERER_CALLBACKS_VERSION: c_int = 4;

#[repr(C)]
pub struct virgl_renderer_callbacks {
    pub version: c_int,
    pub write_fence: Option<extern "C" fn(cookie: *mut c_void, fence: u32)>,
    pub create_gl_context: Option<
        extern "C" fn(
            cookie: *mut c_void,
            scanout_idx: c_int,
            param: *mut virgl_renderer_gl_ctx_param,
        ) -> virgl_renderer_gl_context,
    >,
    pub destroy_gl_context:
        Option<extern "C" fn(cookie: *mut c_void, ctx: virgl_renderer_gl_context)>,
    pub make_current: Option<
        extern "C" fn(
            cookie: *mut c_void,
            scanout_idx: c_int,
            ctx: virgl_renderer_gl_context,
        ) -> c_int,
    >,
    pub get_drm_fd: Option<extern "C" fn(cookie: *mut c_void) -> c_int>,
    pub write_context_fence:
        Option<extern "C" fn(cookie: *mut c_void, ctx_id: u32, ring_idx: u32, fence_id: u64)>,
    pub get_server_fd: Option<extern "C" fn(cookie: *mut c_void, version: u32) -> c_int>,
    pub get_egl_display: Option<extern "C" fn(cookie: *mut c_void) -> *mut c_void>,
}

pub(crate) const VIRGL_RENDERER_USE_EXTERNAL_BLOB: c_int = 1 << 5;
pub(crate) const VIRGL_RENDERER_THREAD_SYNC: c_int = 2;
pub(crate) const VIRGL_RENDERER_VENUS: c_int = 1 << 6;
pub(crate) const VIRGL_RENDERER_NO_VIRGL: c_int = 1 << 7;
pub(crate) const VIRGL_RENDERER_ASYNC_FENCE_CB: c_int = 1 << 8;
pub(crate) const VIRGL_RENDERER_RENDER_SERVER: c_int = 1 << 9;
pub(crate) const VIRGL_RENDERER_USE_GUEST_VRAM: c_int = 1 << 14;
pub(crate) const VIRGL_RENDERER_CONTEXT_FLAG_CAPSET_ID_MASK: u32 = 0xff;
pub(crate) const VIRTIO_GPU_CAPSET_VIRGL: u32 = 1;
pub(crate) const VIRTIO_GPU_CAPSET_VIRGL2: u32 = 2;
pub(crate) const VIRTIO_GPU_CAPSET_VENUS: u32 = 4;

#[repr(C)]
pub struct virgl_renderer_resource_create_blob_args {
    pub res_handle: u32,
    pub ctx_id: u32,
    pub blob_mem: u32,
    pub blob_flags: u32,
    pub blob_id: u64,
    pub size: u64,
    pub iovecs: *const iovec,
    pub num_iovs: u32,
}

#[repr(C)]
pub struct virgl_renderer_resource_create_args {
    pub handle: u32,
    pub target: u32,
    pub format: u32,
    pub bind: u32,
    pub width: u32,
    pub height: u32,
    pub depth: u32,
    pub array_size: u32,
    pub last_level: u32,
    pub nr_samples: u32,
    pub flags: u32,
}

#[repr(C)]
pub struct virgl_box {
    pub x: u32,
    pub y: u32,
    pub z: u32,
    pub width: u32,
    pub height: u32,
    pub depth: u32,
}

#[repr(C)]
pub struct iovec {
    pub iov_base: *mut c_void,
    pub iov_len: usize,
}

unsafe impl Send for iovec {}

unsafe extern "C" {
    pub(super) fn virgl_renderer_init(
        cookie: *mut c_void,
        flags: c_int,
        cb: *mut virgl_renderer_callbacks,
    ) -> c_int;
    pub(super) fn virgl_renderer_get_cap_set(set: u32, max_ver: *mut u32, max_size: *mut u32);
    pub(super) fn virgl_renderer_fill_caps(set: u32, version: u32, caps: *mut c_void);
    pub(super) fn virgl_renderer_context_create_with_flags(
        ctx_id: u32,
        ctx_flags: u32,
        nlen: u32,
        name: *const c_char,
    ) -> c_int;
    pub(super) fn virgl_renderer_context_destroy(handle: u32);
    pub(super) fn virgl_renderer_force_ctx_0();
    pub(super) fn virgl_renderer_ctx_attach_resource(ctx_id: c_int, res_handle: c_int);
    pub(super) fn virgl_renderer_ctx_detach_resource(ctx_id: c_int, res_handle: c_int);
    pub(super) fn virgl_renderer_submit_cmd(
        buffer: *mut c_void,
        ctx_id: c_int,
        ndw: c_int,
    ) -> c_int;
    pub(super) fn virgl_renderer_resource_create(
        args: *mut virgl_renderer_resource_create_args,
        iov: *mut iovec,
        num_iovs: u32,
    ) -> c_int;
    pub(super) fn virgl_renderer_resource_attach_iov(
        res_handle: c_int,
        iov: *mut iovec,
        num_iovs: c_int,
    ) -> c_int;
    pub(super) fn virgl_renderer_resource_detach_iov(
        res_handle: c_int,
        iov: *mut *mut iovec,
        num_iovs: *mut c_int,
    );
    pub(super) fn virgl_renderer_transfer_write_iov(
        handle: u32,
        ctx_id: u32,
        level: c_int,
        stride: u32,
        layer_stride: u32,
        transfer_box: *mut virgl_box,
        offset: u64,
        iov: *mut iovec,
        iovec_cnt: c_uint,
    ) -> c_int;
    pub(super) fn virgl_renderer_transfer_read_iov(
        handle: u32,
        ctx_id: u32,
        level: u32,
        stride: u32,
        layer_stride: u32,
        transfer_box: *mut virgl_box,
        offset: u64,
        iov: *mut iovec,
        iovec_cnt: c_int,
    ) -> c_int;
    pub(super) fn virgl_renderer_bridgevm_scanout_blit_iosurface(
        res_handle: u32,
        width: u32,
        height: u32,
        out_surface_id: *mut u32,
    ) -> c_int;
    pub(super) fn virgl_renderer_bridgevm_scanout_iosurface_checksum(
        out_checksum: *mut u64,
    ) -> c_int;
    pub(super) fn virgl_renderer_bridgevm_scanout_iosurface_dump(path: *const c_char) -> c_int;
    pub(super) fn virgl_renderer_context_create_fence(
        ctx_id: u32,
        flags: u32,
        ring_idx: u32,
        fence_id: u64,
    ) -> c_int;
    pub(super) fn virgl_renderer_context_poll(ctx_id: u32);
    pub(super) fn virgl_renderer_resource_create_blob(
        args: *const virgl_renderer_resource_create_blob_args,
    ) -> c_int;
    pub(super) fn virgl_renderer_resource_map(
        res_handle: c_uint,
        map: *mut *mut c_void,
        out_size: *mut u64,
    ) -> c_int;
    pub(super) fn virgl_renderer_resource_unmap(res_handle: c_uint) -> c_int;
    pub(super) fn virgl_renderer_resource_get_map_info(
        res_handle: c_uint,
        map_info: *mut u32,
    ) -> c_int;
    pub(super) fn virgl_renderer_resource_unref(res_handle: c_uint);
}

#[derive(Default)]
pub(crate) struct VenusShared {
    pub(crate) completed: Vec<CompletedFence>,
}

pub(crate) static INIT: OnceLock<Result<VirtioGpuRendererProtocol, String>> = OnceLock::new();
pub(crate) static SHARED: OnceLock<Arc<Mutex<VenusShared>>> = OnceLock::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VirtioGpuRendererProtocol {
    Venus,
    Virgl,
}

impl VirtioGpuRendererProtocol {
    pub fn from_env() -> Result<Self, String> {
        let value =
            env::var("BRIDGEVM_VIRTIO_GPU_3D_PROTOCOL").unwrap_or_else(|_| "venus".to_string());
        Self::parse(&value).ok_or_else(|| {
            format!("BRIDGEVM_VIRTIO_GPU_3D_PROTOCOL must be 'venus' or 'virgl', got: {value}")
        })
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "" | "venus" => Some(Self::Venus),
            "virgl" => Some(Self::Virgl),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Venus => "venus",
            Self::Virgl => "virgl",
        }
    }

    pub(crate) fn capset_id_for_index(self, capset_index: u32) -> Option<u32> {
        match (self, capset_index) {
            (Self::Venus, 0) => Some(VIRTIO_GPU_CAPSET_VENUS),
            // The Windows Venus WDDM miniport creates a second VirGL context
            // named `virgl-shadow-win32` for kernel-mode present/copy work.
            // Keep Venus first for the Vulkan ICD, then expose both legacy
            // capsets so that shadow context is actually initialized.
            (Self::Venus, 1) => Some(VIRTIO_GPU_CAPSET_VIRGL),
            (Self::Venus, 2) => Some(VIRTIO_GPU_CAPSET_VIRGL2),
            (Self::Virgl, 0) => Some(VIRTIO_GPU_CAPSET_VIRGL),
            (Self::Virgl, 1) => Some(VIRTIO_GPU_CAPSET_VIRGL2),
            _ => None,
        }
    }

    pub(crate) fn supports_capset_id(self, capset_id: u32) -> bool {
        match self {
            Self::Venus => matches!(
                capset_id,
                VIRTIO_GPU_CAPSET_VENUS | VIRTIO_GPU_CAPSET_VIRGL | VIRTIO_GPU_CAPSET_VIRGL2
            ),
            Self::Virgl => {
                capset_id == VIRTIO_GPU_CAPSET_VIRGL || capset_id == VIRTIO_GPU_CAPSET_VIRGL2
            }
        }
    }

    pub(crate) fn init_flags(self) -> c_int {
        match self {
            Self::Venus => {
                VIRGL_RENDERER_USE_EXTERNAL_BLOB
                    | VIRGL_RENDERER_VENUS
                    | VIRGL_RENDERER_RENDER_SERVER
                    | VIRGL_RENDERER_USE_GUEST_VRAM
                    | VIRGL_RENDERER_THREAD_SYNC
                    | VIRGL_RENDERER_ASYNC_FENCE_CB
            }
            Self::Virgl => {
                VIRGL_RENDERER_USE_EXTERNAL_BLOB
                    | VIRGL_RENDERER_USE_GUEST_VRAM
                    | VIRGL_RENDERER_THREAD_SYNC
                    | VIRGL_RENDERER_ASYNC_FENCE_CB
            }
        }
    }
}

pub struct VenusBackend {
    pub(crate) protocol: VirtioGpuRendererProtocol,
    pub(crate) shared: Arc<Mutex<VenusShared>>,
    pub(crate) contexts: Vec<u32>,
    pub(crate) outstanding_fences: BTreeMap<u32, usize>,
    pub(crate) mapped_resources: BTreeMap<u32, VenusMappedResource>,
    pub(crate) resources: BTreeSet<u32>,
    // virglrenderer retains the iovec-array pointer for the resource lifetime.
    // Each array therefore needs stable, per-resource storage rather than a
    // reusable scratch vector.
    pub(crate) resource_iovecs: BTreeMap<u32, Vec<iovec>>,
    pub(crate) resource_ids_scratch: Vec<u32>,
}

impl Clone for VenusBackend {
    fn clone(&self) -> Self {
        Self {
            protocol: self.protocol,
            shared: self.shared.clone(),
            contexts: self.contexts.clone(),
            outstanding_fences: self.outstanding_fences.clone(),
            mapped_resources: self.mapped_resources.clone(),
            resources: BTreeSet::new(),
            resource_iovecs: BTreeMap::new(),
            resource_ids_scratch: Vec::new(),
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct VenusMappedResource {
    pub(crate) host_ptr: *mut u8,
    pub(crate) size: usize,
    pub(crate) map_info: u32,
    pub(crate) refs: usize,
}

unsafe impl Send for VenusMappedResource {}

impl VenusBackend {
    pub fn new() -> Result<Self, String> {
        Self::new_for_protocol(VirtioGpuRendererProtocol::from_env()?)
    }

    pub fn new_for_protocol(protocol: VirtioGpuRendererProtocol) -> Result<Self, String> {
        set_env_defaults();
        let shared = SHARED
            .get_or_init(|| Arc::new(Mutex::new(VenusShared::default())))
            .clone();
        let init = INIT.get_or_init(|| init_renderer(shared.clone(), protocol));
        match init {
            Ok(active) if *active == protocol => {}
            Ok(active) => {
                return Err(format!(
                    "virglrenderer already initialized for protocol={}, cannot switch to protocol={}",
                    active.label(),
                    protocol.label()
                ));
            }
            Err(error) => return Err(error.clone()),
        }
        Ok(Self {
            protocol,
            shared,
            contexts: Vec::new(),
            outstanding_fences: BTreeMap::new(),
            mapped_resources: BTreeMap::new(),
            resources: BTreeSet::new(),
            resource_iovecs: BTreeMap::new(),
            resource_ids_scratch: Vec::new(),
        })
    }

    pub fn protocol(&self) -> VirtioGpuRendererProtocol {
        self.protocol
    }
}

pub(crate) type VenusWorkerJob = Box<dyn FnOnce(&mut VenusBackend) + Send + 'static>;

pub(crate) enum VenusWorkerMessage {
    Run(VenusWorkerJob),
    Shutdown,
}

/// Owns virglrenderer on one host thread.
///
/// CGL's current context is thread-local, while virtio-gpu MMIO exits may be
/// serviced by any vCPU thread. A mutex serializes those exits but does not
/// preserve their host thread identity. Keep renderer initialization and every
/// subsequent FFI call on this worker so multi-vCPU guests cannot migrate a
/// Venus context between host threads.
pub struct ThreadedVenusBackend {
    pub(crate) protocol: VirtioGpuRendererProtocol,
    pub(crate) sender: Sender<VenusWorkerMessage>,
    pub(crate) worker: Option<JoinHandle<()>>,
}

impl ThreadedVenusBackend {
    pub fn new() -> Result<Self, String> {
        Self::new_for_protocol(VirtioGpuRendererProtocol::from_env()?)
    }

    pub fn new_for_protocol(protocol: VirtioGpuRendererProtocol) -> Result<Self, String> {
        let (sender, receiver) = mpsc::channel::<VenusWorkerMessage>();
        let (init_sender, init_receiver) = mpsc::sync_channel(1);
        let worker = thread::Builder::new()
            .name("bridgevm-venus-renderer".to_string())
            .spawn(move || {
                let mut backend = match VenusBackend::new_for_protocol(protocol) {
                    Ok(backend) => {
                        let _ = init_sender.send(Ok(()));
                        backend
                    }
                    Err(error) => {
                        let _ = init_sender.send(Err(error));
                        return;
                    }
                };
                while let Ok(message) = receiver.recv() {
                    match message {
                        VenusWorkerMessage::Run(job) => {
                            job(&mut backend);
                        }
                        VenusWorkerMessage::Shutdown => {
                            backend.reset();
                            break;
                        }
                    }
                }
            })
            .map_err(|error| format!("failed to spawn Venus renderer thread: {error}"))?;

        match init_receiver.recv() {
            Ok(Ok(())) => Ok(Self {
                protocol,
                sender,
                worker: Some(worker),
            }),
            Ok(Err(error)) => {
                let _ = worker.join();
                Err(error)
            }
            Err(error) => {
                let _ = worker.join();
                Err(format!(
                    "Venus renderer thread exited during initialization: {error}"
                ))
            }
        }
    }

    pub fn protocol(&self) -> VirtioGpuRendererProtocol {
        self.protocol
    }

    pub(crate) fn call<R, F>(&self, operation: F) -> R
    where
        R: Send + 'static,
        F: FnOnce(&mut VenusBackend) -> R + Send + 'static,
    {
        let (result_sender, result_receiver) = mpsc::sync_channel(1);
        self.sender
            .send(VenusWorkerMessage::Run(Box::new(move |backend| {
                let _ = result_sender.send(operation(backend));
            })))
            .expect("Venus renderer thread stopped before request dispatch");
        result_receiver
            .recv()
            .expect("Venus renderer thread stopped before request completion")
    }
}

impl Drop for ThreadedVenusBackend {
    fn drop(&mut self) {
        let _ = self.sender.send(VenusWorkerMessage::Shutdown);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl VirtioGpu3dBackend for ThreadedVenusBackend {
    fn capset_count(&self) -> u32 {
        self.call(|backend| backend.capset_count())
    }

    fn capset_info(&mut self, capset_index: u32) -> Option<CapsetInfo> {
        self.call(move |backend| backend.capset_info(capset_index))
    }

    fn capset(&mut self, capset_id: u32, version: u32) -> Option<Vec<u8>> {
        self.call(move |backend| backend.capset(capset_id, version))
    }

    fn capset_into(&mut self, capset_id: u32, version: u32, out: &mut Vec<u8>) -> bool {
        let Some(capset) = self.call(move |backend| backend.capset(capset_id, version)) else {
            return false;
        };
        out.extend_from_slice(&capset);
        true
    }

    fn ctx_create(&mut self, ctx_id: u32, context_init: u32, name: &[u8]) -> bool {
        let name = name.to_vec();
        self.call(move |backend| backend.ctx_create(ctx_id, context_init, &name))
    }

    fn ctx_destroy(&mut self, ctx_id: u32) {
        self.call(move |backend| backend.ctx_destroy(ctx_id));
    }

    fn ctx_attach_resource(&mut self, ctx_id: u32, resource_id: u32) {
        self.call(move |backend| backend.ctx_attach_resource(ctx_id, resource_id));
    }

    fn ctx_detach_resource(&mut self, ctx_id: u32, resource_id: u32) {
        self.call(move |backend| backend.ctx_detach_resource(ctx_id, resource_id));
    }

    fn supports_legacy_3d_resources(&self) -> bool {
        self.call(|backend| backend.supports_legacy_3d_resources())
    }

    fn create_3d(&mut self, args: Create3dArgs) -> bool {
        self.call(move |backend| backend.create_3d(args))
    }

    fn attach_backing(
        &mut self,
        resource_id: u32,
        host_iovecs: &[crate::virtio_gpu_3d::BlobHostIovec],
    ) -> bool {
        let host_iovecs = host_iovecs.to_vec();
        self.call(move |backend| backend.attach_backing(resource_id, &host_iovecs))
    }

    fn detach_backing(&mut self, resource_id: u32) -> bool {
        self.call(move |backend| backend.detach_backing(resource_id))
    }

    fn transfer_3d(&mut self, args: Transfer3dArgs, to_host: bool) -> bool {
        self.call(move |backend| backend.transfer_3d(args, to_host))
    }

    fn submit_3d(&mut self, ctx_id: u32, cmdbuf: &[u8]) -> bool {
        let cmdbuf_address = cmdbuf.as_ptr() as usize;
        let cmdbuf_len = cmdbuf.len();
        self.call(move |backend| {
            // The caller blocks in call(), so the immutable command buffer
            // remains alive and cannot be mutated until submission returns.
            let cmdbuf =
                unsafe { std::slice::from_raw_parts(cmdbuf_address as *const u8, cmdbuf_len) };
            backend.submit_3d(ctx_id, cmdbuf)
        })
    }

    fn create_blob(&mut self, args: CreateBlobArgs<'_>) -> bool {
        let ctx_id = args.ctx_id;
        let resource_id = args.resource_id;
        let blob_mem = args.blob_mem;
        let blob_flags = args.blob_flags;
        let blob_id = args.blob_id;
        let size = args.size;
        let iovecs = args.iovecs.to_vec();
        self.call(move |backend| {
            backend.create_blob(CreateBlobArgs {
                ctx_id,
                resource_id,
                blob_mem,
                blob_flags,
                blob_id,
                size,
                iovecs: &iovecs,
            })
        })
    }

    fn map_blob(&mut self, resource_id: u32) -> Option<MappedBlob> {
        self.call(move |backend| backend.map_blob(resource_id))
    }

    fn unmap_blob(&mut self, resource_id: u32) {
        self.call(move |backend| backend.unmap_blob(resource_id));
    }

    fn scanout_map(&mut self, resource_id: u32) -> Option<ScanoutMappedBlob> {
        self.call(move |backend| backend.scanout_map(resource_id))
    }

    fn scanout_unmap(&mut self, resource_id: u32) {
        self.call(move |backend| backend.scanout_unmap(resource_id));
    }

    fn scanout_read(&mut self, resource_id: u32, width: u32, height: u32, out: &mut [u8]) -> bool {
        let out_address = out.as_mut_ptr() as usize;
        let out_len = out.len();
        self.call(move |backend| {
            // The caller blocks in call() until this job completes, so the
            // borrowed output slice remains alive and exclusively borrowed.
            let out = unsafe { std::slice::from_raw_parts_mut(out_address as *mut u8, out_len) };
            backend.scanout_read(resource_id, width, height, out)
        })
    }

    fn scanout_blit_iosurface(&mut self, resource_id: u32, width: u32, height: u32) -> Option<u32> {
        self.call(move |backend| backend.scanout_blit_iosurface(resource_id, width, height))
    }

    fn scanout_iosurface_checksum(&mut self) -> Option<u64> {
        self.call(|backend| backend.scanout_iosurface_checksum())
    }

    fn scanout_iosurface_dump(&mut self, path: &std::path::Path) -> bool {
        let path = path.to_path_buf();
        self.call(move |backend| backend.scanout_iosurface_dump(&path))
    }

    fn destroy_resource(&mut self, resource_id: u32) {
        self.call(move |backend| backend.destroy_resource(resource_id));
    }

    fn create_fence(&mut self, ctx_id: u32, ring_idx: u8, fence_id: u64) -> bool {
        self.call(move |backend| backend.create_fence(ctx_id, ring_idx, fence_id))
    }

    fn poll_fences(&mut self) {
        self.call(|backend| backend.poll_fences());
    }

    fn drain_completed_fences_into(&mut self, out: &mut Vec<CompletedFence>) {
        let completed = self.call(|backend| backend.drain_completed_fences());
        out.extend(completed);
    }

    fn reset(&mut self) {
        self.call(|backend| backend.reset());
    }
}
