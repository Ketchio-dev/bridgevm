//! Split out of mmio.rs by responsibility.

use super::super::*;
use crate::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HvfMmioReadEmulationProbe {
    pub allowed: bool,
    pub attempted: bool,
    pub vm_created: bool,
    pub memory_allocated: bool,
    pub memory_mapped: bool,
    pub vcpu_created: bool,
    pub pc_set: bool,
    pub cpsr_set: bool,
    pub address_register_set: bool,
    pub first_run_attempted: bool,
    pub mmio_exit_observed: bool,
    pub pc_read_after_mmio_exit: bool,
    pub emulated_value_injected: bool,
    pub pc_advanced: bool,
    pub second_run_attempted: bool,
    pub continuation_exit_observed: bool,
    pub emulated_value_preserved: bool,
    pub watchdog_cancel_fired: bool,
    pub vcpu_destroyed: bool,
    pub memory_unmapped: bool,
    pub vm_destroyed: bool,
    pub memory_deallocated: bool,
    pub host: HvfHostCapabilities,
    pub code_ipa_start: u64,
    pub mmio_ipa: u64,
    pub bytes: usize,
    pub instructions: &'static str,
    pub emulated_value: u64,
    pub vm_create_status: Option<i32>,
    pub allocate_status: Option<i32>,
    pub map_status: Option<i32>,
    pub vcpu_create_status: Option<i32>,
    pub pc_set_status: Option<i32>,
    pub cpsr_set_status: Option<i32>,
    pub address_register_set_status: Option<i32>,
    pub first_run_status: Option<i32>,
    pub mmio_exit_reason: Option<u32>,
    pub mmio_exit_syndrome: Option<u64>,
    pub mmio_exit_virtual_address: Option<u64>,
    pub mmio_exit_physical_address: Option<u64>,
    pub first_watchdog_cancel_status: Option<i32>,
    pub pc_read_status: Option<i32>,
    pub pc_after_mmio_exit: Option<u64>,
    pub emulated_value_set_status: Option<i32>,
    pub pc_advance_status: Option<i32>,
    pub second_run_status: Option<i32>,
    pub continuation_exit_reason: Option<u32>,
    pub continuation_exit_syndrome: Option<u64>,
    pub continuation_exit_virtual_address: Option<u64>,
    pub continuation_exit_physical_address: Option<u64>,
    pub second_watchdog_cancel_status: Option<i32>,
    pub emulated_value_read_status: Option<i32>,
    pub emulated_value_after_continue: Option<u64>,
    pub vcpu_destroy_status: Option<i32>,
    pub unmap_status: Option<i32>,
    pub vm_destroy_status: Option<i32>,
    pub deallocate_status: Option<i32>,
    pub blockers: Vec<String>,
}

impl HvfMmioReadEmulationProbe {
    pub fn render_text(&self) -> String {
        let mut output = String::new();
        output.push_str("HVF MMIO read emulation probe\n");
        output.push_str("QEMU: not used\n");
        output.push_str("Apple VZ: not used\n");
        output.push_str("Guest execution: unmapped LDR, injected read value, then HVC\n");
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
        output.push_str(&format!(
            "Address register set: {}\n",
            self.address_register_set
        ));
        output.push_str(&format!(
            "First run attempted: {}\n",
            self.first_run_attempted
        ));
        output.push_str(&format!(
            "MMIO exit observed: {}\n",
            self.mmio_exit_observed
        ));
        output.push_str(&format!(
            "PC read after MMIO exit: {}\n",
            self.pc_read_after_mmio_exit
        ));
        output.push_str(&format!(
            "Emulated value injected: {}\n",
            self.emulated_value_injected
        ));
        output.push_str(&format!("PC advanced: {}\n", self.pc_advanced));
        output.push_str(&format!(
            "Second run attempted: {}\n",
            self.second_run_attempted
        ));
        output.push_str(&format!(
            "Continuation exit observed: {}\n",
            self.continuation_exit_observed
        ));
        output.push_str(&format!(
            "Emulated value preserved: {}\n",
            self.emulated_value_preserved
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
        output.push_str(&format!("Code IPA start: {:#x}\n", self.code_ipa_start));
        output.push_str(&format!("MMIO IPA: {:#x}\n", self.mmio_ipa));
        output.push_str(&format!("Bytes: {}\n", self.bytes));
        output.push_str(&format!("Instructions: {}\n", self.instructions));
        output.push_str(&format!("Emulated value: {:#x}\n", self.emulated_value));
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
            "Address register set status name: {}\n",
            render_optional_status_name(self.address_register_set_status)
        ));
        output.push_str(&format!(
            "First run status name: {}\n",
            render_optional_status_name(self.first_run_status)
        ));
        output.push_str(&format!(
            "MMIO exit reason name: {}\n",
            render_optional_exit_reason_name(self.mmio_exit_reason)
        ));
        output.push_str(&format!(
            "MMIO exit syndrome: {}\n",
            render_optional_u64(self.mmio_exit_syndrome)
        ));
        output.push_str(&format!(
            "MMIO exit virtual address: {}\n",
            render_optional_u64(self.mmio_exit_virtual_address)
        ));
        output.push_str(&format!(
            "MMIO exit physical address: {}\n",
            render_optional_u64(self.mmio_exit_physical_address)
        ));
        output.push_str(&format!(
            "First watchdog cancel status name: {}\n",
            render_optional_status_name(self.first_watchdog_cancel_status)
        ));
        output.push_str(&format!(
            "PC read status name: {}\n",
            render_optional_status_name(self.pc_read_status)
        ));
        output.push_str(&format!(
            "PC after MMIO exit: {}\n",
            render_optional_u64(self.pc_after_mmio_exit)
        ));
        output.push_str(&format!(
            "Emulated value set status name: {}\n",
            render_optional_status_name(self.emulated_value_set_status)
        ));
        output.push_str(&format!(
            "PC advance status name: {}\n",
            render_optional_status_name(self.pc_advance_status)
        ));
        output.push_str(&format!(
            "Second run status name: {}\n",
            render_optional_status_name(self.second_run_status)
        ));
        output.push_str(&format!(
            "Continuation exit reason name: {}\n",
            render_optional_exit_reason_name(self.continuation_exit_reason)
        ));
        output.push_str(&format!(
            "Continuation exit syndrome: {}\n",
            render_optional_u64(self.continuation_exit_syndrome)
        ));
        output.push_str(&format!(
            "Continuation exit virtual address: {}\n",
            render_optional_u64(self.continuation_exit_virtual_address)
        ));
        output.push_str(&format!(
            "Continuation exit physical address: {}\n",
            render_optional_u64(self.continuation_exit_physical_address)
        ));
        output.push_str(&format!(
            "Second watchdog cancel status name: {}\n",
            render_optional_status_name(self.second_watchdog_cancel_status)
        ));
        output.push_str(&format!(
            "Emulated value read status name: {}\n",
            render_optional_status_name(self.emulated_value_read_status)
        ));
        output.push_str(&format!(
            "Emulated value after continue: {}\n",
            render_optional_u64(self.emulated_value_after_continue)
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

pub fn probe_hvf_mmio_read_emulation(allow_emulate: bool) -> HvfMmioReadEmulationProbe {
    let host = query_hvf_host_capabilities();
    platform::probe_hvf_mmio_read_emulation(allow_emulate, host)
}
