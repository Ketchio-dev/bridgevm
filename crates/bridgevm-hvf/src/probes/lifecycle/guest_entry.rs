//! Split out of lifecycle.rs by responsibility.

use super::super::*;
use crate::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HvfGuestEntryProbe {
    pub allowed: bool,
    pub attempted: bool,
    pub vm_created: bool,
    pub memory_allocated: bool,
    pub memory_mapped: bool,
    pub vcpu_created: bool,
    pub pc_set: bool,
    pub cpsr_set: bool,
    pub run_attempted: bool,
    pub entry_boundary_observed: bool,
    pub watchdog_cancel_fired: bool,
    pub vcpu_destroyed: bool,
    pub memory_unmapped: bool,
    pub vm_destroyed: bool,
    pub memory_deallocated: bool,
    pub host: HvfHostCapabilities,
    pub ipa_start: u64,
    pub bytes: usize,
    pub instruction: &'static str,
    pub vm_create_status: Option<i32>,
    pub allocate_status: Option<i32>,
    pub map_status: Option<i32>,
    pub vcpu_create_status: Option<i32>,
    pub pc_set_status: Option<i32>,
    pub cpsr_set_status: Option<i32>,
    pub run_status: Option<i32>,
    pub exit_reason: Option<u32>,
    pub exit_syndrome: Option<u64>,
    pub exit_virtual_address: Option<u64>,
    pub exit_physical_address: Option<u64>,
    pub watchdog_cancel_status: Option<i32>,
    pub vcpu_destroy_status: Option<i32>,
    pub unmap_status: Option<i32>,
    pub vm_destroy_status: Option<i32>,
    pub deallocate_status: Option<i32>,
    pub blockers: Vec<String>,
}

impl HvfGuestEntryProbe {
    pub fn render_text(&self) -> String {
        let mut output = String::new();
        output.push_str("HVF guest entry probe\n");
        output.push_str("QEMU: not used\n");
        output.push_str("Apple VZ: not used\n");
        output.push_str("Guest execution: one HVC instruction with watchdog\n");
        output.push_str(&format!("Host: {}\n", self.host.host));
        output.push_str(&format!("Host HVF available: {}\n", self.host.available));
        output.push_str(&format!("Allowed: {}\n", self.allowed));
        output.push_str(&format!("Attempted: {}\n", self.attempted));
        output.push_str(&format!("VM created: {}\n", self.vm_created));
        output.push_str(&format!("Memory allocated: {}\n", self.memory_allocated));
        output.push_str(&format!("Memory mapped: {}\n", self.memory_mapped));
        output.push_str(&format!("vCPU created: {}\n", self.vcpu_created));
        output.push_str(&format!("PC set: {}\n", self.pc_set));
        output.push_str(&format!("CPSR set: {}\n", self.cpsr_set));
        output.push_str(&format!("Run attempted: {}\n", self.run_attempted));
        output.push_str(&format!(
            "Entry boundary observed: {}\n",
            self.entry_boundary_observed
        ));
        output.push_str(&format!(
            "Watchdog cancel fired: {}\n",
            self.watchdog_cancel_fired
        ));
        output.push_str(&format!("vCPU destroyed: {}\n", self.vcpu_destroyed));
        output.push_str(&format!("Memory unmapped: {}\n", self.memory_unmapped));
        output.push_str(&format!("VM destroyed: {}\n", self.vm_destroyed));
        output.push_str(&format!(
            "Memory deallocated: {}\n",
            self.memory_deallocated
        ));
        output.push_str(&format!("Guest IPA start: {:#x}\n", self.ipa_start));
        output.push_str(&format!("Bytes: {}\n", self.bytes));
        output.push_str(&format!("Instruction: {}\n", self.instruction));
        output.push_str(&format!(
            "VM create status name: {}\n",
            render_optional_status_name(self.vm_create_status)
        ));
        output.push_str(&format!(
            "Allocate status name: {}\n",
            render_optional_status_name(self.allocate_status)
        ));
        output.push_str(&format!(
            "Map status name: {}\n",
            render_optional_status_name(self.map_status)
        ));
        output.push_str(&format!(
            "vCPU create status name: {}\n",
            render_optional_status_name(self.vcpu_create_status)
        ));
        output.push_str(&format!(
            "PC set status name: {}\n",
            render_optional_status_name(self.pc_set_status)
        ));
        output.push_str(&format!(
            "CPSR set status name: {}\n",
            render_optional_status_name(self.cpsr_set_status)
        ));
        output.push_str(&format!(
            "Run status name: {}\n",
            render_optional_status_name(self.run_status)
        ));
        output.push_str(&format!(
            "Exit reason name: {}\n",
            render_optional_exit_reason_name(self.exit_reason)
        ));
        output.push_str(&format!(
            "Exit syndrome: {}\n",
            render_optional_u64(self.exit_syndrome)
        ));
        output.push_str(&format!(
            "Exit virtual address: {}\n",
            render_optional_u64(self.exit_virtual_address)
        ));
        output.push_str(&format!(
            "Exit physical address: {}\n",
            render_optional_u64(self.exit_physical_address)
        ));
        output.push_str(&format!(
            "Watchdog cancel status name: {}\n",
            render_optional_status_name(self.watchdog_cancel_status)
        ));
        output.push_str(&format!(
            "vCPU destroy status name: {}\n",
            render_optional_status_name(self.vcpu_destroy_status)
        ));
        output.push_str(&format!(
            "Unmap status name: {}\n",
            render_optional_status_name(self.unmap_status)
        ));
        output.push_str(&format!(
            "VM destroy status name: {}\n",
            render_optional_status_name(self.vm_destroy_status)
        ));
        output.push_str(&format!(
            "Deallocate status name: {}\n",
            render_optional_status_name(self.deallocate_status)
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

pub fn probe_hvf_guest_entry(allow_entry: bool) -> HvfGuestEntryProbe {
    let host = query_hvf_host_capabilities();
    platform::probe_hvf_guest_entry(allow_entry, host)
}
