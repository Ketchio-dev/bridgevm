//! Virtio-FS share specs, unique tag derivation, and share flag encoding.

use bridgevm_config::SharedFolder;
use serde::Deserialize;
use serde::Serialize;

/// Single Virtio-FS shared directory destined for the AppleVzRunner helper via a
/// repeatable `--share <tag>=<host_path>` (optionally `ro:`-prefixed) flag.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppleVzShareSpec {
    /// Host path of the shared directory.
    pub host_path: String,
    /// Share/mount tag (the folder `name`, or a derived `"share"`/`"share-N"`).
    pub tag: String,
    /// When true, the directory is shared read-only.
    pub read_only: bool,
}

/// Encode a single share as one `--share` flag value.
///
/// Grammar (consumed verbatim by the Swift AppleVzRunner `--share` parser):
///
/// ```text
/// share-value := [ "ro:" ] tag "=" host-path
/// ```
///
/// The optional `ro:` prefix marks the share read-only. `tag` is everything up
/// to the FIRST `=`; `host-path` is the remainder. Splitting on the first `=`
/// keeps host paths that contain `=`, spaces, or commas intact, and VZ share
/// tags are validated to exclude `=`, so the boundary is unambiguous. Tags here
/// are never empty (callers derive a non-empty tag), so the value never starts
/// with a bare `=`.
pub fn encode_share_flag_value(share: &AppleVzShareSpec) -> String {
    let prefix = if share.read_only { "ro:" } else { "" };
    format!("{prefix}{}={}", share.tag, share.host_path)
}

/// Default Virtio-FS share tag, matching the AppleVzRunner Swift default
/// (`AppleVzSharedDirectorySpec` tag "share").
pub(crate) const DEFAULT_SHARE_TAG: &str = "share";

/// Build the Virtio-FS shares to hand to the AppleVzRunner helper.
///
/// Returns an empty `Vec` unless `integration.shared_folders` is enabled. When
/// enabled, EVERY approved `SharedFolder` is mapped to an `AppleVzShareSpec` so
/// the Swift side can attach all of them (a `VZSingleDirectoryShare` for one, a
/// `VZMultipleDirectoryShare` for 2+).
///
/// VZ requires every share tag to be unique. Each folder's tag is its `name`
/// (trimmed); empty names get the default `"share"` tag. To keep tags unique,
/// any tag that collides with an earlier one is disambiguated by appending
/// `-2`, `-3`, ... (and so on until unique), so unnamed/duplicate folders never
/// clash.
pub(crate) fn build_share_specs(
    shared_folders_enabled: bool,
    shared_folders: &[SharedFolder],
) -> Vec<AppleVzShareSpec> {
    if !shared_folders_enabled {
        return Vec::new();
    }
    let mut used_tags: Vec<String> = Vec::with_capacity(shared_folders.len());
    shared_folders
        .iter()
        .map(|folder| {
            let base = if folder.name.trim().is_empty() {
                DEFAULT_SHARE_TAG.to_string()
            } else {
                folder.name.clone()
            };
            let tag = unique_tag(&base, &used_tags);
            used_tags.push(tag.clone());
            AppleVzShareSpec {
                host_path: folder.host_path.clone(),
                tag,
                read_only: folder.read_only,
            }
        })
        .collect()
}

/// Derive a tag that does not collide with any already-used tag by appending a
/// numeric suffix (`-2`, `-3`, ...) until unique.
pub(crate) fn unique_tag(base: &str, used: &[String]) -> String {
    if !used.iter().any(|tag| tag == base) {
        return base.to_string();
    }
    let mut suffix = 2;
    loop {
        let candidate = format!("{base}-{suffix}");
        if !used.iter().any(|tag| tag == &candidate) {
            return candidate;
        }
        suffix += 1;
    }
}
