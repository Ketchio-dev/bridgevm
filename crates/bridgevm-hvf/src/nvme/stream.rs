//! Stream namespace bytes without selecting or publishing a host path.
use super::{second_namespace_missing, NvmeController};
use std::io::{self, Write};

impl NvmeController {
    pub fn export_disk_into(&mut self, out: &mut dyn Write) -> io::Result<u64> {
        self.disk.export_into(out)
    }

    pub fn export_second_namespace_into(&mut self, out: &mut dyn Write) -> io::Result<u64> {
        self.disk2
            .as_mut()
            .ok_or_else(second_namespace_missing)?
            .export_into(out)
    }
}
