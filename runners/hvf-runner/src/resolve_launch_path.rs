//! Split out of main.rs to keep files under 800 lines.

use std::env;
use std::path::Path;
use std::path::PathBuf;

pub(crate) fn resolve_launch_path(path: &Path, invocation_dir: &Path) -> PathBuf {
    let resolved = if path.is_absolute() {
        path.to_path_buf()
    } else {
        invocation_dir.join(path)
    };
    // Existing media/repository paths are canonicalized so a relative `..`
    // does not become ambiguous after the child changes its working directory.
    // Output paths may not exist yet, so retain their invocation-rooted absolute
    // spelling when canonicalization is unavailable.
    resolved.canonicalize().unwrap_or(resolved)
}

pub(crate) fn path_arg(path: &Path, invocation_dir: &Path) -> String {
    resolve_launch_path(path, invocation_dir)
        .to_string_lossy()
        .into_owned()
}

pub(crate) fn push_path_arg(
    out: &mut Vec<String>,
    flag: &str,
    value: Option<&PathBuf>,
    invocation_dir: &Path,
) {
    if let Some(value) = value {
        out.push(flag.to_string());
        out.push(path_arg(value, invocation_dir));
    }
}

pub(crate) fn push_string_arg(out: &mut Vec<String>, flag: &str, value: Option<&str>) {
    if let Some(value) = value {
        out.push(flag.to_string());
        out.push(value.to_string());
    }
}

pub(crate) fn push_num_arg<T: ToString>(out: &mut Vec<String>, flag: &str, value: Option<T>) {
    if let Some(value) = value {
        out.push(flag.to_string());
        out.push(value.to_string());
    }
}

pub(crate) fn push_flag(out: &mut Vec<String>, enabled: bool, flag: &str) {
    if enabled {
        out.push(flag.to_string());
    }
}

pub(crate) fn env_truthy(name: &str) -> bool {
    match env::var(name) {
        Ok(value) => matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        ),
        Err(_) => false,
    }
}
