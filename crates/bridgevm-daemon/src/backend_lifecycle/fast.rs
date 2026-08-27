//! Fast Mode lifecycle opt-in policy shared by suspend and resume.

use crate::FastModeSpawnConfig;
use anyhow::Result;

pub(super) fn require_real_start(action: &str, refusal: &str) -> Result<FastModeSpawnConfig> {
    let Some(config) = FastModeSpawnConfig::from_env()? else {
        anyhow::bail!(
            "Fast Mode {action} requires explicit real-start opt-in \
             (BRIDGEVM_APPLE_VZ_ALLOW_REAL_START=1); {refusal}"
        );
    };
    Ok(config)
}
