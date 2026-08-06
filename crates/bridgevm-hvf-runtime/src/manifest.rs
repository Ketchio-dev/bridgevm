//! The versioned launch manifest: what a product launch is allowed to say.
//!
//! JSON, hand-parsed with the same helpers style as snapshot_manifest_json in
//! bridgevm-hvf: six fields do not justify a serde dependency in a crate
//! whose value is its small trusted surface. The version field is checked
//! first and unknown versions are refused, not guessed at.

use crate::RuntimeError;
use std::path::Path;

/// The one manifest version this runtime speaks.
pub const MANIFEST_VERSION: u32 = 1;

/// A validated launch request. Construction is the validation: no public
/// field can hold a value `parse` would have refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchManifest {
    disk: String,
    uefi_vars: String,
    ram_mib: u64,
    vcpus: u32,
}

impl LaunchManifest {
    pub fn disk(&self) -> &str {
        &self.disk
    }
    pub fn uefi_vars(&self) -> &str {
        &self.uefi_vars
    }
    pub fn ram_mib(&self) -> u64 {
        self.ram_mib
    }
    pub fn vcpus(&self) -> u32 {
        self.vcpus
    }

    /// Parse and validate one manifest.
    ///
    /// `product` turns on the release-only refusals (repository paths).
    /// Evidence harnesses pass `false`: they legitimately run from a repo.
    pub fn parse(text: &str, product: bool) -> Result<Self, RuntimeError> {
        let version = field_u64(text, "version")?;
        if version != u64::from(MANIFEST_VERSION) {
            return Err(RuntimeError::UnsupportedManifestVersion {
                found: version.try_into().unwrap_or(u32::MAX),
            });
        }
        let disk = field_str(text, "disk")?;
        let uefi_vars = field_str(text, "uefi_vars")?;
        let ram_mib = field_u64(text, "ram_mib")?;
        let vcpus = field_u64(text, "vcpus")?;
        if !(1024..=65_536).contains(&ram_mib) {
            return Err(RuntimeError::ManifestField {
                field: "ram_mib",
                problem: "must be between 1024 and 65536",
            });
        }
        if !(1..=64).contains(&vcpus) {
            return Err(RuntimeError::ManifestField {
                field: "vcpus",
                problem: "must be between 1 and 64",
            });
        }
        if disk == uefi_vars {
            return Err(RuntimeError::DuplicateDiskWriter { path: disk });
        }
        if product {
            for (field, value) in [("disk", &disk), ("uefi_vars", &uefi_vars)] {
                if looks_like_repository(Path::new(value)) {
                    return Err(RuntimeError::RepositoryPathInProduct { field });
                }
            }
        }
        Ok(Self {
            disk,
            uefi_vars,
            ram_mib,
            vcpus: vcpus.try_into().expect("range-checked above"),
        })
    }
}

/// A path is "in a repository" when any ancestor holds a `.git`. Name-based
/// guesses (contains "Projects", ends in ".sh") would both over- and
/// under-match; the marker directory is the thing that makes it a repo.
fn looks_like_repository(path: &Path) -> bool {
    path.ancestors().any(|dir| dir.join(".git").exists())
}

fn field_str(text: &str, key: &str) -> Result<String, RuntimeError> {
    let value = raw_field(text, key)?;
    let value = value
        .strip_prefix('"')
        .and_then(|v| v.strip_suffix('"'))
        .ok_or(RuntimeError::ManifestField {
            field: leak(key),
            problem: "must be a JSON string",
        })?;
    if value.is_empty() {
        return Err(RuntimeError::ManifestField {
            field: leak(key),
            problem: "must not be empty",
        });
    }
    if value.contains('\\') {
        // The four fields are paths and numbers; no legal value needs an
        // escape, and refusing them keeps this parser honest about its size.
        return Err(RuntimeError::ManifestField {
            field: leak(key),
            problem: "escape sequences are not accepted",
        });
    }
    Ok(value.to_string())
}

fn field_u64(text: &str, key: &str) -> Result<u64, RuntimeError> {
    raw_field(text, key)?
        .parse()
        .map_err(|_| RuntimeError::ManifestField {
            field: leak(key),
            problem: "must be an unsigned integer",
        })
}

/// The raw token after `"key":`, up to the next comma or closing brace.
fn raw_field<'a>(text: &'a str, key: &str) -> Result<&'a str, RuntimeError> {
    let needle = format!("\"{key}\"");
    let start = text.find(&needle).ok_or(RuntimeError::ManifestField {
        field: leak(key),
        problem: "missing",
    })?;
    let after = &text[start + needle.len()..];
    let after = after
        .trim_start()
        .strip_prefix(':')
        .ok_or(RuntimeError::ManifestField {
            field: leak(key),
            problem: "missing ':'",
        })?
        .trim_start();
    let end = after
        .char_indices()
        .scan(false, |in_string, (i, c)| {
            match c {
                '"' => *in_string = !*in_string,
                ',' | '}' if !*in_string => return Some(Some(i)),
                _ => {}
            }
            Some(None)
        })
        .flatten()
        .next()
        .unwrap_or(after.len());
    Ok(after[..end].trim_end())
}

/// The error type carries `&'static str` field names so it stays `Send` and
/// allocation-free; the keys are the four compile-time constants above, so
/// interning them is bounded.
fn leak(key: &str) -> &'static str {
    match key {
        "version" => "version",
        "disk" => "disk",
        "uefi_vars" => "uefi_vars",
        "ram_mib" => "ram_mib",
        "vcpus" => "vcpus",
        _ => "unknown",
    }
}

#[cfg(test)]
#[path = "manifest_tests.rs"]
mod tests;
