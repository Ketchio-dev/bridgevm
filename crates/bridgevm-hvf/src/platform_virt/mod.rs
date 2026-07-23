//! platform_virt, split for the 1000-line rule.
mod bootorder;

mod default_nvme_disk_bytes;
mod flatguestram;

pub use default_nvme_disk_bytes::*;
pub use flatguestram::*;

#[cfg(test)]
mod tests;

mod default_nvme_disk_bytes_impl_2;
mod default_nvme_disk_bytes_impl_3;
mod default_nvme_disk_bytes_impl_4;
