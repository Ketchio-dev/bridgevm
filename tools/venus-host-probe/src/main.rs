mod ffi;

use ffi::*;
use std::env;
use std::ffi::CString;
use std::os::raw::{c_int, c_void};
use std::ptr;
use std::sync::atomic::{AtomicPtr, Ordering};

#[derive(Debug, Clone, Copy)]
struct VenusCapset {
    wire_format_version: u32,
    vk_xml_version: u32,
    vk_ext_command_serialization_spec_version: u32,
    vk_mesa_venus_protocol_spec_version: u32,
    supports_blob_id_0: u32,
    allow_vk_wait_syncs: u32,
    supports_multiple_timelines: u32,
    use_guest_vram: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProbeProtocol {
    Venus,
    Virgl,
}

impl ProbeProtocol {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "venus" => Some(Self::Venus),
            "virgl" => Some(Self::Virgl),
            _ => None,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Venus => "venus",
            Self::Virgl => "virgl",
        }
    }

    fn display(self) -> &'static str {
        match self {
            Self::Venus => "VENUS",
            Self::Virgl => "VIRGL",
        }
    }

    fn capset_id(self) -> u32 {
        match self {
            Self::Venus => VIRTIO_GPU_CAPSET_VENUS,
            Self::Virgl => VIRTIO_GPU_CAPSET_VIRGL,
        }
    }

    fn renderer_mode(self) -> &'static str {
        match self {
            Self::Venus => "venus-render-server",
            Self::Virgl => "virgl-gl-callbacks-required",
        }
    }

    fn init_flags(self) -> i32 {
        match self {
            Self::Venus => {
                (1 << 14) // VIRGL_RENDERER_USE_GUEST_VRAM
                    | VIRGL_RENDERER_VENUS
                    | VIRGL_RENDERER_NO_VIRGL
                    | VIRGL_RENDERER_RENDER_SERVER
                    | VIRGL_RENDERER_THREAD_SYNC
                    | VIRGL_RENDERER_ASYNC_FENCE_CB
            }
            Self::Virgl => {
                (1 << 14) // VIRGL_RENDERER_USE_GUEST_VRAM
                    | VIRGL_RENDERER_USE_EXTERNAL_BLOB
                    | VIRGL_RENDERER_THREAD_SYNC
                    | VIRGL_RENDERER_ASYNC_FENCE_CB
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Options {
    protocol: ProbeProtocol,
    allow_unavailable: bool,
}

extern "C" fn write_fence(_cookie: *mut c_void, fence: u32) {
    eprintln!("write_fence fence={fence}");
}

extern "C" fn write_context_fence(_cookie: *mut c_void, ctx_id: u32, ring_idx: u32, fence_id: u64) {
    eprintln!("write_context_fence ctx_id={ctx_id} ring_idx={ring_idx} fence_id={fence_id}");
}

#[cfg(target_os = "macos")]
mod host_gl {
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

    pub fn backend_label(protocol: ProbeProtocol) -> &'static str {
        match protocol {
            ProbeProtocol::Venus => "not-required",
            ProbeProtocol::Virgl => "cgl-opengl",
        }
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
            eprintln!("create_gl_context cgl unavailable: null context params");
            return ptr::null_mut();
        }
        let param = unsafe { &*param };
        eprintln!(
            "create_gl_context cgl request scanout_idx={scanout_idx} major={} minor={} shared={} compat={}",
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
                "create_gl_context cgl unavailable: CGLChoosePixelFormat ret={choose_ret} npix={npix}"
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
                "create_gl_context cgl unavailable: CGLCreateContext ret={create_ret} shared={} share_context={share_context:p}",
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
            "create_gl_context cgl success context={context:p} shared={} share_context={share_context:p}",
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
            eprintln!("make_current cgl failed ret={ret} ctx={ctx:p}");
        }
        ret
    }
}

#[cfg(not(target_os = "macos"))]
mod host_gl {
    use super::*;

    pub fn backend_label(protocol: ProbeProtocol) -> &'static str {
        match protocol {
            ProbeProtocol::Venus => "not-required",
            ProbeProtocol::Virgl => "stub-unavailable",
        }
    }

    pub extern "C" fn create_gl_context(
        _cookie: *mut c_void,
        _scanout_idx: c_int,
        _param: *mut virgl_renderer_gl_ctx_param,
    ) -> virgl_renderer_gl_context {
        eprintln!("create_gl_context unavailable: BridgeVM host probe has no VirGL GL winsys");
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

fn set_env_default(key: &str, value: &str) {
    if env::var_os(key).is_none() {
        env::set_var(key, value);
    }
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

fn read_u32_le(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

fn decode_vk_api_version(version: u32) -> String {
    let variant = version >> 29;
    let major = (version >> 22) & 0x7f;
    let minor = (version >> 12) & 0x3ff;
    let patch = version & 0xfff;
    format!("{variant}.{major}.{minor}.{patch}")
}

fn decode_capset(bytes: &[u8]) -> VenusCapset {
    VenusCapset {
        wire_format_version: read_u32_le(bytes, 0),
        vk_xml_version: read_u32_le(bytes, 4),
        vk_ext_command_serialization_spec_version: read_u32_le(bytes, 8),
        vk_mesa_venus_protocol_spec_version: read_u32_le(bytes, 12),
        supports_blob_id_0: read_u32_le(bytes, 16),
        allow_vk_wait_syncs: read_u32_le(bytes, 148),
        supports_multiple_timelines: read_u32_le(bytes, 152),
        use_guest_vram: read_u32_le(bytes, 156),
    }
}

fn hex_dump_first_64(bytes: &[u8]) {
    let n = bytes.len().min(64);
    println!("capset first 64 bytes:");
    for (row, chunk) in bytes[..n].chunks(16).enumerate() {
        print!("  {:04x}:", row * 16);
        for byte in chunk {
            print!(" {byte:02x}");
        }
        println!();
    }
}

fn fail(message: impl AsRef<str>) -> ! {
    eprintln!("ERROR: {}", message.as_ref());
    std::process::exit(1);
}

fn usage(status: i32) -> ! {
    eprintln!(
        "usage: venus_capset_probe [--protocol venus|virgl] [--allow-unavailable]\n\
         \n\
         Default --protocol venus remains a hard gate. Use --protocol virgl\n\
         --allow-unavailable to record the current host VirGL renderer state\n\
         without making the command fail when VirGL is unavailable."
    );
    std::process::exit(status);
}

fn parse_options() -> Options {
    let mut options = Options {
        protocol: ProbeProtocol::Venus,
        allow_unavailable: false,
    };
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--protocol" => {
                let Some(value) = args.next() else {
                    usage(2);
                };
                let Some(protocol) = ProbeProtocol::parse(&value) else {
                    usage(2);
                };
                options.protocol = protocol;
            }
            "--allow-unavailable" => {
                options.allow_unavailable = true;
            }
            "-h" | "--help" => usage(0),
            _ => usage(2),
        }
    }
    options
}

fn unavailable(options: Options, reason: impl AsRef<str>) -> ! {
    let reason = reason.as_ref();
    println!("renderer_available=false");
    println!("host_renderer_{}=NOT_AVAILABLE", options.protocol.label());
    println!("unavailable_reason={reason}");
    if options.allow_unavailable {
        println!(
            "{}_HOST_RENDERER_UNAVAILABLE reason={reason}",
            options.protocol.display()
        );
        std::process::exit(0);
    }
    fail(reason);
}

fn main() {
    let options = parse_options();
    set_env_default(
        "VK_ICD_FILENAMES",
        "/opt/homebrew/share/vulkan/icd.d/MoltenVK_icd.json",
    );
    append_env_default_path("DYLD_FALLBACK_LIBRARY_PATH", "/opt/homebrew/lib");

    println!(
        "VK_ICD_FILENAMES={}",
        env::var("VK_ICD_FILENAMES").unwrap_or_default()
    );
    println!(
        "DYLD_FALLBACK_LIBRARY_PATH={}",
        env::var("DYLD_FALLBACK_LIBRARY_PATH").unwrap_or_default()
    );
    println!(
        "RENDER_SERVER_EXEC_PATH={}",
        env::var("RENDER_SERVER_EXEC_PATH").unwrap_or_default()
    );
    println!("requested_protocol={}", options.protocol.label());
    println!("renderer_mode={}", options.protocol.renderer_mode());
    println!(
        "gl_context_callbacks={}",
        host_gl::backend_label(options.protocol)
    );

    let mut renderer_cookie = 0u8;
    let renderer_cookie_ptr = (&mut renderer_cookie as *mut u8).cast::<c_void>();
    println!("renderer_cookie_nonnull={}", !renderer_cookie_ptr.is_null());

    let mut callbacks = virgl_renderer_callbacks {
        version: VIRGL_RENDERER_CALLBACKS_VERSION,
        write_fence: Some(write_fence),
        create_gl_context: Some(host_gl::create_gl_context),
        destroy_gl_context: Some(host_gl::destroy_gl_context),
        make_current: Some(host_gl::make_current),
        get_drm_fd: None,
        write_context_fence: Some(write_context_fence),
        get_server_fd: None,
        get_egl_display: None,
    };

    let flags = options.protocol.init_flags();

    let init_ret = unsafe { virgl_renderer_init(renderer_cookie_ptr, flags, &mut callbacks) };
    println!("virgl_renderer_init flags=0x{flags:x} ret={init_ret}");
    if init_ret != 0 {
        unavailable(
            options,
            format!("virgl_renderer_init failed ret={init_ret}"),
        );
    }

    let mut max_ver = 0u32;
    let mut max_size = 0u32;
    unsafe {
        virgl_renderer_get_cap_set(options.protocol.capset_id(), &mut max_ver, &mut max_size);
    }
    println!(
        "virgl_renderer_get_cap_set({}) max_ver={max_ver} max_size={max_size}",
        options.protocol.capset_id()
    );
    if max_size == 0 {
        unsafe {
            virgl_renderer_cleanup(renderer_cookie_ptr);
        }
        unavailable(
            options,
            format!(
                "{} capset {} is unavailable",
                options.protocol.display(),
                options.protocol.capset_id()
            ),
        );
    }

    let mut capset = vec![0u8; max_size as usize];
    unsafe {
        virgl_renderer_fill_caps(
            options.protocol.capset_id(),
            max_ver,
            capset.as_mut_ptr().cast::<c_void>(),
        );
    }
    hex_dump_first_64(&capset);

    if options.protocol == ProbeProtocol::Venus && capset.len() < 160 {
        unsafe {
            virgl_renderer_cleanup(renderer_cookie_ptr);
        }
        unavailable(options, format!("venus capset too small: {}", capset.len()));
    }

    if options.protocol == ProbeProtocol::Venus {
        let decoded = decode_capset(&capset);
        println!(
            "decoded capset: wire_format_version={} vk_xml_version={} ({}) vk_ext_command_serialization_spec_version={} vk_mesa_venus_protocol_spec_version={} supports_blob_id_0={} allow_vk_wait_syncs={} supports_multiple_timelines={} use_guest_vram={}",
            decoded.wire_format_version,
            decoded.vk_xml_version,
            decode_vk_api_version(decoded.vk_xml_version),
            decoded.vk_ext_command_serialization_spec_version,
            decoded.vk_mesa_venus_protocol_spec_version,
            decoded.supports_blob_id_0,
            decoded.allow_vk_wait_syncs,
            decoded.supports_multiple_timelines,
            decoded.use_guest_vram
        );
    }

    let ctx_name = CString::new(format!("{}-host-probe", options.protocol.label())).unwrap();
    let ctx_id = 1u32;
    let ctx_flags = options.protocol.capset_id() & VIRGL_RENDERER_CONTEXT_FLAG_CAPSET_ID_MASK;
    let ctx_ret = unsafe {
        virgl_renderer_context_create_with_flags(
            ctx_id,
            ctx_flags,
            ctx_name.as_bytes().len() as u32,
            ctx_name.as_ptr(),
        )
    };
    println!(
        "virgl_renderer_context_create_with_flags(ctx_id=1, capset={}) ret={ctx_ret}",
        options.protocol.capset_id()
    );
    if ctx_ret != 0 {
        unsafe {
            virgl_renderer_cleanup(renderer_cookie_ptr);
        }
        unavailable(
            options,
            format!(
                "{} context create failed ret={ctx_ret}",
                options.protocol.display()
            ),
        );
    }

    unsafe {
        virgl_renderer_context_destroy(ctx_id);
        virgl_renderer_cleanup(renderer_cookie_ptr);
    }

    println!("renderer_available=true");
    println!("host_renderer_{}=AVAILABLE", options.protocol.label());
    println!(
        "{}_CAPSET_OK ver={max_ver} size={max_size}",
        options.protocol.display()
    );
}
