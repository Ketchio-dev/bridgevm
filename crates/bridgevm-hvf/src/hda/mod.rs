//! hda, split for the 1000-line rule.
mod codec_parameter;
mod hdapcmsink;

pub(crate) use codec_parameter::*;
pub use hdapcmsink::*;

#[cfg(test)]
mod tests;
