//! Host durability of a guest NVMe FLUSH or FUA write. The default is
//! `F_FULLFSYNC` (via `sync_data`), which survives power loss.
//! `BRIDGEVM_NVME_FLUSH=fsync` is an opt-in measurement mode matching QEMU's
//! macOS behaviour (plain `fsync`; recently flushed data may not survive power loss).

use std::fs::File;
use std::io;
use std::os::fd::AsRawFd;
use std::sync::OnceLock;

pub(crate) fn sync(file: &File) -> io::Result<()> {
    static FSYNC_ONLY: OnceLock<bool> = OnceLock::new();
    if *FSYNC_ONLY.get_or_init(|| std::env::var("BRIDGEVM_NVME_FLUSH").as_deref() == Ok("fsync")) {
        // SAFETY: `file` owns a live descriptor for the whole call.
        if unsafe { libc::fsync(file.as_raw_fd()) } != 0 {
            return Err(io::Error::last_os_error());
        }
        return Ok(());
    }
    file.sync_data()
}
