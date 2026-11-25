mod backend;
mod poll_stall;
mod poll_watchdog;
mod renderer_extensions;
mod renderer_init;
mod threaded_scanout;
mod virtio_gpu_3d_backend;

pub use backend::*;
pub(crate) use poll_stall::*;
pub(crate) use poll_watchdog::*;
use renderer_extensions::renderer_extensions;
pub(crate) use renderer_extensions::renderer_has_iosurface_scanout;
pub(crate) use renderer_init::*;

#[cfg(test)]
mod tests;
