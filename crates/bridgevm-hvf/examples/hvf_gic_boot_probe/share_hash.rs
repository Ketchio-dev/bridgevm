//! FNV-1a 64, the content hash share sync compares file versions by.

pub const FNV1A64_START: u64 = 0xcbf29ce484222325;

pub fn fnv1a64(bytes: &[u8]) -> u64 {
    fnv1a64_continue(FNV1A64_START, bytes)
}

/// The hash of everything hashed into `hash` so far, then `bytes`.
pub fn fnv1a64_continue(mut hash: u64, bytes: &[u8]) -> u64 {
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}
