//! Host-file media plumbing for the Path A `virt` platform.
//!
//! The live probes and the eventual engine-facing VM configuration both need the
//! same boring but important behavior: load bounded firmware/media files and
//! persist writable guest state (UEFI vars, raw disks) either to an explicit
//! snapshot path or back to the input file.

use std::{
    env, fs, io,
    path::{Path, PathBuf},
};

pub const DEFAULT_QEMU_AARCH64_CODE: &str =
    "/opt/homebrew/Cellar/qemu/11.0.1/share/qemu/edk2-aarch64-code.fd";
pub const DEFAULT_QEMU_AARCH64_VARS: &str =
    "/opt/homebrew/Cellar/qemu/11.0.1/share/qemu/edk2-arm-vars.fd";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaWriteKind {
    Snapshot,
    WriteBack,
}

impl MediaWriteKind {
    pub fn label(self, subject: &str) -> String {
        match self {
            Self::Snapshot => format!("{subject} snapshot written"),
            Self::WriteBack => format!("{subject} written back"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaWrite {
    pub kind: MediaWriteKind,
    pub path: PathBuf,
    pub bytes: usize,
}

/// A writable host-file media slot with optional snapshot/writeback policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WritableMedia {
    pub path: PathBuf,
    pub snapshot_path: Option<PathBuf>,
    pub write_back: bool,
}

impl WritableMedia {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            snapshot_path: None,
            write_back: false,
        }
    }

    pub fn with_snapshot_path(mut self, path: Option<impl Into<PathBuf>>) -> Self {
        self.snapshot_path = path.map(Into::into);
        self
    }

    pub fn with_write_back(mut self, write_back: bool) -> Self {
        self.write_back = write_back;
        self
    }

    pub fn read_bounded(&self, max_bytes: usize) -> io::Result<Vec<u8>> {
        read_bounded_file(&self.path, max_bytes)
    }

    pub fn persist(&self, bytes: &[u8]) -> io::Result<Vec<MediaWrite>> {
        let mut writes = Vec::new();
        if let Some(path) = self.snapshot_path.as_ref() {
            fs::write(path, bytes)?;
            writes.push(MediaWrite {
                kind: MediaWriteKind::Snapshot,
                path: path.clone(),
                bytes: bytes.len(),
            });
        }
        if self.write_back {
            fs::write(&self.path, bytes)?;
            writes.push(MediaWrite {
                kind: MediaWriteKind::WriteBack,
                path: self.path.clone(),
                bytes: bytes.len(),
            });
        }
        Ok(writes)
    }
}

/// Path A boot media selected for a live run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VirtBootMediaConfig {
    pub firmware_code_path: PathBuf,
    pub flash_vars: WritableMedia,
    pub nvme_disk: Option<WritableMedia>,
}

impl VirtBootMediaConfig {
    pub fn qemu_defaults() -> Self {
        Self {
            firmware_code_path: PathBuf::from(DEFAULT_QEMU_AARCH64_CODE),
            flash_vars: WritableMedia::new(DEFAULT_QEMU_AARCH64_VARS),
            nvme_disk: None,
        }
    }

    pub fn from_probe_env() -> Self {
        let mut cfg = Self::qemu_defaults();
        if let Ok(path) = env::var("BRIDGEVM_AARCH64_UEFI_CODE") {
            cfg.firmware_code_path = PathBuf::from(path);
        }
        if let Ok(path) = env::var("BRIDGEVM_AARCH64_UEFI_VARS") {
            cfg.flash_vars.path = PathBuf::from(path);
        }
        cfg.flash_vars.snapshot_path = env::var("BRIDGEVM_AARCH64_UEFI_VARS_OUT")
            .ok()
            .map(PathBuf::from);
        cfg.flash_vars.write_back = env_flag("BRIDGEVM_AARCH64_UEFI_VARS_WRITABLE");

        cfg.nvme_disk = env::var("BRIDGEVM_NVME_DISK").ok().map(|path| {
            WritableMedia::new(path)
                .with_snapshot_path(env::var("BRIDGEVM_NVME_DISK_OUT").ok())
                .with_write_back(env_flag("BRIDGEVM_NVME_DISK_WRITABLE"))
        });
        cfg
    }
}

pub fn read_bounded_file(path: impl AsRef<Path>, max_bytes: usize) -> io::Result<Vec<u8>> {
    let path = path.as_ref();
    let data = fs::read(path)?;
    if data.len() > max_bytes {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "{} is {} bytes, larger than the {} byte region",
                path.display(),
                data.len(),
                max_bytes
            ),
        ));
    }
    Ok(data)
}

fn env_flag(name: &str) -> bool {
    matches!(
        env::var(name).ok().as_deref(),
        Some("1") | Some("true") | Some("TRUE") | Some("yes") | Some("YES")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_path(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        env::temp_dir().join(format!(
            "bridgevm-hvf-media-{name}-{}-{nanos}",
            std::process::id()
        ))
    }

    #[test]
    fn bounded_read_rejects_oversized_file() {
        let path = temp_path("oversized");
        fs::write(&path, [1, 2, 3, 4]).unwrap();
        let err = read_bounded_file(&path, 3).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        fs::remove_file(path).ok();
    }

    #[test]
    fn writable_media_persists_snapshot_and_writeback() {
        let source = temp_path("source");
        let snapshot = temp_path("snapshot");
        fs::write(&source, [0xaa, 0xbb]).unwrap();

        let media = WritableMedia::new(&source)
            .with_snapshot_path(Some(&snapshot))
            .with_write_back(true);
        let writes = media.persist(&[0x11, 0x22, 0x33]).unwrap();
        assert_eq!(writes.len(), 2);
        assert_eq!(writes[0].kind, MediaWriteKind::Snapshot);
        assert_eq!(writes[0].path, snapshot);
        assert_eq!(writes[0].bytes, 3);
        assert_eq!(writes[1].kind, MediaWriteKind::WriteBack);
        assert_eq!(writes[1].path, source);
        assert_eq!(fs::read(&writes[0].path).unwrap(), [0x11, 0x22, 0x33]);
        assert_eq!(fs::read(&writes[1].path).unwrap(), [0x11, 0x22, 0x33]);

        fs::remove_file(&writes[0].path).ok();
        fs::remove_file(&writes[1].path).ok();
    }

    #[test]
    fn qemu_defaults_are_the_stock_armvirtqemu_paths() {
        let cfg = VirtBootMediaConfig::qemu_defaults();
        assert_eq!(
            cfg.firmware_code_path,
            PathBuf::from(DEFAULT_QEMU_AARCH64_CODE)
        );
        assert_eq!(
            cfg.flash_vars.path,
            PathBuf::from(DEFAULT_QEMU_AARCH64_VARS)
        );
        assert!(cfg.nvme_disk.is_none());
    }
}
