//! Validate everything before unlinking only the captured attempt's members.

use super::super::record::{Record, PIN_NAME};
use std::{fs, io, path::Path};

pub(super) fn clear(root: &Path, stage: &Path, record: &Record) -> io::Result<()> {
    let observed = Record::read_pinned(root, stage)?;
    if observed.record != record.record || !record.root.matches(root)? {
        return Err(io::Error::other("restore ownership record was replaced"));
    }
    record.matches_stage(stage)?;
    // Recheck after any recovery hashing, before the first destructive action.
    for (name, member) in [
        ("disk.raw", record.disk),
        ("vars.fd", record.vars),
        ("manifest.json", record.manifest),
    ] {
        if !record.stage.matches(stage)? || !member.identity.matches(&stage.join(name))? {
            return Err(io::Error::other("restore member changed before cleanup"));
        }
        fs::remove_file(stage.join(name))?;
    }
    if !record.stage.matches(stage)? || !record.record.matches(&stage.join(PIN_NAME))? {
        return Err(io::Error::other("restore pin changed before cleanup"));
    }
    fs::remove_file(stage.join(PIN_NAME))?;
    // An unexpected late entry prevents rmdir; never recurse into it.
    fs::remove_dir(stage)?;
    super::retirement::record(root, record)
}
