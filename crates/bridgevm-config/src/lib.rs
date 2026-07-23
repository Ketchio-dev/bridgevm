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
