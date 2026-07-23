//! Split out of unsupported.rs by responsibility.

use super::super::*;
use super::*;
use crate::*;

pub fn query_hvf_host_capabilities() -> HvfHostCapabilities {
    HvfHostCapabilities {
        available: false,
        host: "unsupported",
        default_ipa_bits: None,
        max_ipa_bits: None,
        el2_supported: None,
        blockers: vec![
            "Apple Hypervisor.framework Arm VM configuration is only available on Apple Silicon macOS".to_string(),
        ],
    }
}
