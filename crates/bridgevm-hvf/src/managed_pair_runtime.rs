//! Selection occurs before VM creation and every media read in the native probe.

use super::{layout, LockedPair};
use crate::media::{VirtBootMediaConfig, WritableMedia};
use crate::media_lease::MediaLease;
use std::{collections::BTreeSet, fs, io, path::PathBuf};

#[derive(Clone, Copy, Debug)]
pub enum RuntimeMediaSlot {
    Vars,
    Primary,
    Target,
}

pub struct RuntimeLease {
    _pair: Option<LockedPair>,
    _logical: MediaLease,
    slots: [Option<WritableMedia>; 3],
    retained: BTreeSet<PathBuf>,
}

#[path = "managed_pair_runtime_acquire.rs"]
mod acquisition;
pub use acquisition::acquire;
#[path = "managed_pair_runtime_persist.rs"]
mod persistence;

#[cfg(test)]
#[path = "managed_pair_runtime_tests.rs"]
mod tests;
