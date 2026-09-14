//! Select and capture the media policies protected by one runtime owner.

use super::*;

pub fn acquire(media: &mut VirtBootMediaConfig) -> io::Result<RuntimeLease> {
    let mut lease = RuntimeLease {
        _pair: None,
        _logical: MediaLease::acquire([])?,
        slots: [None, None, None],
        retained: BTreeSet::new(),
    };
    let disks: Vec<_> = [&media.nvme_disk, &media.nvme_target]
        .into_iter()
        .flatten()
        .collect();
    if disks.is_empty() {
        return capture(lease, media);
    }
    if disks.len() == 2 {
        lease._logical = MediaLease::acquire([
            disks[0].path.as_path(),
            disks[1].path.as_path(),
            media.flash_vars.path.as_path(),
        ])?;
        let vars = fs::canonicalize(&media.flash_vars.path)?;
        for disk in disks {
            let root = layout::resolve_root(&fs::canonicalize(&disk.path)?, &vars)?;
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
        return capture(lease, media);
    }
    let original_disk = fs::canonicalize(&disks[0].path)?;
    let original_vars = fs::canonicalize(&media.flash_vars.path)?;
    let pair = LockedPair::open(&original_disk, &original_vars)?;
    let (disk, vars) = pair.paths()?;
    let slot = media
        .nvme_disk
        .as_mut()
        .or(media.nvme_target.as_mut())
        .unwrap();
    slot.path = disk;
    media.flash_vars.path = vars;
    lease._pair = Some(pair);
    capture(lease, media)
}

fn capture(mut lease: RuntimeLease, media: &VirtBootMediaConfig) -> io::Result<RuntimeLease> {
    lease.slots = [
        Some(media.flash_vars.clone()),
        media.nvme_disk.clone(),
        media.nvme_target.clone(),
    ];
    if media.nvme_disk.is_some() || media.nvme_target.is_some() {
        for slot in lease.slots.iter().flatten() {
            lease.retained.insert(fs::canonicalize(&slot.path)?);
        }
    }
    if let Some(pair) = &lease._pair {
        lease
            .retained
            .extend([pair.disk.clone(), pair.vars.clone()]);
    }
    Ok(lease)
}
