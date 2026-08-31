//! One stable BDS-evidence line shared by the proof and Windows reports.

use super::super::result::BootResult;

pub(super) fn line(boot: &BootResult) -> String {
    format!(
        "stage={} arch={:#x} filesystems={} image={:#x}+{:#x} gop_handles={} framebuffer={:#x}+{:#x}",
        boot.stage,
        boot.arch,
        boot.file_systems,
        boot.image_base,
        boot.image_size,
        boot.gop_handles,
        boot.framebuffer_base,
        boot.framebuffer_size
    )
}
