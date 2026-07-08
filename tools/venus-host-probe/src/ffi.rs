#![allow(non_camel_case_types)]
#![allow(dead_code)]

use std::os::raw::{c_char, c_int, c_uint, c_void};

pub type virgl_renderer_gl_context = *mut c_void;

#[repr(C)]
pub struct virgl_renderer_gl_ctx_param {
    pub version: c_int,
    pub shared: bool,
    pub major_ver: c_int,
    pub minor_ver: c_int,
    pub compat_ctx: c_int,
}

pub const VIRGL_RENDERER_CALLBACKS_VERSION: c_int = 4;

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

pub const VIRGL_RENDERER_THREAD_SYNC: c_int = 2;
pub const VIRGL_RENDERER_USE_EXTERNAL_BLOB: c_int = 1 << 5;
pub const VIRGL_RENDERER_VENUS: c_int = 1 << 6;
pub const VIRGL_RENDERER_NO_VIRGL: c_int = 1 << 7;
pub const VIRGL_RENDERER_ASYNC_FENCE_CB: c_int = 1 << 8;
pub const VIRGL_RENDERER_RENDER_SERVER: c_int = 1 << 9;

pub const VIRGL_RENDERER_CONTEXT_FLAG_CAPSET_ID_MASK: u32 = 0xff;
pub const VIRTIO_GPU_CAPSET_VIRGL: u32 = 1;
pub const VIRTIO_GPU_CAPSET_VENUS: u32 = 4;

unsafe extern "C" {
    pub fn virgl_renderer_init(
        cookie: *mut c_void,
        flags: c_int,
        cb: *mut virgl_renderer_callbacks,
    ) -> c_int;
    pub fn virgl_renderer_poll();
    pub fn virgl_renderer_get_cap_set(set: u32, max_ver: *mut u32, max_size: *mut u32);
    pub fn virgl_renderer_fill_caps(set: u32, version: u32, caps: *mut c_void);
    pub fn virgl_renderer_context_create(handle: u32, nlen: u32, name: *const c_char) -> c_int;
    pub fn virgl_renderer_context_create_with_flags(
        ctx_id: u32,
        ctx_flags: u32,
        nlen: u32,
        name: *const c_char,
    ) -> c_int;
    pub fn virgl_renderer_context_destroy(handle: u32);
    pub fn virgl_renderer_cleanup(cookie: *mut c_void);
    pub fn virgl_renderer_get_poll_fd() -> c_int;
    pub fn virgl_renderer_context_poll(ctx_id: u32);
    pub fn virgl_renderer_context_get_poll_fd(ctx_id: u32) -> c_int;
    pub fn virgl_renderer_submit_cmd(buffer: *mut c_void, ctx_id: c_int, ndw: c_int) -> c_int;
    pub fn virgl_renderer_resource_create_blob(args: *const c_void) -> c_int;
    pub fn virgl_renderer_resource_map(
        res_handle: c_uint,
        map: *mut *mut c_void,
        out_size: *mut u64,
    ) -> c_int;
    pub fn virgl_renderer_resource_unmap(res_handle: c_uint) -> c_int;
    pub fn virgl_renderer_resource_unref(res_handle: c_uint);
    pub fn virgl_renderer_resource_export_blob(
        res_id: c_uint,
        fd_type: *mut c_uint,
        fd: *mut c_int,
    ) -> c_int;
    pub fn virgl_renderer_context_create_fence(
        ctx_id: u32,
        flags: u32,
        ring_idx: u32,
        fence_id: u64,
    ) -> c_int;
}
