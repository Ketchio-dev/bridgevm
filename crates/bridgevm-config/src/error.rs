//! ConfigError: one variant per manifest failure mode.

use crate::*;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("manifest schema version must be {expected}, got {actual}")]
    UnsupportedSchema {
        expected: &'static str,
        actual: String,
    },
    #[error("manifest name cannot be empty")]
    EmptyName,
    #[error("manifest name '{name}' is not usable (it must contain at least one letter or digit)")]
    UnusableName { name: String },
    #[error(
        "manifest {field} must be a bundle-relative path (no absolute or '..' components): {value}"
    )]
    UnsafePath { field: &'static str, value: String },
    #[error("boot mode {mode} requires {field}")]
    MissingBootInput { mode: BootMode, field: &'static str },
    #[error("boot input {field} cannot be empty")]
    EmptyBootInput { field: &'static str },
    #[error("boot mode {mode} cannot use {field}")]
    UnsupportedBootInput { mode: BootMode, field: &'static str },
    #[error("shared folder {index} field {field} cannot be empty")]
    EmptySharedFolderField { index: usize, field: &'static str },
    #[error("duplicate shared folder name '{name}'")]
    DuplicateSharedFolderName { name: String },
    #[error("duplicate shared folder token '{token}'")]
    DuplicateSharedFolderToken { token: String },
    #[error("manifest is {actual} bytes, exceeding the {maximum}-byte limit")]
    ManifestTooLarge { actual: u64, maximum: u64 },
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("YAML error: {0}")]
    Yaml(#[from] serde_yaml::Error),
}
