//! Engine descriptor for the lightweight-VM lane.
//!
//! A thin marker type that lets the engine model name this lane; the behaviour it
//! stands for lives in the engine crates themselves.

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
