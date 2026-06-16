use bridgevm_core::{VmEngine, VmState};

#[derive(Debug, Default)]
pub struct LightVmEngine;

impl VmEngine for LightVmEngine {
    fn name(&self) -> &'static str {
        "lightvm"
    }

    fn start(&self, _vm_name: &str) -> Result<VmState, String> {
        Ok(VmState::Running)
    }

    fn stop(&self, _vm_name: &str) -> Result<VmState, String> {
        Ok(VmState::Stopped)
    }

    fn suspend(&self, _vm_name: &str) -> Result<VmState, String> {
        Ok(VmState::Suspended)
    }

    fn resume(&self, _vm_name: &str) -> Result<VmState, String> {
        Ok(VmState::Running)
    }
}
