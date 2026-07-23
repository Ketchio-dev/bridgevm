//! Split out of lifecycle.rs by responsibility.

use super::super::*;
use crate::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HvfVmCreateProbe {
    pub allowed: bool,
    pub attempted: bool,
    pub created: bool,
    pub destroyed: bool,
    pub host: HvfHostCapabilities,
    pub create_status: Option<i32>,
    pub destroy_status: Option<i32>,
    pub blockers: Vec<String>,
}

impl HvfVmCreateProbe {
    pub fn render_text(&self) -> String {
        let mut output = String::new();
        output.push_str("HVF VM create/destroy probe\n");
        output.push_str("QEMU: not used\n");
        output.push_str("Apple VZ: not used\n");
        output.push_str(&format!("Host: {}\n", self.host.host));
        output.push_str(&format!("Host HVF available: {}\n", self.host.available));
        output.push_str(&format!("Allowed: {}\n", self.allowed));
        output.push_str(&format!("Attempted: {}\n", self.attempted));
        output.push_str(&format!("Created: {}\n", self.created));
        output.push_str(&format!("Destroyed: {}\n", self.destroyed));
        output.push_str(&format!(
            "Create status: {}\n",
            render_optional_status(self.create_status)
        ));
        output.push_str(&format!(
            "Create status name: {}\n",
            render_optional_status_name(self.create_status)
        ));
        output.push_str(&format!(
            "Destroy status: {}\n",
            render_optional_status(self.destroy_status)
        ));
        output.push_str(&format!(
            "Destroy status name: {}\n",
            render_optional_status_name(self.destroy_status)
        ));
        if self.blockers.is_empty() {
            output.push_str("Blockers: none\n");
        } else {
            output.push_str("Blockers:\n");
            for blocker in &self.blockers {
                output.push_str(&format!("- {blocker}\n"));
            }
        }
        output
    }
}

pub fn probe_hvf_vm_create(allow_create: bool) -> HvfVmCreateProbe {
    let host = query_hvf_host_capabilities();
    platform::probe_hvf_vm_create(allow_create, host)
}
