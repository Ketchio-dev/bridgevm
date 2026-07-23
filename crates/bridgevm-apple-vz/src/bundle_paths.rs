//! Bundle-relative path resolution shared by the plan, boot and artifact code.

use std::path::Path;
use std::path::PathBuf;

pub(crate) fn resolve_bundle_path(bundle_path: &Path, relative_or_absolute: &str) -> PathBuf {
    let path = PathBuf::from(relative_or_absolute);
    if path.is_absolute() {
        path
    } else {
        bundle_path.join(path)
    }
}
