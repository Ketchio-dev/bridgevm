use super::*;
use bridgevm_hvf::snapshot_pair::managed::runtime::{acquire, RuntimeLease};

type Prepared = (ProbeConfig, RuntimeLease, Option<Box<dyn Tpm2Backend>>);

/// The run's media and this generation's TPM, both owned before the VM exists.
pub(super) fn prepare() -> Option<Prepared> {
    let mut config = ProbeConfig::from_env();
    let lease = match acquire(&mut config.media) {
        Ok(lease) => lease,
        Err(error) => {
            eprintln!("media ownership/selection refused before VM creation: {error}");
            return None;
        }
    };
    let restoring = checkpoint_glue::CheckpointEnv::from_env().restore.is_some();
    let data = config.swtpm_data_socket.as_deref();
    let tpm = generation_tpm(data, config.swtpm_control_socket.as_deref(), restoring)
        .map_err(|stop| println!("{stop}"))
        .ok()?;
    Some((config, lease, tpm))
}
