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

mod magic_value_impl_2;
mod magic_value_impl_3;
mod magic_value_impl_4;
mod magic_value_impl_5;
mod magic_value_impl_6;

mod virtiopcigpu_impl_2;
