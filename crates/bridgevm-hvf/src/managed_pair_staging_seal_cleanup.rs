//! Same-attempt metadata cleanup uses identities held before any record write.

use super::{sync, Identity, PIN_NAME, ROOT_NAME};
use std::{fs, io, path::Path};

fn check_directories(
    root: &Path,
    stage: &Path,
    root_id: Identity,
    stage_id: Identity,
) -> io::Result<()> {
    if !root_id.matches(root)? || !stage_id.matches(stage)? {
        return Err(io::Error::other(
            "restore ownership directories changed during sealing",
        ));
    }
    Ok(())
}

fn remove_owned(path: &Path, identity: Identity) -> io::Result<()> {
    if !identity.matches(path)? {
        return Err(io::Error::other("restore ownership metadata was replaced"));
    }
    fs::remove_file(path)
}

pub(super) fn unsealed(
    root: &Path,
    stage: &Path,
    root_id: Identity,
    stage_id: Identity,
    record_id: Identity,
) -> io::Result<()> {
    check_directories(root, stage, root_id, stage_id)?;
    match fs::symlink_metadata(stage.join(PIN_NAME)) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Ok(_) => {
            return Err(io::Error::other(
                "unsupported restore ownership link left a pin",
            ))
        }
        Err(error) => return Err(error),
    }
    remove_owned(&root.join(ROOT_NAME), record_id)
}

pub(super) fn failed(
    root: &Path,
    stage: &Path,
    root_id: Identity,
    stage_id: Identity,
    record_id: Identity,
) -> io::Result<()> {
    check_directories(root, stage, root_id, stage_id)?;
    match fs::symlink_metadata(stage.join(PIN_NAME)) {
        Ok(_) => {
            remove_owned(&stage.join(PIN_NAME), record_id)?;
            sync(stage)?;
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    match fs::symlink_metadata(root.join(ROOT_NAME)) {
        Ok(_) => remove_owned(&root.join(ROOT_NAME), record_id)?,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    }
    sync(root)
}
