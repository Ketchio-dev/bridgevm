//! Create managed storage together with its initial-generation marker.
//! The root is built under a sibling name and renamed into place, so an
//! interrupted setup leaves either no root or a root with its marker.

use super::layout::private_directory;
use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

/// Internal durable boundaries in initialization; production observes none.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum InitStage {
    DirectoryCreated,
    MarkerSynced,
    RootPublished,
}

/// An existing root is never repaired here: without `current` or the marker
/// it is indistinguishable from lost media and must keep failing closed.
pub(super) fn initialize(root: &Path, mut observe: impl FnMut(InitStage)) -> io::Result<()> {
    if private_directory(root, false)? {
        return Ok(());
    }
    let building = building_path(root);
    if private_directory(&building, false)? {
        // An interrupted setup of this pair leaves it empty or holding only
        // the empty marker; remove_dir refuses anything else.
        let marker = building.join("original");
        if fs::symlink_metadata(&marker).is_ok_and(|m| m.is_file() && m.len() == 0) {
            fs::remove_file(&marker)?;
        }
        fs::remove_dir(&building)?;
    }
    private_directory(&building, true)?;
    observe(InitStage::DirectoryCreated);
    let marker = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(building.join("original"))?;
    marker.sync_all()?;
    observe(InitStage::MarkerSynced);
    File::open(&building)?.sync_all()?;
    fs::rename(&building, root)?;
    observe(InitStage::RootPublished);
    File::open(root.parent().unwrap())?.sync_all()
}

/// Identity resolution refuses unbound ".bridgevm-pair-*" entries, so the
/// unpublished root must not start with that prefix.
fn building_path(root: &Path) -> PathBuf {
    let mut name = OsString::from(".bridgevm-init-");
    name.push(root.file_name().unwrap());
    root.with_file_name(name)
}
