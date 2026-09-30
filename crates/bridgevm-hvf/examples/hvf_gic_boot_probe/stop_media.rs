//! Stop-time media persistence and its records in the final host report.
//!
//! Each write prints its `NVMe disk written back: PATH (N bytes)` line when it
//! completes, before the final report and in the same run.log as guest agent
//! output. The same outcomes come back as `host media: ` records, which
//! final_report.rs prints on the lines right after the report's stop record,
//! inside host framing; scripts/live-gates/hvf_host_media.py and
//! scripts/hvf-terminal-report.sh read them only there. A failed write panics
//! before the report, so no framed report exists and every reader fails closed.

use crate::guest_text::escape_guest_text;
use crate::*;
use bridgevm_hvf::snapshot_pair::managed::runtime::{RuntimeLease, RuntimeMediaSlot};
use std::os::unix::ffi::OsStrExt;

/// Prefix of each framed persistence record.
pub(crate) const HOST_MEDIA_RECORD: &str = "host media: ";

/// Persists the UEFI vars and each configured NVMe namespace, printing each
/// completed write now, and returns the framed records in write order.
pub(crate) fn persist_stop_media(
    platform: &mut VirtPlatform,
    media: &VirtBootMediaConfig,
    owner: &mut RuntimeLease,
) -> Vec<String> {
    let vars = owner
        .persist(RuntimeMediaSlot::Vars, platform.flash_vars_image())
        .unwrap_or_else(|e| panic!("persist UEFI vars: {e}"));
    let mut records = report_media_writes("UEFI vars", &vars);
    for (disk, namespace) in [
        (media.nvme_disk.as_ref(), NvmePersistNamespace::Primary),
        (media.nvme_target.as_ref(), NvmePersistNamespace::Target),
    ] {
        if let Some(disk) = disk {
            let writes = persist_nvme_media(platform, disk, namespace, owner);
            records.extend(report_media_writes(namespace.subject(), &writes));
        }
    }
    records
}

/// Prints each write's unframed line and returns its framed record. The host
/// path is escaped like guest text, so every record stays one ASCII line.
fn report_media_writes(subject: &str, writes: &[MediaWrite]) -> Vec<String> {
    print_media_writes(subject, writes);
    writes
        .iter()
        .map(|write| {
            format!(
                "{HOST_MEDIA_RECORD}{}: {} ({} bytes)",
                write.kind.label(subject),
                escape_guest_text(write.path.as_os_str().as_bytes()),
                write.bytes
            )
        })
        .collect()
}

#[cfg(test)]
#[path = "stop_media_tests.rs"]
mod tests;
