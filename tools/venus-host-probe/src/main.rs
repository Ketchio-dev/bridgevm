mod ffi;

use ffi::*;
use std::env;
use std::ffi::CString;
use std::os::raw::c_void;
use std::ptr;

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

extern "C" fn write_fence(_cookie: *mut c_void, fence: u32) {
    eprintln!("write_fence fence={fence}");
}

extern "C" fn write_context_fence(
    _cookie: *mut c_void,
    ctx_id: u32,
    ring_idx: u32,
    fence_id: u64,
) {
    eprintln!("write_context_fence ctx_id={ctx_id} ring_idx={ring_idx} fence_id={fence_id}");
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

fn main() {
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

    let flags = (1 << 14) // VIRGL_RENDERER_USE_GUEST_VRAM
        | VIRGL_RENDERER_VENUS
        | VIRGL_RENDERER_NO_VIRGL
        | VIRGL_RENDERER_RENDER_SERVER
        | VIRGL_RENDERER_THREAD_SYNC
        | VIRGL_RENDERER_ASYNC_FENCE_CB;

    let init_ret = unsafe { virgl_renderer_init(ptr::null_mut(), flags, &mut callbacks) };
    println!("virgl_renderer_init flags=0x{flags:x} ret={init_ret}");
    if init_ret != 0 {
        fail(format!("virgl_renderer_init failed ret={init_ret}"));
    }

    let mut max_ver = 0u32;
    let mut max_size = 0u32;
    unsafe {
        virgl_renderer_get_cap_set(VIRTIO_GPU_CAPSET_VENUS, &mut max_ver, &mut max_size);
    }
    println!("virgl_renderer_get_cap_set(4) max_ver={max_ver} max_size={max_size}");
    if max_size == 0 {
        unsafe {
            virgl_renderer_cleanup(ptr::null_mut());
        }
        fail("venus capset is unavailable");
    }

    let mut capset = vec![0u8; max_size as usize];
    unsafe {
        virgl_renderer_fill_caps(
            VIRTIO_GPU_CAPSET_VENUS,
            max_ver,
            capset.as_mut_ptr().cast::<c_void>(),
        );
    }
    hex_dump_first_64(&capset);

    if capset.len() < 160 {
        unsafe {
            virgl_renderer_cleanup(ptr::null_mut());
        }
        fail(format!("venus capset too small: {}", capset.len()));
    }

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

    let ctx_name = CString::new("venus-host-probe").unwrap();
    let ctx_id = 1u32;
    let ctx_flags = VIRTIO_GPU_CAPSET_VENUS & VIRGL_RENDERER_CONTEXT_FLAG_CAPSET_ID_MASK;
    let ctx_ret = unsafe {
        virgl_renderer_context_create_with_flags(
            ctx_id,
            ctx_flags,
            ctx_name.as_bytes().len() as u32,
            ctx_name.as_ptr(),
        )
    };
    println!("virgl_renderer_context_create_with_flags(ctx_id=1, capset=4) ret={ctx_ret}");
    if ctx_ret != 0 {
        unsafe {
            virgl_renderer_cleanup(ptr::null_mut());
        }
        fail(format!("venus context create failed ret={ctx_ret}"));
    }

    unsafe {
        virgl_renderer_context_destroy(ctx_id);
        virgl_renderer_cleanup(ptr::null_mut());
    }

    println!("VENUS_CAPSET_OK ver={max_ver} size={max_size}");
}
