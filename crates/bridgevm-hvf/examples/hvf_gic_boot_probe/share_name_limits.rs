//! The longest share key that can sync. macOS path calls refuse a path of
//! PATH_MAX (1024) bytes or more, so the host scan could never see a file
//! under a longer key, and APFS, like NTFS in the guest, refuses a name over
//! NAME_MAX (255) UTF-16 code units. A guest listing such a key is refused.
//! That also bounds per-name work that grows with depth (the unseen-name check
//! hashes every parent of a key): a key under PATH_MAX bytes has at most 512
//! components.

/// macOS PATH_MAX (sys/syslimits.h), counting the terminating NUL.
pub(super) const PATH_MAX_BYTES: usize = 1024;
/// macOS NAME_MAX, which APFS, like NTFS, counts in UTF-16 code units.
const NAME_MAX_UTF16: usize = 255;

/// Whether a normalized (`/`-separated) share key fits both limits.
pub(super) fn fits_host_limits(key: &str) -> bool {
    key.len() < PATH_MAX_BYTES
        && key
            .split('/')
            .all(|part| part.encode_utf16().count() <= NAME_MAX_UTF16)
}
