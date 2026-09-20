//! Installed native CLI discovery and a bounded compatibility-marker check.
//! The marker is not a signature or a cryptographic trust check.

use crate::*;
use std::ffi::{CStr, OsStr};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::OpenOptionsExt;

#[path = "app_cli_candidates.rs"]
mod candidate_paths;
use candidate_paths::candidates;

const PROTOCOL_MARKER: &[u8] = b"bridgevm-native-cli-v1";
const MAX_EXECUTABLE_BYTES: u64 = 256 * 1024 * 1024;
#[path = "app_cli_resolver_diagnostics.rs"]
pub mod diagnostics;
pub(crate) use diagnostics::{diagnose, DiscoveryDiagnosis};
pub(crate) fn resolve() -> Result<PathBuf> {
    if !cfg!(target_os = "macos") {
        bail!("native app commands require macOS and a compatible BridgeVM app");
    }
    let executable = env::current_exe()
        .context("could not locate the current bridgevm executable")?
        .canonicalize()
        .context("could not resolve the current bridgevm executable")?;
    resolve_candidates(&candidates(&executable, account_home().as_deref()))
}

fn resolve_candidates(paths: &[PathBuf]) -> Result<PathBuf> {
    for path in paths {
        match fs::symlink_metadata(path) {
            Ok(metadata) => {
                if !metadata.file_type().is_file() {
                    bail!(
                        "native app executable must be a regular file: {}",
                        path.display()
                    );
                }
                validate_marker(path).with_context(|| format!(
                    "incompatible native app at {}; install a current BridgeVM app or use its paired CLI",
                    path.display()
                ))?;
                return Ok(path.clone());
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(error).with_context(|| {
                    format!("cannot inspect native app executable: {}", path.display())
                })
            }
        }
    }
    bail!("compatible BridgeVM.app or BridgeVMControl.app was not found; install it in /Applications or ~/Applications, or use the CLI bundled in its Contents/Resources/target/release directory")
}

fn validate_marker(path: &Path) -> Result<()> {
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.permissions().mode() & 0o111 == 0 {
        bail!("native app target must be a regular executable file");
    }
    if metadata.len() > MAX_EXECUTABLE_BYTES {
        bail!("native app executable exceeds the 256 MiB compatibility-check limit");
    }
    let mut total = 0_u64;
    let mut window = Vec::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        total += read as u64;
        if total > MAX_EXECUTABLE_BYTES {
            bail!("native app executable grew beyond the compatibility-check limit");
        }
        window.extend_from_slice(&buffer[..read]);
        if window
            .windows(PROTOCOL_MARKER.len())
            .any(|bytes| bytes == PROTOCOL_MARKER)
        {
            return Ok(());
        }
        let keep = window.len().min(PROTOCOL_MARKER.len() - 1);
        window.drain(..window.len() - keep);
    }
    bail!("native CLI protocol bridgevm-native-cli-v1 is missing; this app will not be launched")
}

fn account_home() -> Option<PathBuf> {
    // Resolve the account home without treating HOME as a helper override.
    let mut buffer = vec![0_u8; 128 * 1024];
    let mut entry = std::mem::MaybeUninit::<libc::passwd>::uninit();
    let mut result = std::ptr::null_mut();
    // getpwuid_r writes into caller-owned storage; pointers remain valid here.
    let status = unsafe {
        libc::getpwuid_r(
            libc::geteuid(),
            entry.as_mut_ptr(),
            buffer.as_mut_ptr().cast(),
            buffer.len(),
            &mut result,
        )
    };
    if status != 0 || result.is_null() {
        return None;
    }
    // A successful non-null result initializes entry and its NUL-terminated directory.
    let directory = unsafe { (*result).pw_dir };
    if directory.is_null() {
        return None;
    }
    let home = unsafe { CStr::from_ptr(directory) };
    let path = PathBuf::from(OsStr::from_bytes(home.to_bytes()));
    path.is_absolute().then_some(path)
}

#[cfg(test)]
#[path = "app_cli_resolver_tests.rs"]
mod tests;
