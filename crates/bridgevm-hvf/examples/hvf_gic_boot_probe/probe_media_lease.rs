use super::*;
use bridgevm_hvf::media_lease::MediaLease;

pub(super) fn prepare() -> Option<(ProbeConfig, MediaLease)> {
    let config = ProbeConfig::from_env();
    let mut paths = Vec::new();
    for disk in [&config.media.nvme_disk, &config.media.nvme_target]
        .into_iter()
        .flatten()
    {
        paths.push(disk.path.as_path());
    }
    if !paths.is_empty() {
        paths.push(config.media.flash_vars.path.as_path());
    }
    match MediaLease::acquire(paths) {
        Ok(lease) => Some((config, lease)),
        Err(error) => {
            eprintln!("media ownership refused before VM creation: {error}");
            None
        }
    }
}
