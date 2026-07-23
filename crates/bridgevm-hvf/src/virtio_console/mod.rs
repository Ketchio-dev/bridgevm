//! virtio_console, split for the 1000-line rule.
#[macro_use]
mod console_trace;
mod default;

pub use console_trace::*;
pub use default::*;

#[cfg(test)]
mod tests;

mod console_trace_impl_2;
