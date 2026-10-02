//! Conservative Unicode alias comparison for absent names in one resolved parent.
use std::{
    ffi::{c_void, OsStr},
    io,
    os::unix::ffi::OsStrExt,
};
#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFStringCreateWithBytes(
        allocator: *const c_void,
        bytes: *const u8,
        count: isize,
        encoding: u32,
        external: u8,
    ) -> *const c_void;
    fn CFStringCompare(left: *const c_void, right: *const c_void, flags: usize) -> isize;
    fn CFRelease(value: *const c_void);
}
struct StringRef(*const c_void);
impl StringRef {
    fn new(name: &OsStr) -> io::Result<Self> {
        let bytes = name.as_bytes();
        let count = isize::try_from(bytes.len())
            .map_err(|_| io::Error::other("media output name too long"))?;
        // SAFETY: bytes live through the call; CF copies them with the default allocator.
        let value = unsafe {
            CFStringCreateWithBytes(std::ptr::null(), bytes.as_ptr(), count, 0x0800_0100, 0)
        };
        if value.is_null() {
            return Err(io::Error::other("media output name is not valid UTF-8"));
        }
        Ok(Self(value))
    }
}
impl Drop for StringRef {
    fn drop(&mut self) {
        // SAFETY: this owns one non-null create-rule CoreFoundation reference.
        unsafe { CFRelease(self.0) };
    }
}
pub(in super::super) fn equivalent(
    left: &OsStr,
    right: &OsStr,
    insensitive: bool,
) -> io::Result<bool> {
    if left.as_bytes().is_ascii() && right.as_bytes().is_ascii() {
        return Ok(insensitive && left.as_bytes().eq_ignore_ascii_case(right.as_bytes()));
    }
    let (left, right) = (StringRef::new(left)?, StringRef::new(right)?);
    // kCFCompareNonliteral preserves canonical equivalence; case folding is
    // admitted only on a case-insensitive filesystem. This is conservative
    // collision refusal, not an exact reproduction of APFS's Unicode tables.
    let flags = 16 | usize::from(insensitive);
    // SAFETY: both owned references are valid CFStrings for this call.
    Ok(unsafe { CFStringCompare(left.0, right.0, flags) } == 0)
}

use std::{fs, path::Path};
pub(super) fn case_alias(left: &Path, right: &Path) -> io::Result<bool> {
    use std::os::fd::AsRawFd;
    if left.parent() != right.parent()
        || !equivalent(left.file_name().unwrap(), right.file_name().unwrap(), true)?
    {
        return Ok(false);
    }
    let parent = fs::File::open(left.parent().unwrap())?;
    // SAFETY: the file owns this descriptor and fpathconf only queries it.
    let sensitive = unsafe { libc::fpathconf(parent.as_raw_fd(), libc::_PC_CASE_SENSITIVE) };
    if sensitive < 0 {
        return Err(io::Error::last_os_error());
    }
    equivalent(
        left.file_name().unwrap(),
        right.file_name().unwrap(),
        sensitive == 0,
    )
}
