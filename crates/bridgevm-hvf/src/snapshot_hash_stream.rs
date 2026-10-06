//! Streaming digests share the same read bytes with their reported size.

use sha2::{Digest, Sha256};
use std::io::{self, Read};

const HASH_CHUNK: usize = 1024 * 1024;

pub(in crate::snapshot_pair) fn sha256_reader(mut reader: impl Read) -> io::Result<(String, u64)> {
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; HASH_CHUNK];
    let mut bytes = 0;
    loop {
        let n = reader.read(&mut buf)?;
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
