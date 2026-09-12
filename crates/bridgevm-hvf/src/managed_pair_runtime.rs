//! Selection occurs before VM creation and every media read in the native probe.

use super::{layout, LockedPair};
use crate::media::VirtBootMediaConfig;
use crate::media_lease::MediaLease;
use std::{fs, io};

pub struct RuntimeLease {
    _pair: Option<LockedPair>,
    _logical: Option<MediaLease>,
    _selected: Option<MediaLease>,
}

pub fn acquire(media: &mut VirtBootMediaConfig) -> io::Result<RuntimeLease> {
    let mut lease = RuntimeLease {
        _pair: None,
        _logical: None,
        _selected: None,
    };
    let disks: Vec<_> = [&media.nvme_disk, &media.nvme_target]
        .into_iter()
        .flatten()
        .collect();
    if disks.is_empty() {
        return Ok(lease);
    }
    if disks.len() == 2 {
        lease._logical = Some(MediaLease::acquire([
            disks[0].path.as_path(),
            disks[1].path.as_path(),
            media.flash_vars.path.as_path(),
        ])?);
        let vars = fs::canonicalize(&media.flash_vars.path)?;
        for disk in disks {
            let root = layout::store_root(&fs::canonicalize(&disk.path)?, &vars);
            match fs::symlink_metadata(root) {
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
                Ok(_) => {
                    return Err(io::Error::other(
                        "managed shared-vars selection is ambiguous with two disks",
                    ))
                }
            }
        }
        return Ok(lease);
    }
    let original_disk = fs::canonicalize(&disks[0].path)?;
    let original_vars = fs::canonicalize(&media.flash_vars.path)?;
    let pair = LockedPair::open(&original_disk, &original_vars)?;
    let (disk, vars) = pair.paths()?;
    if disk != original_disk || vars != original_vars {
        lease._selected = Some(MediaLease::acquire([disk.as_path(), vars.as_path()])?);
    }
    let slot = media
        .nvme_disk
        .as_mut()
        .or(media.nvme_target.as_mut())
        .unwrap();
    slot.path = disk;
    media.flash_vars.path = vars;
    lease._pair = Some(pair);
    Ok(lease)
}

#[cfg(test)]
#[path = "managed_pair_runtime_tests.rs"]
mod tests;
