//! VM manifest types, schema and on-disk IO.
//!
//! Owns the persisted description of a VM: what it boots, which disks and devices
//! it has, and which engine it targets. Every other crate reads a VM through these
//! types rather than parsing bundle files itself.

#[cfg(test)]
#[path = "tests_split/mod.rs"]
mod tests;

mod error;
mod json_schema;
mod manifest_defaults;
mod manifest_io;
mod manifest_model;
mod naming;
pub(crate) mod validation;

pub use error::*;
pub use json_schema::*;
pub use manifest_io::*;
pub use manifest_model::*;
pub use naming::*;
