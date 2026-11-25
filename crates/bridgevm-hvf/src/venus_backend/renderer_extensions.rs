//! Optional entry points exported by BridgeVM's renderer build.

use super::backend::{virgl_renderer_init, VirglSubmitDiagnostic};
use std::ffi::CStr;
use std::mem::{transmute, MaybeUninit};
use std::os::raw::{c_char, c_int, c_void};
use std::sync::OnceLock;

type SubmitDiagnostic = unsafe extern "C" fn(*mut VirglSubmitDiagnostic) -> c_int;
type ScanoutBlit = unsafe extern "C" fn(u32, u32, u32, *mut u32) -> c_int;
type ScanoutChecksum = unsafe extern "C" fn(*mut u64) -> c_int;
type ScanoutDump = unsafe extern "C" fn(*const c_char) -> c_int;

#[derive(Default)]
pub(super) struct RendererExtensions {
    pub(super) submit_diagnostic: Option<SubmitDiagnostic>,
    pub(super) scanout_blit: Option<ScanoutBlit>,
    pub(super) scanout_checksum: Option<ScanoutChecksum>,
    pub(super) scanout_dump: Option<ScanoutDump>,
}

impl RendererExtensions {
    fn load() -> Self {
        let mut info = MaybeUninit::<libc::Dl_info>::uninit();
        // SAFETY: the address names a linked function and dladdr initializes
        // the out parameter on success.
        if unsafe { libc::dladdr(virgl_renderer_init as *const c_void, info.as_mut_ptr()) } == 0 {
            return Self::default();
        }
        let info = unsafe { info.assume_init() };
        let flags = libc::RTLD_LAZY | libc::RTLD_NOLOAD;
        #[cfg(target_os = "macos")]
        let flags = flags | libc::RTLD_FIRST;
        // SAFETY: dli_fname belongs to the linked image. NOLOAD takes a
        // reference to that image without loading any other library.
        let handle = unsafe { libc::dlopen(info.dli_fname, flags) };
        if handle.is_null() {
            return Self::default();
        }

        let resolve = |name: &CStr| unsafe { libc::dlsym(handle, name.as_ptr()) };
        // SAFETY: each symbol uses its declared C ABI. The nullable C function
        // pointer representation is supported by Option<extern "C" fn>.
        let extensions = unsafe {
            Self {
                submit_diagnostic: transmute::<*mut c_void, Option<SubmitDiagnostic>>(resolve(
                    c"virgl_renderer_bridgevm_get_last_submit_diagnostic",
                )),
                scanout_blit: transmute::<*mut c_void, Option<ScanoutBlit>>(resolve(
                    c"virgl_renderer_bridgevm_scanout_blit_iosurface",
                )),
                scanout_checksum: transmute::<*mut c_void, Option<ScanoutChecksum>>(resolve(
                    c"virgl_renderer_bridgevm_scanout_iosurface_checksum",
                )),
                scanout_dump: transmute::<*mut c_void, Option<ScanoutDump>>(resolve(
                    c"virgl_renderer_bridgevm_scanout_iosurface_dump",
                )),
            }
        };
        // SAFETY: release the temporary reference. The linked dependency
        // keeps the image and its function pointers live.
        unsafe { libc::dlclose(handle) };
        extensions
    }
}

pub(super) fn renderer_extensions() -> &'static RendererExtensions {
    static EXTENSIONS: OnceLock<RendererExtensions> = OnceLock::new();
    EXTENSIONS.get_or_init(RendererExtensions::load)
}

pub(crate) fn renderer_has_iosurface_scanout() -> bool {
    renderer_extensions().scanout_blit.is_some()
}
