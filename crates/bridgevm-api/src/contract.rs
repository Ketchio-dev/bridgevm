//! Split out of lib.rs by responsibility.

use crate::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CurrentRuntimeEngine {
    AppleVz,
    QemuCompatibility,
}

impl CurrentRuntimeEngine {
    pub fn for_mode(mode: VmMode) -> Self {
        match current_engine_descriptor_for_mode(mode).lane {
            EngineLane::AppleVz => Self::AppleVz,
            EngineLane::QemuCompatibility => Self::QemuCompatibility,
            EngineLane::BridgeHvf => {
                unreachable!("BridgeVM HVF is a target engine, not a current VmMode runtime")
            }
        }
    }

    pub fn for_manifest(manifest: &VmManifest) -> Self {
        Self::for_mode(manifest.mode)
    }

    pub fn network_backend(self) -> NetworkBackend {
        match self {
            Self::AppleVz => NetworkBackend::AppleVz,
            Self::QemuCompatibility => NetworkBackend::Qemu,
        }
    }

    pub fn runner_metadata_engine(self) -> &'static str {
        match self {
            Self::AppleVz => "lightvm",
            Self::QemuCompatibility => "fullvm",
        }
    }

    pub fn uses_qmp(self) -> bool {
        matches!(self, Self::QemuCompatibility)
    }

    pub fn lifecycle_backend_label(self) -> &'static str {
        match self {
            Self::AppleVz => "apple-vz",
            Self::QemuCompatibility => "qemu-qmp",
        }
    }
}

pub(crate) fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct BridgeVmServiceContract {
    pub schema_id: &'static str,
    pub version: u16,
    pub service: &'static str,
    pub request_type: &'static str,
    pub response_type: &'static str,
    pub transport: &'static str,
}

impl BridgeVmServiceContract {
    pub const fn json_over_uds() -> Self {
        Self {
            schema_id: BRIDGEVM_API_SCHEMA_ID,
            version: BRIDGEVM_API_CONTRACT_VERSION,
            service: BRIDGEVM_API_SERVICE_NAME,
            request_type: "BridgeVmRequest",
            response_type: "BridgeVmResponse",
            transport: BRIDGEVM_API_JSON_OVER_UDS_TRANSPORT,
        }
    }

    pub const fn grpc_over_uds() -> Self {
        Self {
            transport: BRIDGEVM_API_GRPC_OVER_UDS_TRANSPORT,
            ..Self::json_over_uds()
        }
    }

    pub fn is_same_contract_as(&self, other: &Self) -> bool {
        self.schema_id == other.schema_id
            && self.version == other.version
            && self.service == other.service
            && self.request_type == other.request_type
            && self.response_type == other.response_type
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateVmRequest {
    pub manifest: VmManifest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmSummary {
    pub name: String,
    pub mode: String,
    pub guest: String,
    pub arch: String,
    pub state: String,
}

pub trait VmService {
    type Error;

    fn list_vms(&self) -> Result<Vec<VmSummary>, Self::Error>;
    fn create_vm(&self, request: CreateVmRequest) -> Result<VmSummary, Self::Error>;
    fn start_vm(&self, name: &str) -> Result<VmSummary, Self::Error>;
    fn suspend_vm(&self, name: &str) -> Result<VmSummary, Self::Error>;
    fn resume_vm(&self, name: &str) -> Result<VmSummary, Self::Error>;
    fn stop_vm(&self, name: &str) -> Result<VmSummary, Self::Error>;
}

pub trait ModeService {
    fn recommend_mode(&self, choice: GuestChoice) -> ModeRecommendation;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_runtime_engine_preserves_mode_to_engine_boundary() {
        let fast = CurrentRuntimeEngine::for_mode(VmMode::Fast);
        assert_eq!(fast, CurrentRuntimeEngine::AppleVz);
        assert_eq!(fast.network_backend(), NetworkBackend::AppleVz);
        assert_eq!(fast.runner_metadata_engine(), "lightvm");
        assert!(!fast.uses_qmp());
        assert_eq!(fast.lifecycle_backend_label(), "apple-vz");

        let compatibility = CurrentRuntimeEngine::for_mode(VmMode::Compatibility);
        assert_eq!(compatibility, CurrentRuntimeEngine::QemuCompatibility);
        assert_eq!(compatibility.network_backend(), NetworkBackend::Qemu);
        assert_eq!(compatibility.runner_metadata_engine(), "fullvm");
        assert!(compatibility.uses_qmp());
        assert_eq!(compatibility.lifecycle_backend_label(), "qemu-qmp");
    }
}
