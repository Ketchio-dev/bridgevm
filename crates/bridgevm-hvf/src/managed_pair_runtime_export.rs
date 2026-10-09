//! Stream raw snapshot output through the runtime's captured policy and owner.
#[cfg(test)]
#[path = "managed_pair_runtime_export_tests.rs"]
mod tests;

use super::*;
use crate::media::MediaWriteKind;
use std::io::Write;

impl RuntimeLease {
    /// Writes only the configured snapshot, never write-back media. The producer
    /// streams bytes; this owner controls staging, sync, publication and leases.
    pub fn export_snapshot(
        &mut self,
        slot: RuntimeMediaSlot,
        write: impl FnOnce(&mut dyn Write) -> io::Result<u64>,
    ) -> io::Result<Option<MediaWrite>> {
        self.export_snapshot_using(slot, write, |staged, destination| {
            fs::rename(staged, destination)?;
            fs::File::open(destination.parent().unwrap())?.sync_all()
        })
    }

    fn export_snapshot_using(
        &mut self,
        slot: RuntimeMediaSlot,
        write: impl FnOnce(&mut dyn Write) -> io::Result<u64>,
        mut publish: impl FnMut(&Path, &Path) -> io::Result<()>,
    ) -> io::Result<Option<MediaWrite>> {
        let policy = self.policies.slot(slot)?;
        let Some(path) = policy.snapshot_path else {
            return Ok(None);
        };
        if let RuntimeMediaSlot::Primary | RuntimeMediaSlot::Target = slot {
            let index = slot as usize - 1;
            if !self.raw_source_pinned[index] {
                self.owner().pin([policy.path.as_path()])?;
                // Preserve the open source, not successive self-export generations.
                self.raw_source_pinned[index] = true;
            }
        }
        let bytes = self.write_owned_with(
            &path,
            |out| {
                usize::try_from(write(out)?)
                    .map_err(|_| io::Error::other("snapshot byte count exceeds host size"))
            },
            &mut publish,
        )?;
        Ok(Some(MediaWrite {
            kind: MediaWriteKind::Snapshot,
            path,
            bytes,
        }))
    }
}
