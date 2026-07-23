//! virtio_gpu, split for the 1000-line rule.
mod drop;
mod magic_value;
mod virtiopcigpu;
mod write_trace_command_response;

pub use magic_value::*;
pub use virtiopcigpu::*;
pub(crate) use write_trace_command_response::*;

#[cfg(test)]
mod tests;
