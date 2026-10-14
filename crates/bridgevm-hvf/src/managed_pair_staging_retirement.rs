//! Retire metadata only; writable current media is never rehashed or removed.

use super::super::record::{Record, ROOT_NAME};
use std::{
    fs::{self, File},
    io,
    path::Path,
};

pub(super) fn current(root: &Path) -> io::Result<()> {
    if !super::present(&root.join(ROOT_NAME))? {
        return Ok(());
    }
    let current = root.join("current");
    let owned = Record::read_pinned(root, &current)?;
    if !owned.stage.matches(&current)? {
        return Err(io::Error::other(
            "ownership record does not identify current generation",
        ));
    }
    record(root, &owned)
}

pub(super) fn record(root: &Path, owned: &Record) -> io::Result<()> {
    if !owned.root.matches(root)? || !owned.record.matches(&root.join(ROOT_NAME))? {
        return Err(io::Error::other(
            "restore ownership root or record was replaced",
        ));
    }
    // A successful publisher can leave old staging after best-effort cleanup.
    // Even without a manifest or pin, it must never become legacy copy debris
    // just because retirement forgot the exchange on this or a later attempt.
    if super::present(&root.join("staging"))? {
        return Err(io::Error::other(
            "unselected staging may be the previous published generation",
        ));
    }
    fs::remove_file(root.join(ROOT_NAME))?;
    File::open(root)?.sync_all()
}
