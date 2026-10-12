//! Where a helper starts reading the legacy live-input channel.
//!
//! Product runs recreate the helper on a guest SYSTEM_RESET (exit 42); the
//! runner passes the same `BRIDGEVM_INPUT_CONTROL` with an advanced
//! `BRIDGEVM_RESET_GENERATION`, and the app keeps appending to the same inode.
//! The previous generation's consumed offset died with its process, so reading
//! from 0 would replay every earlier KEY/POINTER line into the new boot. Only
//! the first generation (unset, empty or `0`) reads lines written before it
//! started; any later or unrecognised generation starts at the channel end.

use super::InputControlFile;
use std::ffi::OsStr;

const GENERATION_ENV: &str = "BRIDGEVM_RESET_GENERATION";

/// Placeholder until the end can be sampled under the channel lock; the reader
/// resolves it on the first validated access instead of falling back to 0.
pub(super) const AT_END: u64 = u64::MAX;

pub(super) fn initial_offset(source: Option<&mut InputControlFile>) -> u64 {
    let generation = std::env::var_os(GENERATION_ENV);
    if first_generation(generation.as_deref()) {
        return 0;
    }
    source.map_or(0, |source| source.length().unwrap_or(AT_END))
}

fn first_generation(value: Option<&OsStr>) -> bool {
    value.is_none_or(|value| value.is_empty() || value == "0")
}
