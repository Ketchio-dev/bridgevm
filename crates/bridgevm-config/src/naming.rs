//! Name slugging and stable share-token derivation.

pub(crate) fn stable_share_token(name: &str, host_path: &str) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in name
        .as_bytes()
        .iter()
        .copied()
        .chain([0])
        .chain(host_path.as_bytes().iter().copied())
    {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("share-{hash:016x}")
}

pub fn slug(value: &str) -> String {
    value
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}
