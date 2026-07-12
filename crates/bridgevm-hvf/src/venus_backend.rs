#![allow(non_camel_case_types)]
#![allow(dead_code)]

use std::{
    collections::{BTreeMap, BTreeSet},
    env,
    ffi::CString,
    os::raw::{c_char, c_int, c_uint, c_void},
    ptr,
    sync::{Arc, Mutex, OnceLock},
};

use crate::virtio_gpu_3d::{
    CapsetInfo, CompletedFence, Create3dArgs, CreateBlobArgs, MappedBlob, ScanoutMappedBlob,
    Transfer3dArgs, VirtioGpu3dBackend, VIRTIO_GPU_RESP_ERR_UNSPEC,
};

type virgl_renderer_gl_context = *mut c_void;

#[repr(C)]
pub struct virgl_renderer_gl_ctx_param {
    pub version: c_int,
    pub shared: bool,
    pub major_ver: c_int,
    pub minor_ver: c_int,
    pub compat_ctx: c_int,
}

const VIRGL_RENDERER_CALLBACKS_VERSION: c_int = 4;

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

const VIRGL_RENDERER_USE_EXTERNAL_BLOB: c_int = 1 << 5;
const VIRGL_RENDERER_THREAD_SYNC: c_int = 2;
const VIRGL_RENDERER_VENUS: c_int = 1 << 6;
const VIRGL_RENDERER_NO_VIRGL: c_int = 1 << 7;
const VIRGL_RENDERER_ASYNC_FENCE_CB: c_int = 1 << 8;
const VIRGL_RENDERER_RENDER_SERVER: c_int = 1 << 9;
const VIRGL_RENDERER_USE_GUEST_VRAM: c_int = 1 << 14;
const VIRGL_RENDERER_CONTEXT_FLAG_CAPSET_ID_MASK: u32 = 0xff;
const VIRTIO_GPU_CAPSET_VIRGL: u32 = 1;
const VIRTIO_GPU_CAPSET_VIRGL2: u32 = 2;
const VIRTIO_GPU_CAPSET_VENUS: u32 = 4;

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
    fn virgl_renderer_init(
        cookie: *mut c_void,
        flags: c_int,
        cb: *mut virgl_renderer_callbacks,
    ) -> c_int;
    fn virgl_renderer_get_cap_set(set: u32, max_ver: *mut u32, max_size: *mut u32);
    fn virgl_renderer_fill_caps(set: u32, version: u32, caps: *mut c_void);
    fn virgl_renderer_context_create_with_flags(
        ctx_id: u32,
        ctx_flags: u32,
        nlen: u32,
        name: *const c_char,
    ) -> c_int;
    fn virgl_renderer_context_destroy(handle: u32);
    fn virgl_renderer_force_ctx_0();
    fn virgl_renderer_ctx_attach_resource(ctx_id: c_int, res_handle: c_int);
    fn virgl_renderer_ctx_detach_resource(ctx_id: c_int, res_handle: c_int);
    fn virgl_renderer_submit_cmd(buffer: *mut c_void, ctx_id: c_int, ndw: c_int) -> c_int;
    fn virgl_renderer_resource_create(
        args: *mut virgl_renderer_resource_create_args,
        iov: *mut iovec,
        num_iovs: u32,
    ) -> c_int;
    fn virgl_renderer_resource_attach_iov(
        res_handle: c_int,
        iov: *mut iovec,
        num_iovs: c_int,
    ) -> c_int;
    fn virgl_renderer_resource_detach_iov(
        res_handle: c_int,
        iov: *mut *mut iovec,
        num_iovs: *mut c_int,
    );
    fn virgl_renderer_transfer_write_iov(
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
    fn virgl_renderer_transfer_read_iov(
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
    fn virgl_renderer_context_create_fence(
        ctx_id: u32,
        flags: u32,
        ring_idx: u32,
        fence_id: u64,
    ) -> c_int;
    fn virgl_renderer_context_poll(ctx_id: u32);
    fn virgl_renderer_resource_create_blob(
        args: *const virgl_renderer_resource_create_blob_args,
    ) -> c_int;
    fn virgl_renderer_resource_map(
        res_handle: c_uint,
        map: *mut *mut c_void,
        out_size: *mut u64,
    ) -> c_int;
    fn virgl_renderer_resource_unmap(res_handle: c_uint) -> c_int;
    fn virgl_renderer_resource_get_map_info(res_handle: c_uint, map_info: *mut u32) -> c_int;
    fn virgl_renderer_resource_unref(res_handle: c_uint);
}

#[derive(Default)]
struct VenusShared {
    completed: Vec<CompletedFence>,
}

static INIT: OnceLock<Result<VirtioGpuRendererProtocol, String>> = OnceLock::new();
static SHARED: OnceLock<Arc<Mutex<VenusShared>>> = OnceLock::new();

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

    fn capset_id_for_index(self, capset_index: u32) -> Option<u32> {
        match (self, capset_index) {
            (Self::Venus, 0) => Some(VIRTIO_GPU_CAPSET_VENUS),
            (Self::Virgl, 0) => Some(VIRTIO_GPU_CAPSET_VIRGL),
            (Self::Virgl, 1) => Some(VIRTIO_GPU_CAPSET_VIRGL2),
            _ => None,
        }
    }

    fn supports_capset_id(self, capset_id: u32) -> bool {
        match self {
            Self::Venus => capset_id == VIRTIO_GPU_CAPSET_VENUS,
            Self::Virgl => {
                capset_id == VIRTIO_GPU_CAPSET_VIRGL || capset_id == VIRTIO_GPU_CAPSET_VIRGL2
            }
        }
    }

    fn init_flags(self) -> c_int {
        match self {
            Self::Venus => {
                VIRGL_RENDERER_USE_EXTERNAL_BLOB
                    | VIRGL_RENDERER_VENUS
                    | VIRGL_RENDERER_NO_VIRGL
                    | VIRGL_RENDERER_RENDER_SERVER
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
    protocol: VirtioGpuRendererProtocol,
    shared: Arc<Mutex<VenusShared>>,
    contexts: Vec<u32>,
    outstanding_fences: BTreeMap<u32, usize>,
    mapped_resources: BTreeMap<u32, VenusMappedResource>,
    resources: BTreeSet<u32>,
    // virglrenderer retains the iovec-array pointer for the resource lifetime.
    // Each array therefore needs stable, per-resource storage rather than a
    // reusable scratch vector.
    resource_iovecs: BTreeMap<u32, Vec<iovec>>,
    resource_ids_scratch: Vec<u32>,
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
struct VenusMappedResource {
    host_ptr: *mut u8,
    size: usize,
    map_info: u32,
    refs: usize,
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

impl VirtioGpu3dBackend for VenusBackend {
    fn capset_count(&self) -> u32 {
        let capset_ids: &[u32] = match self.protocol {
            VirtioGpuRendererProtocol::Venus => &[VIRTIO_GPU_CAPSET_VENUS],
            VirtioGpuRendererProtocol::Virgl => {
                &[VIRTIO_GPU_CAPSET_VIRGL, VIRTIO_GPU_CAPSET_VIRGL2]
            }
        };
        capset_ids
            .iter()
            .filter(|capset_id| {
                let mut max_version = 0u32;
                let mut max_size = 0u32;
                unsafe {
                    virgl_renderer_get_cap_set(**capset_id, &mut max_version, &mut max_size);
                }
                max_size != 0
            })
            .count() as u32
    }

    fn capset_info(&mut self, capset_index: u32) -> Option<CapsetInfo> {
        let capset_id = self.protocol.capset_id_for_index(capset_index)?;
        let mut max_version = 0u32;
        let mut max_size = 0u32;
        unsafe {
            virgl_renderer_get_cap_set(capset_id, &mut max_version, &mut max_size);
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
        if !self.protocol.supports_capset_id(capset_id) {
            return false;
        }
        let mut max_version = 0u32;
        let mut max_size = 0u32;
        unsafe {
            virgl_renderer_get_cap_set(capset_id, &mut max_version, &mut max_size);
        }
        if max_size == 0 {
            return false;
        }
        if version > max_version {
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
        unsafe {
            virgl_renderer_context_destroy(ctx_id);
        }
        self.contexts.retain(|ctx| *ctx != ctx_id);
        self.outstanding_fences.remove(&ctx_id);
    }

    fn ctx_attach_resource(&mut self, ctx_id: u32, resource_id: u32) {
        unsafe {
            virgl_renderer_ctx_attach_resource(ctx_id as c_int, resource_id as c_int);
        }
    }

    fn ctx_detach_resource(&mut self, ctx_id: u32, resource_id: u32) {
        unsafe {
            virgl_renderer_ctx_detach_resource(ctx_id as c_int, resource_id as c_int);
        }
    }

    fn create_3d(&mut self, args: Create3dArgs) -> bool {
        // Resource creation is not tied to a guest renderer context.  On CGL,
        // the current context is thread-local while a serialized virtio-gpu
        // notification may arrive on any vCPU thread, so make ctx0 current
        // before virglrenderer issues glGenBuffers/glBufferData or texture
        // allocation calls.
        if self.protocol == VirtioGpuRendererProtocol::Virgl {
            unsafe {
                virgl_renderer_force_ctx_0();
            }
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
        ret == 0
    }

    fn create_blob(&mut self, args: CreateBlobArgs<'_>) -> bool {
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
        let mapped = self.map_resource_ref(resource_id)?;
        Some(MappedBlob {
            host_ptr: mapped.host_ptr,
            size: mapped.size,
            map_info: mapped.map_info,
        })
    }

    fn unmap_blob(&mut self, resource_id: u32) {
        self.unmap_resource_ref(resource_id);
    }

    fn scanout_map(&mut self, resource_id: u32) -> Option<ScanoutMappedBlob> {
        let mapped = self.map_resource_ref(resource_id)?;
        Some(ScanoutMappedBlob {
            host_ptr: mapped.host_ptr.cast_const(),
            size: mapped.size,
        })
    }

    fn scanout_unmap(&mut self, resource_id: u32) {
        self.unmap_resource_ref(resource_id);
    }

    fn destroy_resource(&mut self, resource_id: u32) {
        while self.mapped_resources.contains_key(&resource_id) {
            self.unmap_resource_ref(resource_id);
        }
        // Resource destruction is global too and may delete a shared GL object.
        // Rebind ctx0 for the same thread-local CGL reason as create_3d().
        if self.protocol == VirtioGpuRendererProtocol::Virgl {
            unsafe {
                virgl_renderer_force_ctx_0();
            }
        }
        unsafe {
            virgl_renderer_resource_unref(resource_id);
        }
        self.resource_iovecs.remove(&resource_id);
        self.resources.remove(&resource_id);
    }

    fn create_fence(&mut self, ctx_id: u32, ring_idx: u8, fence_id: u64) -> bool {
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

impl VenusBackend {
    fn map_resource_ref(&mut self, resource_id: u32) -> Option<VenusMappedResource> {
        if let Some(mapped) = self.mapped_resources.get_mut(&resource_id) {
            mapped.refs = mapped.refs.saturating_add(1);
            return Some(*mapped);
        }
        let mut ptr_out: *mut c_void = ptr::null_mut();
        let mut size = 0u64;
        let ret = unsafe { virgl_renderer_resource_map(resource_id, &mut ptr_out, &mut size) };
        if ret != 0 || ptr_out.is_null() {
            eprintln!(
                "{}: resource_map res={resource_id} ret={ret} ptr={ptr_out:p} size={size}",
                self.protocol.label()
            );
            return None;
        }
        let mut map_info = 0u32;
        let info_ret = unsafe { virgl_renderer_resource_get_map_info(resource_id, &mut map_info) };
        if info_ret != 0 {
            eprintln!(
                "{}: resource_get_map_info res={resource_id} ret={info_ret}",
                self.protocol.label()
            );
            unsafe {
                virgl_renderer_resource_unmap(resource_id);
            }
            return None;
        }
        let mapped = VenusMappedResource {
            host_ptr: ptr_out.cast::<u8>(),
            size: usize::try_from(size).ok()?,
            map_info,
            refs: 1,
        };
        self.mapped_resources.insert(resource_id, mapped);
        Some(mapped)
    }

    fn unmap_resource_ref(&mut self, resource_id: u32) {
        let Some(mapped) = self.mapped_resources.get_mut(&resource_id) else {
            return;
        };
        if mapped.refs > 1 {
            mapped.refs -= 1;
            return;
        }
        self.mapped_resources.remove(&resource_id);
        let ret = unsafe { virgl_renderer_resource_unmap(resource_id) };
        if ret != 0 {
            eprintln!(
                "{}: resource_unmap res={resource_id} ret={ret}",
                self.protocol.label()
            );
        }
    }
}

extern "C" fn write_fence(_cookie: *mut c_void, _fence: u32) {}

extern "C" fn write_context_fence(cookie: *mut c_void, ctx_id: u32, ring_idx: u32, fence_id: u64) {
    if cookie.is_null() {
        return;
    }
    let shared = unsafe { &*(cookie as *const Mutex<VenusShared>) };
    shared.lock().unwrap().completed.push(CompletedFence {
        ctx_id,
        ring_idx: ring_idx as u8,
        fence_id,
    });
}

fn init_renderer(
    shared: Arc<Mutex<VenusShared>>,
    protocol: VirtioGpuRendererProtocol,
) -> Result<VirtioGpuRendererProtocol, String> {
    // virglrenderer stores the callback cookie process-globally. Leak one Arc
    // clone so the raw cookie remains stable for callbacks until process exit.
    let cookie = Arc::into_raw(shared) as *mut c_void;
    let create_gl_context: Option<
        extern "C" fn(
            cookie: *mut c_void,
            scanout_idx: c_int,
            param: *mut virgl_renderer_gl_ctx_param,
        ) -> virgl_renderer_gl_context,
    > = match protocol {
        VirtioGpuRendererProtocol::Venus => None,
        VirtioGpuRendererProtocol::Virgl => Some(host_gl::create_gl_context),
    };
    let destroy_gl_context: Option<
        extern "C" fn(cookie: *mut c_void, ctx: virgl_renderer_gl_context),
    > = match protocol {
        VirtioGpuRendererProtocol::Venus => None,
        VirtioGpuRendererProtocol::Virgl => Some(host_gl::destroy_gl_context),
    };
    let make_current: Option<
        extern "C" fn(
            cookie: *mut c_void,
            scanout_idx: c_int,
            ctx: virgl_renderer_gl_context,
        ) -> c_int,
    > = match protocol {
        VirtioGpuRendererProtocol::Venus => None,
        VirtioGpuRendererProtocol::Virgl => Some(host_gl::make_current),
    };
    let callbacks = Box::leak(Box::new(virgl_renderer_callbacks {
        version: VIRGL_RENDERER_CALLBACKS_VERSION,
        write_fence: Some(write_fence),
        create_gl_context,
        destroy_gl_context,
        make_current,
        get_drm_fd: None,
        write_context_fence: Some(write_context_fence),
        get_server_fd: None,
        get_egl_display: None,
    }));
    let flags = protocol.init_flags();
    let ret = unsafe { virgl_renderer_init(cookie, flags, callbacks) };
    if ret == 0 {
        Ok(protocol)
    } else {
        Err(format!(
            "virgl_renderer_init protocol={} flags=0x{flags:x} failed ret={ret}; resp_err={VIRTIO_GPU_RESP_ERR_UNSPEC:#x}",
            protocol.label()
        ))
    }
}

#[cfg(target_os = "macos")]
mod host_gl {
    use std::sync::atomic::{AtomicPtr, Ordering};

    use super::*;

    type CGLContextObj = *mut c_void;
    type CGLPixelFormatObj = *mut c_void;

    const K_CGL_PFA_ACCELERATED: c_int = 73;
    const K_CGL_PFA_ALLOW_OFFLINE_RENDERERS: c_int = 96;
    const K_CGL_PFA_OPENGL_PROFILE: c_int = 99;
    const K_CGL_OGLP_VERSION_3_2_CORE: c_int = 0x3200;

    static FIRST_SHARED_CONTEXT: AtomicPtr<c_void> = AtomicPtr::new(ptr::null_mut());

    unsafe extern "C" {
        fn CGLChoosePixelFormat(
            attribs: *const c_int,
            pix: *mut CGLPixelFormatObj,
            npix: *mut c_int,
        ) -> c_int;
        fn CGLDestroyPixelFormat(pix: CGLPixelFormatObj) -> c_int;
        fn CGLCreateContext(
            pix: CGLPixelFormatObj,
            share: CGLContextObj,
            ctx: *mut CGLContextObj,
        ) -> c_int;
        fn CGLDestroyContext(ctx: CGLContextObj) -> c_int;
        fn CGLSetCurrentContext(ctx: CGLContextObj) -> c_int;
    }

    fn supports_requested_version(param: &virgl_renderer_gl_ctx_param) -> bool {
        if param.compat_ctx != 0 {
            return false;
        }
        match (param.major_ver, param.minor_ver) {
            (4, minor) => minor <= 1,
            (3, minor) => minor >= 2,
            _ => false,
        }
    }

    pub extern "C" fn create_gl_context(
        _cookie: *mut c_void,
        scanout_idx: c_int,
        param: *mut virgl_renderer_gl_ctx_param,
    ) -> virgl_renderer_gl_context {
        if param.is_null() {
            eprintln!("virgl: create_gl_context cgl unavailable: null context params");
            return ptr::null_mut();
        }
        let param = unsafe { &*param };
        eprintln!(
            "virgl: create_gl_context cgl request scanout_idx={scanout_idx} major={} minor={} shared={} compat={}",
            param.major_ver, param.minor_ver, param.shared, param.compat_ctx
        );
        if !supports_requested_version(param) {
            return ptr::null_mut();
        }

        let attribs = [
            K_CGL_PFA_OPENGL_PROFILE,
            K_CGL_OGLP_VERSION_3_2_CORE,
            K_CGL_PFA_ACCELERATED,
            K_CGL_PFA_ALLOW_OFFLINE_RENDERERS,
            0,
        ];
        let mut pixel_format = ptr::null_mut();
        let mut npix = 0;
        let choose_ret =
            unsafe { CGLChoosePixelFormat(attribs.as_ptr(), &mut pixel_format, &mut npix) };
        if choose_ret != 0 || pixel_format.is_null() || npix <= 0 {
            eprintln!(
                "virgl: create_gl_context cgl unavailable: CGLChoosePixelFormat ret={choose_ret} npix={npix}"
            );
            return ptr::null_mut();
        }

        let share_context = if param.shared {
            FIRST_SHARED_CONTEXT.load(Ordering::SeqCst)
        } else {
            ptr::null_mut()
        };
        let mut context = ptr::null_mut();
        let create_ret = unsafe { CGLCreateContext(pixel_format, share_context, &mut context) };
        unsafe {
            CGLDestroyPixelFormat(pixel_format);
        }
        if create_ret != 0 || context.is_null() {
            eprintln!(
                "virgl: create_gl_context cgl unavailable: CGLCreateContext ret={create_ret} shared={} share_context={share_context:p}",
                param.shared
            );
            return ptr::null_mut();
        }
        let _ = FIRST_SHARED_CONTEXT.compare_exchange(
            ptr::null_mut(),
            context,
            Ordering::SeqCst,
            Ordering::SeqCst,
        );
        eprintln!(
            "virgl: create_gl_context cgl success context={context:p} shared={} share_context={share_context:p}",
            param.shared
        );
        context.cast::<c_void>()
    }

    pub extern "C" fn destroy_gl_context(_cookie: *mut c_void, ctx: virgl_renderer_gl_context) {
        if ctx.is_null() {
            return;
        }
        let context = ctx.cast::<c_void>();
        let _ = FIRST_SHARED_CONTEXT.compare_exchange(
            context,
            ptr::null_mut(),
            Ordering::SeqCst,
            Ordering::SeqCst,
        );
        unsafe {
            CGLSetCurrentContext(ptr::null_mut());
            CGLDestroyContext(context);
        }
    }

    pub extern "C" fn make_current(
        _cookie: *mut c_void,
        _scanout_idx: c_int,
        ctx: virgl_renderer_gl_context,
    ) -> c_int {
        let ret = unsafe { CGLSetCurrentContext(ctx.cast::<c_void>()) };
        if ret != 0 {
            eprintln!("virgl: make_current cgl failed ret={ret} ctx={ctx:p}");
        }
        ret
    }
}

#[cfg(not(target_os = "macos"))]
mod host_gl {
    use super::*;

    pub extern "C" fn create_gl_context(
        _cookie: *mut c_void,
        _scanout_idx: c_int,
        _param: *mut virgl_renderer_gl_ctx_param,
    ) -> virgl_renderer_gl_context {
        eprintln!("virgl: create_gl_context unavailable: BridgeVM has no VirGL GL winsys");
        ptr::null_mut()
    }

    pub extern "C" fn destroy_gl_context(_cookie: *mut c_void, _ctx: virgl_renderer_gl_context) {}

    pub extern "C" fn make_current(
        _cookie: *mut c_void,
        _scanout_idx: c_int,
        _ctx: virgl_renderer_gl_context,
    ) -> c_int {
        -1
    }
}

fn set_env_defaults() {
    if env::var_os("VK_ICD_FILENAMES").is_none() {
        env::set_var(
            "VK_ICD_FILENAMES",
            "/opt/homebrew/share/vulkan/icd.d/MoltenVK_icd.json",
        );
    }
    append_env_default_path("DYLD_FALLBACK_LIBRARY_PATH", "/opt/homebrew/lib");
}

fn append_env_default_path(key: &str, value: &str) {
    let current = env::var_os(key)
        .map(|v| v.to_string_lossy().into_owned())
        .unwrap_or_default();
    if current.split(':').any(|part| part == value) {
        return;
    }
    let next = if current.is_empty() {
        value.to_string()
    } else {
        format!("{value}:{current}")
    };
    env::set_var(key, next);
}
