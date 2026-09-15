//! Shared snapshot-then-writeback policy; owners provide the write boundary.

use super::*;

impl WritableMedia {
    /// Persist through a temp file and rename, never a truncating write.
    ///
    /// These bytes are UEFI variables: boot order, Secure Boot state, and the
    /// firmware's own bookkeeping. A `fs::write` interrupted by a crash leaves
    /// a truncated vars file, and the guest no longer knows how to boot.
    /// Runtime media owners must instead use RuntimeLease::persist so atomic
    /// replacement transfers their cooperative inode ownership.
    pub fn persist(&self, bytes: &[u8]) -> io::Result<Vec<MediaWrite>> {
        self.persist_with(bytes, crate::snapshot_pair::write_file_atomically)
    }

    pub(crate) fn persist_with(
        &self,
        bytes: &[u8],
        mut write: impl FnMut(&std::path::Path, &[u8]) -> io::Result<()>,
    ) -> io::Result<Vec<MediaWrite>> {
        let mut writes = Vec::new();
        let mut put = |path: &PathBuf, kind| -> io::Result<()> {
            write(path, bytes)?;
            writes.push(MediaWrite {
                kind,
                path: path.clone(),
                bytes: bytes.len(),
            });
            Ok(())
        };
        if let Some(path) = self.snapshot_path.as_ref() {
            put(path, MediaWriteKind::Snapshot)?;
        }
        if self.write_back {
            put(&self.path, MediaWriteKind::WriteBack)?;
        }
        Ok(writes)
    }
}
