//! Bounded read of a manifest file and the validated read/write round-trip.

use crate::*;
use std::fs;
use std::io::Read;
use std::path::Path;

pub const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;

pub fn read_manifest_bytes(path: &Path) -> Result<Vec<u8>, ConfigError> {
    let file = fs::File::open(path)?;
    let size = file.metadata()?.len();
    if size > MAX_MANIFEST_BYTES {
        return Err(ConfigError::ManifestTooLarge {
            actual: size,
            maximum: MAX_MANIFEST_BYTES,
        });
    }
    let capacity = usize::try_from(size).map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "manifest size exceeds host address space",
        )
    })?;
    let mut bytes = Vec::with_capacity(capacity);
    file.take(MAX_MANIFEST_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_MANIFEST_BYTES {
        return Err(ConfigError::ManifestTooLarge {
            actual: bytes.len() as u64,
            maximum: MAX_MANIFEST_BYTES,
        });
    }
    Ok(bytes)
}

impl VmManifest {
    pub fn read(path: &Path) -> Result<Self, ConfigError> {
        let bytes = read_manifest_bytes(path)?;
        let manifest = serde_yaml::from_slice::<Self>(&bytes)?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn write(&self, path: &Path) -> Result<(), ConfigError> {
        self.validate()?;
        fs::write(path, serde_yaml::to_string(self)?)?;
        Ok(())
    }
}
