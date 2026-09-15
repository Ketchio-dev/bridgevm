//! Streaming SHA-256 over a file.
//!
//! Separate from snapshot_pair so the hash can be taken of a disk image tens
//! of gigabytes long without reading it into memory.

use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

const HASH_CHUNK: usize = 1024 * 1024;

pub(super) fn sha256_file_and_size(path: &Path) -> io::Result<(String, u64)> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; HASH_CHUNK];
    let mut bytes = 0;
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        bytes += n as u64;
    }
    let digest = hasher.finalize();
    let hash = digest.iter().map(|b| format!("{b:02x}")).collect();
    Ok((hash, bytes))
}
