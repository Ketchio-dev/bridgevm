//! Streaming SHA-256 over a file.
//!
//! Separate from snapshot_pair so the hash can be taken of a disk image tens
//! of gigabytes long without reading it into memory.

use std::io;
use std::path::Path;

#[path = "snapshot_hash_stream.rs"]
mod stream;
pub(super) use stream::sha256_reader;

pub(super) fn sha256_file_and_size(path: &Path) -> io::Result<(String, u64)> {
    sha256_reader(super::regular_file::open(path)?)
}
