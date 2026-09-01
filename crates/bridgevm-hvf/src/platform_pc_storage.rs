//! Storage attachment surface for the experimental BridgeVM Virtual ARM PC.

use super::BridgeVmPcPlatform;
use std::io;
use std::path::Path;

impl BridgeVmPcPlatform {
    /// Replace the primary NVMe namespace with a small in-memory image.
    pub fn load_nvme_disk_image(&mut self, image: Vec<u8>) {
        self.nvme.load_disk_image(image);
    }

    /// Attach a host raw disk without reading the whole image into memory.
    /// Guest writes remain in a sparse overlay unless `write_back` is true.
    pub fn attach_nvme_raw_file(
        &mut self,
        path: impl AsRef<Path>,
        write_back: bool,
    ) -> io::Result<()> {
        self.nvme.load_raw_file(path, write_back)
    }
}
