//! Typed errors for every product-reachable failure.
//!
//! PLAN.md R1: the runtime's product paths must not `assert!`/`expect` on
//! guest input or host I/O -- they return one of these, with enough context
//! to act on and nothing that leaks a private path into a published receipt.

use std::fmt;

/// Why a launch or reset was refused or failed.
#[derive(Debug)]
pub enum RuntimeError {
    /// The manifest named a version this runtime does not speak. Refusing is
    /// the contract: silently reading a newer manifest with older rules would
    /// validate against the wrong policy.
    UnsupportedManifestVersion { found: u32 },
    /// A required field was missing or empty.
    ManifestField {
        field: &'static str,
        problem: &'static str,
    },
    /// The manifest pointed a product launch at the development repository.
    /// Products run from bundled, signed resources; a repo path in a release
    /// manifest is the shell-out defect A14 exists to remove.
    RepositoryPathInProduct { field: &'static str },
    /// Two writable attachments named the same file. The exclusive-writer
    /// lease would catch this at open; refusing at validation names it
    /// before a VM exists.
    DuplicateDiskWriter { path: String },
    /// Host I/O failed. Wrapped rather than propagated raw so the product
    /// path reports which file, not just which errno.
    Io {
        context: &'static str,
        source: std::io::Error,
    },
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedManifestVersion { found } => write!(
                f,
                "launch manifest version {found} is not supported (this runtime speaks {})",
                crate::MANIFEST_VERSION
            ),
            Self::ManifestField { field, problem } => {
                write!(f, "launch manifest field {field}: {problem}")
            }
            Self::RepositoryPathInProduct { field } => write!(
                f,
                "launch manifest field {field} points into a source repository; \
                 product launches run from bundled resources only"
            ),
            Self::DuplicateDiskWriter { path } => {
                write!(f, "two writable disks name the same file: {path}")
            }
            Self::Io { context, source } => write!(f, "{context}: {source}"),
        }
    }
}

impl std::error::Error for RuntimeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}
