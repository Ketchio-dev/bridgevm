//! Explicit boot-media modes for the sealed probe and Windows diagnostics.

use bridgevm_hvf::platform_pc::BridgeVmPcPlatform;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

const EXPECTED_MEDIA: &str = "a49be97db44c0d68b3382f3b1e46eba2fc7a3b12bcba14c1ec720f0511b71979";

pub(super) struct MediaIdentity {
    pub(super) byte_len: u64,
    pub(super) ram_bytes: u64,
    pub(super) sha256: Option<String>,
}

pub(super) enum BootMedia {
    Sealed { bytes: Vec<u8>, sha256: String },
    WindowsRaw { path: PathBuf, byte_len: u64 },
}

impl BootMedia {
    pub(super) fn open(path: &Path, windows_raw: bool) -> Result<Self, String> {
        if windows_raw {
            let metadata = std::fs::symlink_metadata(path)
                .map_err(|error| format!("inspect raw disk {}: {error}", path.display()))?;
            if !metadata.file_type().is_file() || metadata.len() == 0 || metadata.len() % 512 != 0 {
                return Err(
                    "raw disk must be a non-empty regular file aligned to 512 bytes".into(),
                );
            }
            return Ok(Self::WindowsRaw {
                path: path.to_path_buf(),
                byte_len: metadata.len(),
            });
        }
        let bytes = std::fs::read(path)
            .map_err(|error| format!("read boot media {}: {error}", path.display()))?;
        let sha256 = Sha256::digest(&bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        if sha256 != EXPECTED_MEDIA {
            return Err(format!("unexpected boot-media hash: {sha256}"));
        }
        Ok(Self::Sealed { bytes, sha256 })
    }

    pub(super) fn is_windows_diagnostic(&self) -> bool {
        matches!(self, Self::WindowsRaw { .. })
    }

    pub(super) fn identity(&self) -> MediaIdentity {
        match self {
            Self::Sealed { bytes, sha256 } => MediaIdentity {
                byte_len: bytes.len() as u64,
                ram_bytes: 512 << 20,
                sha256: Some(sha256.clone()),
            },
            Self::WindowsRaw { byte_len, .. } => MediaIdentity {
                byte_len: *byte_len,
                ram_bytes: 6144 << 20,
                sha256: None,
            },
        }
    }

    pub(super) fn attach(self, platform: &mut BridgeVmPcPlatform) -> Result<(), String> {
        match self {
            Self::Sealed { bytes, .. } => {
                platform.load_nvme_disk_image(bytes);
                Ok(())
            }
            Self::WindowsRaw { path, .. } => platform
                .attach_nvme_raw_file(path, false)
                .map_err(|error| format!("attach raw disk COW: {error}")),
        }
    }
}
