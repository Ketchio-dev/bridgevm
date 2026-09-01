//! SHA-256 rendering for sealed live-run inputs and outputs.

use sha2::{Digest, Sha256};

pub(super) fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
