//! Split out of venus_backend.rs to keep files under 500 lines.

#![allow(non_camel_case_types)]
#![allow(dead_code)]
use super::*;

use std::{
    env,
    os::raw::{c_int, c_void},
    ptr,
    sync::{Arc, Mutex},
};

use crate::virtio_gpu_3d::{CompletedFence, VIRTIO_GPU_RESP_ERR_UNSPEC};

impl VenusBackend {
    pub(crate) fn map_resource_ref(&mut self, resource_id: u32) -> Option<VenusMappedResource> {
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

    pub(crate) fn unmap_resource_ref(&mut self, resource_id: u32) {
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

pub(crate) fn init_renderer(
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
    > = Some(host_gl::create_gl_context);
    let destroy_gl_context: Option<
        extern "C" fn(cookie: *mut c_void, ctx: virgl_renderer_gl_context),
    > = Some(host_gl::destroy_gl_context);
    let make_current: Option<
        extern "C" fn(
            cookie: *mut c_void,
            scanout_idx: c_int,
            ctx: virgl_renderer_gl_context,
        ) -> c_int,
    > = Some(host_gl::make_current);
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
pub(crate) mod host_gl {
    use std::sync::atomic::{AtomicPtr, Ordering};

    use super::*;

    type CGLContextObj = *mut c_void;
    type CGLPixelFormatObj = *mut c_void;

    const K_CGL_PFA_ACCELERATED: c_int = 73;
    const K_CGL_PFA_ALLOW_OFFLINE_RENDERERS: c_int = 96;
    const K_CGL_PFA_OPENGL_PROFILE: c_int = 99;
    const K_CGL_OGLP_VERSION_3_2_CORE: c_int = 0x3200;

    static FIRST_SHARED_CONTEXT: AtomicPtr<c_void> = AtomicPtr::new(ptr::null_mut());
    static LAST_CURRENT_CONTEXT: AtomicPtr<c_void> = AtomicPtr::new(ptr::null_mut());

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

    /// CGL's current context is thread-local. The virtio-gpu device is already
    /// serialized by the platform lock, but consecutive MMIO exits can run on
    /// different vCPU host threads. Rebind the renderer's last logical context
    /// before entering virglrenderer so a same-context fast path cannot inherit
    /// a null or stale thread-local CGL binding.
    pub fn rebind_last_context() {
        let context = LAST_CURRENT_CONTEXT.load(Ordering::Acquire);
        if context.is_null() {
            return;
        }
        let ret = unsafe { CGLSetCurrentContext(context) };
        if ret != 0 {
            eprintln!("virgl: rebind current cgl failed ret={ret} ctx={context:p}");
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
        let _ = LAST_CURRENT_CONTEXT.compare_exchange(
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
        } else {
            LAST_CURRENT_CONTEXT.store(ctx.cast::<c_void>(), Ordering::Release);
        }
        ret
    }
}

#[cfg(not(target_os = "macos"))]
pub(crate) mod host_gl {
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

    pub fn rebind_last_context() {}
}

pub(crate) fn set_env_defaults() {
    if env::var_os("VK_ICD_FILENAMES").is_none() {
        env::set_var(
            "VK_ICD_FILENAMES",
            "/opt/homebrew/share/vulkan/icd.d/MoltenVK_icd.json",
        );
    }
    append_env_default_path("DYLD_FALLBACK_LIBRARY_PATH", "/opt/homebrew/lib");
}

pub(crate) fn append_env_default_path(key: &str, value: &str) {
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
