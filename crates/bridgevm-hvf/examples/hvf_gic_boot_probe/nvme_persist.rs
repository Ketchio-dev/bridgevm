//! Persisting the NVMe namespaces at stop.
//!
//! Split out so the stop path can call one function rather than repeating the
//! optional-namespace dance, without either file growing past its ceiling.

use crate::*;

/// Persist whichever NVMe namespaces are present and print what each wrote.
pub(crate) fn persist_both_nvme_namespaces(
    platform: &mut VirtPlatform,
    media: &VirtBootMediaConfig,
) {
    for (disk, namespace) in [
        (media.nvme_disk.as_ref(), NvmePersistNamespace::Primary),
        (media.nvme_target.as_ref(), NvmePersistNamespace::Target),
    ] {
        if let Some(disk) = disk {
            let writes = persist_nvme_media(platform, disk, namespace);
            print_media_writes(namespace.subject(), &writes);
        }
    }
}
