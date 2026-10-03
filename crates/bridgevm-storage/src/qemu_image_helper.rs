//! Release image operations use the fixed external installation policy.

use std::io;
use std::path::PathBuf;

pub(crate) fn resolve(program: &str) -> io::Result<PathBuf> {
    #[cfg(debug_assertions)]
    {
        Ok(PathBuf::from(program))
    }
    #[cfg(not(debug_assertions))]
    {
        #[cfg(target_os = "macos")]
        let candidates = ["/opt/homebrew/bin/qemu-img", "/usr/local/bin/qemu-img"];
        #[cfg(target_os = "linux")]
        let candidates = ["/usr/bin/qemu-img"];
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        let candidates: [&str; 0] = [];
        resolve_release(program, &candidates)
    }
}

#[cfg(any(not(debug_assertions), test))]
pub(crate) fn resolve_release(program: &str, candidates: &[&str]) -> io::Result<PathBuf> {
    if program != "qemu-img" {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "release storage permits only the logical qemu-img helper",
        ));
    }
    for candidate in candidates {
        let path = std::path::Path::new(candidate);
        if path.metadata().is_ok_and(|metadata| executable(&metadata)) {
            return Ok(path.to_path_buf());
        }
    }
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        "qemu-img is unavailable at an approved fixed installation path",
    ))
}

#[cfg(any(not(debug_assertions), test))]
fn executable(metadata: &std::fs::Metadata) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        let _ = metadata;
        false
    }
}
