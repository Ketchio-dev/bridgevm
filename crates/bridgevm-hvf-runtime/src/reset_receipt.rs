//! The reset receipt: proof that storage was flushed before the helper died.
//!
//! PLAN.md R1 fixes the product SYSTEM_RESET order: flush disk/vars/vTPM,
//! fsync a receipt, exit the helper, and only then may the supervisor start a
//! fresh process. The receipt is the load-bearing step -- a supervisor that
//! restarts without one may hand the new VM a disk whose last writes are
//! still in the dead helper's page cache. So the receipt is written only
//! after every named file has been `sync_all`ed, and is itself written
//! atomically (temp, fsync, rename, fsync parent) in the snapshot_pair
//! style: a crash leaves either a complete receipt or none.

use crate::error::RuntimeError;
use crate::reset_generation::GenerationTag;
use std::fs::{self, File};
use std::path::Path;

/// Flush every file, then write the receipt. Order is the contract: the
/// receipt names files that are already durable, never files that are about
/// to be. Any failure aborts before the receipt exists, so the supervisor's
/// "no receipt, no restart" rule holds with no further coordination.
pub fn flush_and_write_receipt(
    flushed: &[&Path],
    receipt: &Path,
    generation: GenerationTag,
) -> Result<(), RuntimeError> {
    for path in flushed {
        File::open(path)
            .and_then(|file| file.sync_all())
            .map_err(|source| RuntimeError::Io {
                context: "flush storage before reset",
                source,
            })?;
    }
    let body = render(flushed, generation);
    write_atomically(receipt, body.as_bytes()).map_err(|source| RuntimeError::Io {
        context: "write reset receipt",
        source,
    })
}

/// Whether `receipt` proves a flush for `generation`. A receipt from another
/// generation is a stale artifact of an earlier reset and proves nothing
/// about this one.
pub fn receipt_proves_flush(receipt: &Path, generation: GenerationTag) -> bool {
    fs::read_to_string(receipt).is_ok_and(|body| {
        body.lines()
            .any(|line| line == format!("generation: {}", generation.value()))
    })
}

fn render(flushed: &[&Path], generation: GenerationTag) -> String {
    let mut body = format!("generation: {}\n", generation.value());
    for path in flushed {
        body.push_str(&format!("flushed: {}\n", path.display()));
    }
    body
}

/// Temp file, fsync, rename, fsync parent: a crash at any point leaves the
/// destination either absent or complete, never truncated.
fn write_atomically(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let parent = path.parent().unwrap_or(Path::new("."));
    let tmp = parent.join(format!(
        ".{}.tmp",
        path.file_name()
            .map(|n| n.to_string_lossy())
            .unwrap_or_default()
    ));
    fs::write(&tmp, bytes)?;
    File::open(&tmp)?.sync_all()?;
    fs::rename(&tmp, path)?;
    File::open(parent)?.sync_all()
}

#[cfg(test)]
#[path = "reset_receipt_tests.rs"]
mod tests;
