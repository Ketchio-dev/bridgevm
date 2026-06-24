use std::time::Duration;

use super::XhciSetupInputEnvError;

const SETUP_INPUT_RAMFB_DELAY_ENV: &str = "BRIDGEVM_XHCI_SETUP_INPUT_RAMFB_DELAY_MS";
const SETUP_INPUT_RAMFB_DELAY_ENV_MAX_BYTES: usize = 128;
const SETUP_INPUT_RAMFB_MAX_DELAYS: usize = 16;
const SETUP_INPUT_RAMFB_MAX_DELAY_MS: u64 = 120_000;
const SETUP_INPUT_RAMFB_DEFAULT_DELAY_MS: &[u64] = &[1_000, 5_000, 15_000];

#[derive(Debug)]
pub(super) struct RamfbDelayCheckpoint {
    pub(super) label: String,
    pub(super) delay: Duration,
    pub(super) emitted: bool,
}

pub(super) fn parse_setup_input_ramfb_delay_env(
) -> Result<Vec<RamfbDelayCheckpoint>, XhciSetupInputEnvError> {
    match std::env::var(SETUP_INPUT_RAMFB_DELAY_ENV) {
        Ok(value) => parse_setup_input_ramfb_delay_value(&value),
        Err(std::env::VarError::NotPresent) => Ok(default_ramfb_delay_checkpoints()),
        Err(std::env::VarError::NotUnicode(_)) => Err(XhciSetupInputEnvError::RamfbDelayInvalid {
            token: String::from("<non-unicode>"),
        }),
    }
}

pub(super) fn default_ramfb_delay_checkpoints() -> Vec<RamfbDelayCheckpoint> {
    SETUP_INPUT_RAMFB_DEFAULT_DELAY_MS
        .iter()
        .map(|delay_ms| ramfb_delay_checkpoint(*delay_ms))
        .collect()
}

pub(super) fn ramfb_delay_checkpoints_from_ms(
    delays_ms: &[u64],
) -> Result<Vec<RamfbDelayCheckpoint>, XhciSetupInputEnvError> {
    if delays_ms.is_empty() {
        return Err(XhciSetupInputEnvError::RamfbDelayEmpty);
    }
    let mut unique_delays_ms = Vec::new();
    for delay_ms in delays_ms {
        validate_delay_ms(*delay_ms)?;
        if unique_delays_ms.contains(delay_ms) {
            return Err(XhciSetupInputEnvError::RamfbDelayDuplicate {
                delay_ms: *delay_ms,
            });
        }
        unique_delays_ms.push(*delay_ms);
        if unique_delays_ms.len() > SETUP_INPUT_RAMFB_MAX_DELAYS {
            return Err(XhciSetupInputEnvError::RamfbDelayTooMany {
                requested: unique_delays_ms.len(),
                max: SETUP_INPUT_RAMFB_MAX_DELAYS,
            });
        }
    }
    Ok(unique_delays_ms
        .iter()
        .map(|delay_ms| ramfb_delay_checkpoint(*delay_ms))
        .collect())
}

fn parse_setup_input_ramfb_delay_value(
    value: &str,
) -> Result<Vec<RamfbDelayCheckpoint>, XhciSetupInputEnvError> {
    if value.len() > SETUP_INPUT_RAMFB_DELAY_ENV_MAX_BYTES {
        return Err(XhciSetupInputEnvError::RamfbDelayTooLong {
            len: value.len(),
            max: SETUP_INPUT_RAMFB_DELAY_ENV_MAX_BYTES,
        });
    }
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(XhciSetupInputEnvError::RamfbDelayEmpty);
    }
    let mut delays_ms = Vec::new();
    for token in trimmed
        .split([',', ' ', '\t', '\n'])
        .filter(|token| !token.is_empty())
    {
        let Ok(delay_ms) = token.parse::<u64>() else {
            return Err(XhciSetupInputEnvError::RamfbDelayInvalid {
                token: token.to_string(),
            });
        };
        delays_ms.push(delay_ms);
    }
    ramfb_delay_checkpoints_from_ms(&delays_ms)
}

fn validate_delay_ms(delay_ms: u64) -> Result<(), XhciSetupInputEnvError> {
    if delay_ms == 0 {
        return Err(XhciSetupInputEnvError::RamfbDelayInvalid {
            token: delay_ms.to_string(),
        });
    }
    if delay_ms > SETUP_INPUT_RAMFB_MAX_DELAY_MS {
        return Err(XhciSetupInputEnvError::RamfbDelayTooLarge {
            requested_ms: delay_ms,
            max_ms: SETUP_INPUT_RAMFB_MAX_DELAY_MS,
        });
    }
    Ok(())
}

fn ramfb_delay_checkpoint(delay_ms: u64) -> RamfbDelayCheckpoint {
    RamfbDelayCheckpoint {
        label: format!("setup-input-delay-{delay_ms}ms"),
        delay: Duration::from_millis(delay_ms),
        emitted: false,
    }
}
