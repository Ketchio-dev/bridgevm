#![allow(non_camel_case_types)]
#![allow(dead_code)]

use std::{
    collections::BTreeMap,
    env,
    ffi::CString,
    os::raw::{c_char, c_int, c_uint, c_void},
    ptr,
    sync::{Arc, Mutex, OnceLock},
};

use crate::virtio_gpu_3d::{
    CapsetInfo, CompletedFence, CreateBlobArgs, MappedBlob, ScanoutMappedBlob, VirtioGpu3dBackend,
    VIRTIO_GPU_RESP_ERR_UNSPEC,
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

const VIRGL_RENDERER_THREAD_SYNC: c_int = 2;
const VIRGL_RENDERER_VENUS: c_int = 1 << 6;
const VIRGL_RENDERER_NO_VIRGL: c_int = 1 << 7;
const VIRGL_RENDERER_ASYNC_FENCE_CB: c_int = 1 << 8;
const VIRGL_RENDERER_RENDER_SERVER: c_int = 1 << 9;
const VIRGL_RENDERER_USE_GUEST_VRAM: c_int = 1 << 14;
const VIRGL_RENDERER_CONTEXT_FLAG_CAPSET_ID_MASK: u32 = 0xff;
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
pub struct iovec {
    pub iov_base: *mut c_void,
    pub iov_len: usize,
}

unsafe extern "C" {
    fn virgl_renderer_init(
        cookie: *mut c_void,
        flags: c_int,
        cb: *mut virgl_renderer_callbacks,
    ) -> c_int;
    fn virgl_renderer_poll();
    fn virgl_renderer_get_poll_fd() -> c_int;
    fn virgl_renderer_get_cap_set(set: u32, max_ver: *mut u32, max_size: *mut u32);
    fn virgl_renderer_fill_caps(set: u32, version: u32, caps: *mut c_void);
    fn virgl_renderer_context_create_with_flags(
        ctx_id: u32,
        ctx_flags: u32,
        nlen: u32,
        name: *const c_char,
    ) -> c_int;
    fn virgl_renderer_context_destroy(handle: u32);
    fn virgl_renderer_ctx_attach_resource(ctx_id: c_int, res_handle: c_int);
    fn virgl_renderer_ctx_detach_resource(ctx_id: c_int, res_handle: c_int);
    fn virgl_renderer_context_poll(ctx_id: u32);
    fn virgl_renderer_context_get_poll_fd(ctx_id: u32) -> c_int;
    fn virgl_renderer_submit_cmd(buffer: *mut c_void, ctx_id: c_int, ndw: c_int) -> c_int;
    fn virgl_renderer_context_create_fence(
        ctx_id: u32,
        flags: u32,
        ring_idx: u32,
        fence_id: u64,
    ) -> c_int;
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

static INIT: OnceLock<Result<(), String>> = OnceLock::new();
static SHARED: OnceLock<Arc<Mutex<VenusShared>>> = OnceLock::new();

#[derive(Clone)]
pub struct VenusBackend {
    shared: Arc<Mutex<VenusShared>>,
    contexts: Vec<u32>,
    ring0_deferred: Vec<CompletedFence>,
    mapped_resources: BTreeMap<u32, VenusMappedResource>,
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
        set_env_defaults();
        let shared = SHARED
            .get_or_init(|| Arc::new(Mutex::new(VenusShared::default())))
            .clone();
        let init = INIT.get_or_init(|| init_renderer(shared.clone()));
        init.as_ref().map_err(Clone::clone)?;
        Ok(Self {
            shared,
            contexts: Vec::new(),
            ring0_deferred: Vec::new(),
            mapped_resources: BTreeMap::new(),
        })
    }
}

impl VirtioGpu3dBackend for VenusBackend {
    fn capset_info(&mut self, capset_index: u32) -> Option<CapsetInfo> {
        if capset_index != 0 {
            return None;
        }
        let mut max_version = 0u32;
        let mut max_size = 0u32;
        unsafe {
            virgl_renderer_get_cap_set(VIRTIO_GPU_CAPSET_VENUS, &mut max_version, &mut max_size);
        }
        (max_size != 0).then_some(CapsetInfo {
            capset_id: VIRTIO_GPU_CAPSET_VENUS,
            max_version,
            max_size,
        })
    }

    fn capset(&mut self, capset_id: u32, version: u32) -> Option<Vec<u8>> {
        if capset_id != VIRTIO_GPU_CAPSET_VENUS {
            return None;
        }
        let info = self.capset_info(0)?;
        let mut capset = vec![0u8; info.max_size as usize];
        unsafe {
            virgl_renderer_fill_caps(capset_id, version, capset.as_mut_ptr().cast::<c_void>());
        }
        Some(capset)
    }

    fn ctx_create(&mut self, ctx_id: u32, context_init: u32, name: &[u8]) -> bool {
        let name = CString::new(name).unwrap_or_else(|_| CString::new("bridgevm-venus").unwrap());
        let flags = context_init & VIRGL_RENDERER_CONTEXT_FLAG_CAPSET_ID_MASK;
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
            true
        } else {
            eprintln!("venus: context_create_with_flags ctx={ctx_id} ret={ret}");
            false
        }
    }

    fn ctx_destroy(&mut self, ctx_id: u32) {
        unsafe {
            virgl_renderer_context_destroy(ctx_id);
        }
        self.contexts.retain(|ctx| *ctx != ctx_id);
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
                "venus: submit_cmd ctx={ctx_id} bytes={} ret={ret}",
                cmdbuf.len()
            );
        }
        ret == 0
    }

    fn create_blob(&mut self, args: CreateBlobArgs<'_>) -> bool {
        let iovecs: Vec<iovec> = args
            .iovecs
            .iter()
            .map(|entry| iovec {
                iov_base: entry.host_ptr.cast::<c_void>(),
                iov_len: entry.len,
            })
            .collect();
        let create = virgl_renderer_resource_create_blob_args {
            res_handle: args.resource_id,
            ctx_id: args.ctx_id,
            blob_mem: args.blob_mem,
            blob_flags: args.blob_flags,
            blob_id: args.blob_id,
            size: args.size,
            iovecs: if iovecs.is_empty() {
                ptr::null()
            } else {
                iovecs.as_ptr()
            },
            num_iovs: iovecs.len() as u32,
        };
        let ret = unsafe { virgl_renderer_resource_create_blob(&create) };
        if ret != 0 {
            eprintln!(
                "venus: resource_create_blob ctx={} res={} blob_mem={} blob_id={} size={} ret={ret}",
                args.ctx_id, args.resource_id, args.blob_mem, args.blob_id, args.size
            );
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
        unsafe {
            virgl_renderer_resource_unref(resource_id);
        }
    }

    fn create_fence(&mut self, ctx_id: u32, ring_idx: u8, fence_id: u64) -> bool {
        let ret =
            unsafe { virgl_renderer_context_create_fence(ctx_id, 0, ring_idx.into(), fence_id) };
        if ret != 0 {
            eprintln!("venus: context_create_fence ctx={ctx_id} ring={ring_idx} fence={fence_id} ret={ret}");
        }
        if ret == 0 && ring_idx == 0 {
            self.ring0_deferred.push(CompletedFence {
                ctx_id,
                ring_idx,
                fence_id,
            });
        }
        ret == 0
    }

    fn drain_completed_fences(&mut self) -> Vec<CompletedFence> {
        unsafe {
            virgl_renderer_poll();
        }
        let completed = std::mem::take(&mut self.shared.lock().unwrap().completed);
        if !self.ring0_deferred.is_empty() {
            self.shared
                .lock()
                .unwrap()
                .completed
                .extend(self.ring0_deferred.drain(..));
        }
        completed
    }

    fn reset(&mut self) {
        let resource_ids: Vec<u32> = self.mapped_resources.keys().copied().collect();
        for resource_id in resource_ids {
            while self.mapped_resources.contains_key(&resource_id) {
                self.unmap_resource_ref(resource_id);
            }
        }
        for ctx_id in std::mem::take(&mut self.contexts) {
            unsafe {
                virgl_renderer_context_destroy(ctx_id);
            }
        }
        self.shared.lock().unwrap().completed.clear();
        self.ring0_deferred.clear();
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
                "venus: resource_map res={resource_id} ret={ret} ptr={ptr_out:p} size={size}"
            );
            return None;
        }
        let mut map_info = 0u32;
        let info_ret = unsafe { virgl_renderer_resource_get_map_info(resource_id, &mut map_info) };
        if info_ret != 0 {
            eprintln!("venus: resource_get_map_info res={resource_id} ret={info_ret}");
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
            eprintln!("venus: resource_unmap res={resource_id} ret={ret}");
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

fn init_renderer(shared: Arc<Mutex<VenusShared>>) -> Result<(), String> {
    // virglrenderer stores the callback cookie process-globally. Leak one Arc
    // clone so the raw cookie remains stable for callbacks until process exit.
    let cookie = Arc::into_raw(shared) as *mut c_void;
    let mut callbacks = virgl_renderer_callbacks {
        version: VIRGL_RENDERER_CALLBACKS_VERSION,
        write_fence: Some(write_fence),
        create_gl_context: None,
        destroy_gl_context: None,
        make_current: None,
        get_drm_fd: None,
        write_context_fence: Some(write_context_fence),
        get_server_fd: None,
        get_egl_display: None,
    };
    let flags = VIRGL_RENDERER_VENUS
        | VIRGL_RENDERER_NO_VIRGL
        | VIRGL_RENDERER_RENDER_SERVER
        | VIRGL_RENDERER_USE_GUEST_VRAM
        | VIRGL_RENDERER_THREAD_SYNC
        | VIRGL_RENDERER_ASYNC_FENCE_CB;
    let ret = unsafe { virgl_renderer_init(cookie, flags, &mut callbacks) };
    if ret == 0 {
        Ok(())
    } else {
        Err(format!(
            "virgl_renderer_init flags=0x{flags:x} failed ret={ret}; resp_err={VIRTIO_GPU_RESP_ERR_UNSPEC:#x}"
        ))
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
