//! runner.json read, write and clear.

use crate::*;
use std::fs;

impl VmStore {
    pub fn runner_metadata(&self, vm_name: &str) -> Result<Option<RunnerMetadata>, StorageError> {
        let (bundle, _) = self.get_vm(vm_name)?;
        let path = bundle.join("metadata").join("runner.json");
        if !path.exists() {
            return Ok(None);
        }
        Ok(Some(read_json_required(&path)?))
    }

    pub fn write_runner_metadata(
        &self,
        vm_name: &str,
        metadata: &RunnerMetadata,
    ) -> Result<(), StorageError> {
        let (bundle, _) = self.get_vm(vm_name)?;
        let dir = bundle.join("metadata");
        fs::create_dir_all(&dir)?;
        // Atomic (temp + rename): a torn write of runner.json would otherwise
        // leave invalid JSON that bricks every later lifecycle read.
        write_json_pretty_atomic(&dir.join("runner.json"), metadata)?;
        Ok(())
    }

    pub fn clear_runner_metadata(&self, vm_name: &str) -> Result<(), StorageError> {
        let (bundle, _) = self.get_vm(vm_name)?;
        let path = bundle.join("metadata").join("runner.json");
        if path.exists() {
            fs::remove_file(path)?;
        }
        Ok(())
    }
}
