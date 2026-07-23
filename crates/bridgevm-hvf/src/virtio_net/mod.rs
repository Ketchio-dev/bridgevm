//! virtio_net, split for the 1000-line rule.
mod magic_value;
mod virtiopcinet;

pub use magic_value::*;
pub use virtiopcinet::*;

#[cfg(test)]
mod tests;
