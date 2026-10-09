//! Bounded NVMe export for a caller that owns the destination and publication.
use super::VirtPlatform;
use std::io::{self, Write};

impl VirtPlatform {
    pub fn export_nvme_disk_into(&mut self, out: &mut dyn Write) -> io::Result<u64> {
        self.nvme.export_disk_into(out)
    }

    pub fn export_nvme_second_namespace_into(&mut self, out: &mut dyn Write) -> io::Result<u64> {
        self.nvme.export_second_namespace_into(out)
    }
}
