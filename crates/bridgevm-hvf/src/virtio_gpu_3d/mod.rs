//! virtio_gpu_3d, split for the 1000-line rule.
mod is_local_scanout_resource;
mod virtio_gpu_f_virgl;

pub use is_local_scanout_resource::*;
pub use virtio_gpu_f_virgl::*;

#[cfg(test)]
mod tests;

mod virtio_gpu_f_virgl_impl_2;
mod virtio_gpu_f_virgl_impl_3;
