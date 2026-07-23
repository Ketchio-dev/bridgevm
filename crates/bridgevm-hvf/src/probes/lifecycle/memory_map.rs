//! Split out of lifecycle.rs by responsibility.

use super::super::*;
use crate::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HvfMemoryMapProbe {
    pub allowed: bool,
    pub attempted: bool,
    pub vm_created: bool,
    pub memory_allocated: bool,
    pub memory_mapped: bool,
    pub memory_unmapped: bool,
    pub memory_deallocated: bool,
    pub vm_destroyed: bool,
    pub host: HvfHostCapabilities,
    pub ipa_start: u64,
    pub bytes: usize,
    pub vm_create_status: Option<i32>,
    pub allocate_status: Option<i32>,
    pub map_status: Option<i32>,
    pub unmap_status: Option<i32>,
    pub deallocate_status: Option<i32>,
    pub vm_destroy_status: Option<i32>,
    pub blockers: Vec<String>,
}

impl HvfMemoryMapProbe {
    pub fn render_text(&self) -> String {
        let mut output = String::new();
        output.push_str("HVF memory map/unmap probe\n");
        output.push_str("QEMU: not used\n");
        output.push_str("Apple VZ: not used\n");
        output.push_str("Guest execution: not entered\n");
        output.push_str(&format!("Host: {}\n", self.host.host));
        output.push_str(&format!("Host HVF available: {}\n", self.host.available));
        output.push_str(&format!("Allowed: {}\n", self.allowed));
        output.push_str(&format!("Attempted: {}\n", self.attempted));
        output.push_str(&format!("VM created: {}\n", self.vm_created));
        output.push_str(&format!("Memory allocated: {}\n", self.memory_allocated));
        output.push_str(&format!("Memory mapped: {}\n", self.memory_mapped));
        output.push_str(&format!("Memory unmapped: {}\n", self.memory_unmapped));
        output.push_str(&format!(
            "Memory deallocated: {}\n",
            self.memory_deallocated
        ));
        output.push_str(&format!("VM destroyed: {}\n", self.vm_destroyed));
        output.push_str(&format!("Guest IPA start: {:#x}\n", self.ipa_start));
        output.push_str(&format!("Bytes: {}\n", self.bytes));
        output.push_str(&format!(
            "VM create status: {}\n",
            render_optional_status(self.vm_create_status)
        ));
        output.push_str(&format!(
            "VM create status name: {}\n",
            render_optional_status_name(self.vm_create_status)
        ));
        output.push_str(&format!(
            "Allocate status: {}\n",
            render_optional_status(self.allocate_status)
        ));
        output.push_str(&format!(
            "Allocate status name: {}\n",
            render_optional_status_name(self.allocate_status)
        ));
        output.push_str(&format!(
            "Map status: {}\n",
            render_optional_status(self.map_status)
        ));
        output.push_str(&format!(
            "Map status name: {}\n",
            render_optional_status_name(self.map_status)
        ));
        output.push_str(&format!(
            "Unmap status: {}\n",
            render_optional_status(self.unmap_status)
        ));
        output.push_str(&format!(
            "Unmap status name: {}\n",
            render_optional_status_name(self.unmap_status)
        ));
        output.push_str(&format!(
            "Deallocate status: {}\n",
            render_optional_status(self.deallocate_status)
        ));
        output.push_str(&format!(
            "Deallocate status name: {}\n",
            render_optional_status_name(self.deallocate_status)
        ));
        output.push_str(&format!(
            "VM destroy status: {}\n",
            render_optional_status(self.vm_destroy_status)
        ));
        output.push_str(&format!(
            "VM destroy status name: {}\n",
            render_optional_status_name(self.vm_destroy_status)
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

pub fn probe_hvf_memory_map(allow_map: bool) -> HvfMemoryMapProbe {
    let host = query_hvf_host_capabilities();
    platform::probe_hvf_memory_map(allow_map, host)
}
