//! Persist only the configured slots through the runtime's existing owner.

use super::*;
use crate::media::MediaWrite;
use std::path::Path;

impl RuntimeLease {
    /// Atomically persist a captured media policy while retaining ownership of
    /// every runtime input and output until this owner is dropped.
    pub fn persist(&mut self, slot: RuntimeMediaSlot, bytes: &[u8]) -> io::Result<Vec<MediaWrite>> {
        self.persist_using(slot, bytes, |staged, destination| {
            fs::rename(staged, destination)?;
            fs::File::open(destination.parent().unwrap())?.sync_all()
        })
    }

    pub(super) fn persist_using(
        &mut self,
        slot: RuntimeMediaSlot,
        bytes: &[u8],
        mut publish: impl FnMut(&Path, &Path) -> io::Result<()>,
    ) -> io::Result<Vec<MediaWrite>> {
        let policy = self.slots[slot as usize].clone().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "runtime media slot absent")
        })?;
        policy.persist_with(bytes, |path, bytes| {
            self.write_owned(path, bytes, &mut publish)
        })
    }

    pub(super) fn owner(&mut self) -> &mut MediaLease {
        match self._pair.as_mut() {
            Some(pair) => &mut pair._lease,
            None => &mut self._logical,
        }
    }
}

#[path = "managed_pair_runtime_atomic.rs"]
mod atomic;
