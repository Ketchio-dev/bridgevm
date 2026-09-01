//! acpi, split for the 1000-line rule.
mod acpi_header_len;
mod aml_field_length;
mod bridgevm_pc;

pub use acpi_header_len::*;
pub(crate) use aml_field_length::*;
pub use bridgevm_pc::*;

#[cfg(test)]
mod tests;
