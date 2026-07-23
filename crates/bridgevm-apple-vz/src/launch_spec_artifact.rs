//! Persisting and reloading the launch spec JSON inside the bundle.

use crate::*;
use std::fs;
use std::io::Read;
use std::path::Path;
use std::path::PathBuf;

pub fn launch_spec_path(bundle_path: &Path) -> PathBuf {
    bundle_path.join("metadata").join("apple-vz-launch.json")
}

pub fn write_launch_spec_artifact(
    bundle_path: &Path,
    spec: &AppleVzLaunchSpec,
) -> Result<PathBuf, AppleVzLaunchSpecArtifactError> {
    let path = launch_spec_path(bundle_path);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| {
            AppleVzLaunchSpecArtifactError::CreateDirectory {
                path: parent.to_path_buf(),
                source,
            }
        })?;
    }
    let json = serde_json::to_string_pretty(spec)?;
    fs::write(&path, json).map_err(|source| AppleVzLaunchSpecArtifactError::Write {
        path: path.clone(),
        source,
    })?;
    Ok(path)
}

pub fn read_launch_spec_artifact(
    path: &Path,
) -> Result<AppleVzLaunchSpec, AppleVzLaunchSpecArtifactError> {
    const MAX_LAUNCH_SPEC_BYTES: u64 = 1024 * 1024;
    let mut bytes = Vec::new();
    fs::File::open(path)
        .and_then(|file| file.take(MAX_LAUNCH_SPEC_BYTES + 1).read_to_end(&mut bytes))
        .map_err(|source| AppleVzLaunchSpecArtifactError::Read {
            path: path.to_path_buf(),
            source,
        })?;
    if bytes.len() as u64 > MAX_LAUNCH_SPEC_BYTES {
        return Err(AppleVzLaunchSpecArtifactError::TooLarge {
            path: path.to_path_buf(),
            maximum: MAX_LAUNCH_SPEC_BYTES,
        });
    }
    serde_json::from_slice(&bytes).map_err(|source| AppleVzLaunchSpecArtifactError::Deserialize {
        path: path.to_path_buf(),
        source,
    })
}
