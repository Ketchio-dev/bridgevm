//! Where a harness-mode GET writes (follow-up of 2026-10-23). The directory is
//! the operator's BRIDGEVM_VIRTIO_CONSOLE_GET_DIR; its deepest existing
//! ancestor is opened as configured, symlinks included. Each missing level
//! below it is created like a directory for a guest path (owner-only 0700 and
//! marked, guest_write_directory.rs), not with create_dir_all's 0755, and the
//! file gets the share's file policy through Destination.

use super::super::host_destination::Destination;
use std::io;
use std::path::Path;

/// The destination of the file `name` in the GET directory `dir`.
pub(super) fn destination(dir: &Path, name: &str) -> io::Result<Destination> {
    let mut existing = dir;
    let mut key = vec![name];
    while !existing.as_os_str().is_empty() && !existing.try_exists()? {
        let level = existing.file_name().and_then(|level| level.to_str());
        key.push(level.ok_or(io::ErrorKind::InvalidInput)?);
        existing = existing.parent().unwrap_or(Path::new(""));
    }
    key.reverse();
    let existing = if existing.as_os_str().is_empty() {
        Path::new(".")
    } else {
        existing
    };
    Destination::open(existing, &key.join("/"), true)
}
