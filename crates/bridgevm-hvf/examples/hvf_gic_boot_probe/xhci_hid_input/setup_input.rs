#[path = "setup_input/actions.rs"]
mod actions;
#[path = "setup_input/delay.rs"]
mod delay;
#[path = "setup_input/error.rs"]
mod error;
#[path = "setup_input/host_wake.rs"]
mod host_wake;
#[path = "setup_input/trigger.rs"]
mod trigger;

use super::marker::MARKER_MAX_BYTES;

const SETUP_INPUT_DEFAULT_MARKER: &[u8] = b"BdsDxe: starting Boot0001";
const SETUP_INPUT_ENV_MAX_BYTES: usize = 128;
const SETUP_INPUT_MAX_ACTIONS: usize = 32;

const _: () = {
    assert!(!SETUP_INPUT_DEFAULT_MARKER.is_empty());
    assert!(SETUP_INPUT_DEFAULT_MARKER.len() <= MARKER_MAX_BYTES);
};

pub(crate) use actions::parse_setup_input_actions;
pub(crate) use error::{print_setup_input_rejection, XhciSetupInputEnvError};
pub(crate) use host_wake::SetupInputHostWake;
pub(crate) use trigger::XhciSetupInputTrigger;
