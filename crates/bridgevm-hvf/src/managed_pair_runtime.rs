//! Selection occurs before VM creation and every media read in the native probe.

use super::{layout, LockedPair};
use crate::media::{VirtBootMediaConfig, WritableMedia};
use crate::media_lease::MediaLease;
use std::{collections::BTreeSet, fs, io, path::PathBuf};

#[path = "managed_pair_runtime_policy.rs"]
mod policy;
use policy::Policies;
pub use policy::RuntimeMediaSlot;

pub struct RuntimeLease {
    _pair: Option<LockedPair>,
    _logical: MediaLease,
    policies: Policies,
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
