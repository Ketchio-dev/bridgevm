//! From validated manifest to a launch-ready VM: leases before any VM exists.
//!
//! PLAN.md R1/H1: one process may write a disk image; the second writer must
//! fail *before launch*, not corrupt the guest at some later flush. The
//! builder is where that ordering lives -- it takes the exclusive writer
//! leases for the disk and the UEFI vars and only then hands back a
//! `PreparedVm` that a runtime can boot. A `PreparedVm` existing means the
//! leases are held; dropping it releases them. There is no path to a running
//! VM that skips this type.

use bridgevm_hvf::media::lock::{MediaLease, MediaLockError};

use crate::error::RuntimeError;
use crate::manifest::LaunchManifest;
use crate::reset_generation::ResetGeneration;
use crate::vm_event::VmEventQueue;

/// A VM whose images are exclusively leased and whose lifetime bookkeeping
/// (generation counter, event queue) exists, but which has not booted.
#[derive(Debug)]
pub struct PreparedVm {
    manifest: LaunchManifest,
    // Held for their Drop: the leases ARE the exclusivity.
    _disk_lease: MediaLease,
    _vars_lease: MediaLease,
    generation: ResetGeneration,
    events: VmEventQueue,
}

impl PreparedVm {
    pub fn manifest(&self) -> &LaunchManifest {
        &self.manifest
    }

    pub fn generation(&self) -> &ResetGeneration {
        &self.generation
    }

    pub fn events(&self) -> &VmEventQueue {
        &self.events
    }
}

/// Take the writer leases for everything the manifest opens writable.
///
/// `holder` names this process in the refusal message shown to whoever
/// loses the race. Lease order (disk, then vars) is fixed; with a single
/// builder per process and non-blocking locks there is no deadlock to
/// order around, but a fixed order keeps refusal messages deterministic.
pub fn prepare(manifest: LaunchManifest, holder: &str) -> Result<PreparedVm, RuntimeError> {
    let disk_lease = lease(manifest.disk(), holder)?;
    let vars_lease = lease(manifest.uefi_vars(), holder)?;
    Ok(PreparedVm {
        manifest,
        _disk_lease: disk_lease,
        _vars_lease: vars_lease,
        generation: ResetGeneration::new(),
        events: VmEventQueue::new(),
    })
}

fn lease(path: &str, holder: &str) -> Result<MediaLease, RuntimeError> {
    MediaLease::acquire(path.as_ref(), holder).map_err(|err| match err {
        MediaLockError::Held { path, holder } => RuntimeError::MediaHeld {
            path: path.display().to_string(),
            holder,
        },
        MediaLockError::Io { path, source } => RuntimeError::Io {
            context: "acquiring image writer lease",
            source: std::io::Error::new(source.kind(), format!("{}: {source}", path.display())),
        },
    })
}

#[cfg(test)]
#[path = "vm_builder_tests.rs"]
mod tests;
