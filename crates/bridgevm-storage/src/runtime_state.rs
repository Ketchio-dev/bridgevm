//! Read, validate-transition and force-write VM runtime state.

use crate::*;
use std::fs;
use std::path::Path;

pub(crate) fn validate_transition(
    from: VmRuntimeState,
    to: VmRuntimeState,
) -> Result<(), StorageError> {
    let valid = matches!(
        (from, to),
        (VmRuntimeState::Stopped, VmRuntimeState::Running)
            | (VmRuntimeState::Running, VmRuntimeState::Stopped)
            | (VmRuntimeState::Running, VmRuntimeState::Suspended)
            | (VmRuntimeState::Suspended, VmRuntimeState::Running)
            | (VmRuntimeState::Suspended, VmRuntimeState::Stopped)
    ) || from == to;

    if valid {
        Ok(())
    } else {
        Err(StorageError::InvalidStateTransition { from, to })
    }
}

impl VmStore {
    pub fn state(&self, name: &str) -> Result<VmRuntimeMetadata, StorageError> {
        let (bundle, _) = self.get_vm(name)?;
        self.state_at(&bundle)
    }

    pub fn transition_state(
        &self,
        name: &str,
        to: VmRuntimeState,
    ) -> Result<VmRuntimeMetadata, StorageError> {
        let (bundle, _) = self.get_vm(name)?;
        let current = self.state_at(&bundle)?;
        validate_transition(current.state, to)?;
        self.write_state_at(&bundle, to)
    }

    /// Write the runtime state UNCONDITIONALLY, bypassing the transition-validity
    /// check. For use only after an irreversible action has already made the new
    /// state the ground truth (e.g. the backend process has been killed, or a
    /// suspend snapshot committed): the recorded state must then reflect reality
    /// even if the prior state was unexpected — refusing the write here is what
    /// strands a dead backend recorded as `Running`.
    pub fn force_transition_state(
        &self,
        name: &str,
        to: VmRuntimeState,
    ) -> Result<VmRuntimeMetadata, StorageError> {
        let (bundle, _) = self.get_vm(name)?;
        self.write_state_at(&bundle, to)
    }

    pub(crate) fn state_at(&self, bundle: &Path) -> Result<VmRuntimeMetadata, StorageError> {
        let path = bundle.join("metadata").join("state.json");
        if !path.exists() {
            return self.write_state_at(bundle, VmRuntimeState::Stopped);
        }
        read_json_required(&path)
    }

    pub(crate) fn write_state_at(
        &self,
        bundle: &Path,
        state: VmRuntimeState,
    ) -> Result<VmRuntimeMetadata, StorageError> {
        let metadata = VmRuntimeMetadata {
            state,
            updated_at_unix: now_unix(),
        };
        let dir = bundle.join("metadata");
        fs::create_dir_all(&dir)?;
        // Atomic (temp + rename): a torn write of state.json would otherwise
        // leave invalid JSON that bricks lifecycle reads.
        write_json_pretty_atomic(&dir.join("state.json"), &metadata)?;
        Ok(metadata)
    }
}
