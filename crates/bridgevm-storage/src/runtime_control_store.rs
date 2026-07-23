//! Runtime resource policy and QMP supervisor metadata persistence.

use crate::*;

impl VmStore {
    pub fn runtime_resource_policy_metadata(
        &self,
        vm_name: &str,
    ) -> Result<Option<RuntimeResourcePolicyMetadata>, StorageError> {
        let (bundle, _) = self.get_vm(vm_name)?;
        let path = runtime_resource_policy_path(&bundle);
        if !path.exists() {
            return Ok(None);
        }
        Ok(Some(read_json_required(&path)?))
    }

    pub fn write_runtime_resource_policy_metadata(
        &self,
        vm_name: &str,
        metadata: &RuntimeResourcePolicyMetadata,
    ) -> Result<(), StorageError> {
        let (bundle, _) = self.get_vm(vm_name)?;
        write_json_pretty_atomic(&runtime_resource_policy_path(&bundle), metadata)
    }

    pub fn qmp_supervisor_metadata(
        &self,
        vm_name: &str,
    ) -> Result<Option<QmpSupervisorMetadata>, StorageError> {
        let (bundle, _) = self.get_vm(vm_name)?;
        let path = qmp_supervisor_path(&bundle);
        if !path.exists() {
            return Ok(None);
        }
        Ok(Some(read_json_required(&path)?))
    }

    pub fn write_qmp_supervisor_metadata(
        &self,
        vm_name: &str,
        metadata: &QmpSupervisorMetadata,
    ) -> Result<(), StorageError> {
        let (bundle, _) = self.get_vm(vm_name)?;
        write_json_pretty_atomic(&qmp_supervisor_path(&bundle), metadata)
    }
}
