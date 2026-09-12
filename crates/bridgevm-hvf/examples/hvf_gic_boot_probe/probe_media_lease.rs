use super::*;
use bridgevm_hvf::snapshot_pair::managed::runtime::{acquire, RuntimeLease};

pub(super) fn prepare() -> Option<(ProbeConfig, RuntimeLease)> {
    let mut config = ProbeConfig::from_env();
    match acquire(&mut config.media) {
        Ok(lease) => Some((config, lease)),
        Err(error) => {
            eprintln!("media ownership/selection refused before VM creation: {error}");
            None
        }
    }
}
