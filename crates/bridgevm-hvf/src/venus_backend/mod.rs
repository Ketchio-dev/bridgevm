//! venus_backend, split for the 1000-line rule.
mod map_resource_ref;
mod poll_stall;
mod threaded_scanout;
mod venus_start_trace_capset_cou;
mod virgl_renderer_gl_context_mod;

pub(crate) use map_resource_ref::*;
pub(crate) use poll_stall::*;
pub use virgl_renderer_gl_context_mod::*;

#[cfg(test)]
mod tests;
