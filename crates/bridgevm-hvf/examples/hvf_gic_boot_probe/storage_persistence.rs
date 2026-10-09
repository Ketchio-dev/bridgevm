//! Native NVMe persistence; snapshot publication uses the runtime owner.

use crate::*;
use bridgevm_hvf::snapshot_pair::managed::runtime::{RuntimeLease, RuntimeMediaSlot};

#[derive(Clone, Copy)]
pub(crate) enum NvmePersistNamespace {
    Primary,
    Target,
}

impl NvmePersistNamespace {
    fn slot(self) -> RuntimeMediaSlot {
        match self {
            Self::Primary => RuntimeMediaSlot::Primary,
            Self::Target => RuntimeMediaSlot::Target,
        }
    }
    pub(crate) fn subject(self) -> &'static str {
        match self {
            Self::Primary => "NVMe disk",
            Self::Target => "NVMe target namespace (NSID 2)",
        }
    }
    pub(crate) fn image_if_memory(self, platform: &VirtPlatform) -> Option<&[u8]> {
        match self {
            Self::Primary => platform.nvme_disk_if_memory(),
            Self::Target => platform.nvme_second_namespace_disk_if_memory(),
        }
    }
    pub(crate) fn export_snapshot(
        self,
        platform: &mut VirtPlatform,
        out: &mut dyn std::io::Write,
    ) -> std::io::Result<u64> {
        match self {
            Self::Primary => platform.export_nvme_disk_into(out),
            Self::Target => platform.export_nvme_second_namespace_into(out),
        }
    }
    pub(crate) fn flush(self, platform: &mut VirtPlatform) -> std::io::Result<()> {
        match self {
            Self::Primary => platform.flush_nvme_disk(),
            Self::Target => platform.flush_nvme_second_namespace_disk(),
        }
    }
    pub(crate) fn disk_len(self, platform: &VirtPlatform) -> u64 {
        match self {
            Self::Primary => platform.nvme_disk_len(),
            Self::Target => platform.nvme_second_namespace_disk_len().unwrap_or(0),
        }
    }
}

pub(crate) fn persist_nvme_media(
    platform: &mut VirtPlatform,
    media: &WritableMedia,
    namespace: NvmePersistNamespace,
    owner: &mut RuntimeLease,
) -> Vec<MediaWrite> {
    if let Some(image) = namespace.image_if_memory(platform) {
        return owner
            .persist(namespace.slot(), image)
            .unwrap_or_else(|e| panic!("persist {}: {e}", namespace.subject()));
    }

    let mut writes = Vec::new();
    if let Some(write) = owner
        .export_snapshot(namespace.slot(), |out| namespace.export_snapshot(platform, out))
        .unwrap_or_else(|e| panic!("export {} snapshot: {e}", namespace.subject()))
    {
        writes.push(write);
    }
    if media.write_back {
        namespace.flush(platform).unwrap_or_else(|e| {
            panic!(
                "flush {} {}: {e}",
                namespace.subject(),
                media.path.display()
            )
        });
        writes.push(MediaWrite {
            kind: MediaWriteKind::WriteBack,
            path: media.path.clone(),
            bytes: usize::try_from(namespace.disk_len(platform)).unwrap_or(usize::MAX),
        });
    }
    writes
}

#[cfg(test)]
#[path = "storage_persistence_tests.rs"]
mod tests;
