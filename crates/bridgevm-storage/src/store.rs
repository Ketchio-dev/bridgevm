//! VmStore itself: root discovery, bundle path derivation, directory bootstrap.

use crate::*;
use bridgevm_config::slug;
use std::env;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct VmStore {
    pub(crate) root: PathBuf,
}

impl Default for VmStore {
    fn default() -> Self {
        let root = env::var_os("BRIDGEVM_HOME")
            .map(PathBuf::from)
            .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".bridgevm")))
            .unwrap_or_else(|| PathBuf::from(".bridgevm"));
        Self::new(root)
    }
}

impl VmStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: absolutize(root.into()),
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn vms_dir(&self) -> PathBuf {
        self.root.join("vms")
    }

    pub fn bundle_path(&self, name: &str) -> PathBuf {
        self.vms_dir().join(format!("{}.vmbridge", slug(name)))
    }

    pub fn ensure(&self) -> Result<(), StorageError> {
        fs::create_dir_all(self.vms_dir())?;
        Ok(())
    }
}
