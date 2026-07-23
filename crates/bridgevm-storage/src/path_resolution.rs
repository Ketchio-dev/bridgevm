//! Absolutize, resolve-for-new, containment checks and bundle-relative join.

use crate::*;
use std::env;
use std::ffi::OsStr;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

pub(crate) fn resolve_path_for_new(path: &Path) -> Result<PathBuf, StorageError> {
    if path.exists() {
        return Ok(fs::canonicalize(path)?);
    }

    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        env::current_dir()?.join(path)
    };
    let mut existing = absolute.as_path();
    let mut missing = Vec::new();
    while !existing.exists() {
        if let Some(name) = existing.file_name() {
            missing.push(name.to_os_string());
        }
        existing = existing.parent().unwrap_or_else(|| Path::new("."));
    }

    let mut resolved = fs::canonicalize(existing)?;
    for component in missing.iter().rev() {
        if component == OsStr::new(".") {
            continue;
        }
        if component == OsStr::new("..") {
            resolved.pop();
        } else {
            resolved.push(component);
        }
    }
    Ok(resolved)
}

pub(crate) fn is_same_or_descendant(path: &Path, ancestor: &Path) -> bool {
    path == ancestor || path.starts_with(ancestor)
}

pub(crate) fn resolve_bundle_path(bundle_path: &Path, relative_or_absolute: &str) -> PathBuf {
    let path = PathBuf::from(relative_or_absolute);
    if path.is_absolute() {
        path
    } else {
        bundle_path.join(path)
    }
}

pub(crate) fn absolutize(path: PathBuf) -> PathBuf {
    if path.is_absolute() {
        path
    } else {
        env::current_dir()
            .map(|cwd| cwd.join(&path))
            .unwrap_or(path)
    }
}
