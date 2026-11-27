//! Reading a file into a fixed-size region.
//!
//! Split from media.rs: this is the input-side bound (a firmware blob must fit
//! the region it is mapped into), the counterpart to the atomic writes in
//! snapshot_pair. Both are I/O boundary rules rather than media description.

use std::fs;
use std::io::{self, Read};
use std::path::Path;

pub fn read_bounded_file(path: impl AsRef<Path>, max_bytes: usize) -> io::Result<Vec<u8>> {
    let path = path.as_ref();
    let mut file = fs::File::open(path)?;
    let file_bytes = file.metadata()?.len();
    let max_bytes_u64 = u64::try_from(max_bytes).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{} byte limit does not fit in u64", max_bytes),
        )
    })?;
    if file_bytes > max_bytes_u64 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "{} is {} bytes, larger than the {} byte region",
                path.display(),
                file_bytes,
                max_bytes
            ),
        ));
    }
    let read_limit = max_bytes_u64.checked_add(1).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "{} byte limit cannot reserve an overflow sentinel",
                max_bytes
            ),
        )
    })?;
    let mut data = Vec::new();
    file.by_ref().take(read_limit).read_to_end(&mut data)?;
    if data.len() > max_bytes {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "{} is {} bytes, larger than the {} byte region",
                path.display(),
                data.len(),
                max_bytes
            ),
        ));
    }
    Ok(data)
}

/// Read a file into the start of `region`, leaving the rest as it is, and return its
/// length; a file larger than the region is refused. Unlike [`read_bounded_file`] it needs
/// no buffer of its own: macOS malloc keeps a freed 64 MiB buffer resident for the run.
pub fn read_file_into(path: impl AsRef<Path>, region: &mut [u8]) -> io::Result<usize> {
    let path = path.as_ref();
    let mut file = fs::File::open(path)?;
    let file_bytes = file.metadata()?.len();
    let Some(len) = usize::try_from(file_bytes)
        .ok()
        .filter(|len| *len <= region.len())
    else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "{} is {} bytes, larger than the {} byte region",
                path.display(),
                file_bytes,
                region.len()
            ),
        ));
    };
    file.read_exact(&mut region[..len])?;
    Ok(len)
}
