//! Admit identities before probing links, without rejecting filesystem metadata.

use super::{private_directory, Identity, ROOT_NAME};
use std::{io, path::Path};

pub(super) fn before_link(
    root: &Path,
    stage: &Path,
    root_id: Identity,
    stage_id: Identity,
    record_id: Identity,
) -> io::Result<()> {
    if !private_directory(root, false)?
        || !private_directory(stage, false)?
        || stage.parent() != Some(root)
        || !root_id.matches(root)?
        || !stage_id.matches(stage)?
        || !record_id.matches(&root.join(ROOT_NAME))?
    {
        return Err(io::Error::other(
            "restore ownership identities changed before the link probe",
        ));
    }
    Ok(())
}
