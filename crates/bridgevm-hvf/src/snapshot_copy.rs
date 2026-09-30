//! Copy one media file into staging and make it durable.

use super::free_space;
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::Path;

/// Bytes copied per read/write when streaming a large disk image.
pub(super) const COPY_CHUNK: usize = 4 * 1024 * 1024;

/// Copy `src` to `dst` and fsync the result, returning bytes written.
///
/// Streamed rather than read wholesale: a disk image is tens of gigabytes and
/// must not be brought into memory to be copied.
pub(super) fn copy_and_sync(src: &Path, dst: &Path) -> io::Result<u64> {
    // Clone first where the filesystem allows it. On APFS this is a
    // copy-on-write reference: the 64 GiB image clones in 2ms and adds no used
    // bytes, where copying it needs minutes and room for a second full copy.
    // That difference is the whole reason a restore could not run on a volume
    // with 59 GiB free.
    if free_space::clone_file(src, dst).is_some() {
        let cloned = File::open(dst)?;
        // Still fsync: the clone is metadata, and the manifest is about to
        // claim these bytes are durable.
        cloned.sync_all()?;
        return Ok(cloned.metadata()?.len());
    }
    stream_and_sync(src, dst, |_| Ok(()))
}

/// The copy taken when cloning is refused, as across volumes. `after_chunk`
/// sees the running total; production observes nothing, and tests end the
/// copy there as a full volume or a killed process would.
pub(super) fn stream_and_sync(
    src: &Path,
    dst: &Path,
    mut after_chunk: impl FnMut(u64) -> io::Result<()>,
) -> io::Result<u64> {
    let mut input = File::open(src)?;
    let mut output = File::create(dst)?;
    let mut buf = vec![0u8; COPY_CHUNK];
    let mut total = 0u64;
    loop {
        let n = input.read(&mut buf)?;
        if n == 0 {
            break;
        }
        output.write_all(&buf[..n])?;
        total += n as u64;
        after_chunk(total)?;
    }
    output.sync_all()?;
    Ok(total)
}
