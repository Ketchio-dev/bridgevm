//! MMIO probes: read-exit, read/write emulation, the PL011 and PL031 device probes, and the virtio-mmio block register/queue probes.
//!
//! Moved verbatim out of the legacy probe monolith. Items keep the visibility
//! they had at the crate root and are re-exported there, so the public API is
//! unchanged. The live backends live in `crate::platform`.

use crate::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HvfMmioReadExitProbe {
    pub allowed: bool,
    pub attempted: bool,
    pub vm_created: bool,
    pub memory_allocated: bool,
    pub memory_mapped: bool,
    pub vcpu_created: bool,
    pub pc_set: bool,
    pub cpsr_set: bool,
    pub address_register_set: bool,
    pub run_attempted: bool,
    pub mmio_exit_observed: bool,
    pub watchdog_cancel_fired: bool,
    pub vcpu_destroyed: bool,
    pub memory_unmapped: bool,
    pub vm_destroyed: bool,
    pub memory_deallocated: bool,
    pub host: HvfHostCapabilities,
    pub code_ipa_start: u64,
    pub mmio_ipa: u64,
    pub bytes: usize,
    pub instruction: &'static str,
    pub vm_create_status: Option<i32>,
    pub allocate_status: Option<i32>,
    pub map_status: Option<i32>,
    pub vcpu_create_status: Option<i32>,
    pub pc_set_status: Option<i32>,
    pub cpsr_set_status: Option<i32>,
    pub address_register_set_status: Option<i32>,
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

impl HvfMmioReadExitProbe {
    pub fn render_text(&self) -> String {
        let mut output = String::new();
        output.push_str("HVF MMIO read exit probe\n");
        output.push_str("QEMU: not used\n");
        output.push_str("Apple VZ: not used\n");
        output.push_str("Guest execution: one unmapped LDR read with watchdog\n");
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
        output.push_str(&format!("Run attempted: {}\n", self.run_attempted));
        output.push_str(&format!(
            "MMIO exit observed: {}\n",
            self.mmio_exit_observed
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
            "Address register set status name: {}\n",
            render_optional_status_name(self.address_register_set_status)
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

pub fn probe_hvf_mmio_read_exit(allow_mmio: bool) -> HvfMmioReadExitProbe {
    let host = query_hvf_host_capabilities();
    platform::probe_hvf_mmio_read_exit(allow_mmio, host)
}

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HvfMmioWriteEmulationProbe {
    pub allowed: bool,
    pub attempted: bool,
    pub vm_created: bool,
    pub memory_allocated: bool,
    pub memory_mapped: bool,
    pub vcpu_created: bool,
    pub pc_set: bool,
    pub cpsr_set: bool,
    pub write_value_register_set: bool,
    pub address_register_set: bool,
    pub first_run_attempted: bool,
    pub mmio_exit_observed: bool,
    pub pc_read_after_mmio_exit: bool,
    pub write_value_captured: bool,
    pub pc_advanced: bool,
    pub second_run_attempted: bool,
    pub continuation_exit_observed: bool,
    pub write_value_preserved: bool,
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
    pub write_value: u64,
    pub vm_create_status: Option<i32>,
    pub allocate_status: Option<i32>,
    pub map_status: Option<i32>,
    pub vcpu_create_status: Option<i32>,
    pub pc_set_status: Option<i32>,
    pub cpsr_set_status: Option<i32>,
    pub write_value_register_set_status: Option<i32>,
    pub address_register_set_status: Option<i32>,
    pub first_run_status: Option<i32>,
    pub mmio_exit_reason: Option<u32>,
    pub mmio_exit_syndrome: Option<u64>,
    pub mmio_exit_virtual_address: Option<u64>,
    pub mmio_exit_physical_address: Option<u64>,
    pub first_watchdog_cancel_status: Option<i32>,
    pub pc_read_status: Option<i32>,
    pub pc_after_mmio_exit: Option<u64>,
    pub write_value_capture_status: Option<i32>,
    pub captured_write_value: Option<u64>,
    pub pc_advance_status: Option<i32>,
    pub second_run_status: Option<i32>,
    pub continuation_exit_reason: Option<u32>,
    pub continuation_exit_syndrome: Option<u64>,
    pub continuation_exit_virtual_address: Option<u64>,
    pub continuation_exit_physical_address: Option<u64>,
    pub second_watchdog_cancel_status: Option<i32>,
    pub write_value_after_continue_status: Option<i32>,
    pub write_value_after_continue: Option<u64>,
    pub vcpu_destroy_status: Option<i32>,
    pub unmap_status: Option<i32>,
    pub vm_destroy_status: Option<i32>,
    pub deallocate_status: Option<i32>,
    pub blockers: Vec<String>,
}

impl HvfMmioWriteEmulationProbe {
    pub fn render_text(&self) -> String {
        let mut output = String::new();
        output.push_str("HVF MMIO write emulation probe\n");
        output.push_str("QEMU: not used\n");
        output.push_str("Apple VZ: not used\n");
        output.push_str("Guest execution: unmapped STR, captured write value, then HVC\n");
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
            "Write value register set: {}\n",
            self.write_value_register_set
        ));
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
            "Write value captured: {}\n",
            self.write_value_captured
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
            "Write value preserved: {}\n",
            self.write_value_preserved
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
        output.push_str(&format!("Write value: {:#x}\n", self.write_value));
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
            "Write value register set status name: {}\n",
            render_optional_status_name(self.write_value_register_set_status)
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
            "Write value capture status name: {}\n",
            render_optional_status_name(self.write_value_capture_status)
        ));
        output.push_str(&format!(
            "Captured write value: {}\n",
            render_optional_u64(self.captured_write_value)
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
            "Write value after continue status name: {}\n",
            render_optional_status_name(self.write_value_after_continue_status)
        ));
        output.push_str(&format!(
            "Write value after continue: {}\n",
            render_optional_u64(self.write_value_after_continue)
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

pub fn probe_hvf_mmio_write_emulation(allow_emulate: bool) -> HvfMmioWriteEmulationProbe {
    let host = query_hvf_host_capabilities();
    platform::probe_hvf_mmio_write_emulation(allow_emulate, host)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HvfMmioSerialDeviceProbe {
    pub allowed: bool,
    pub attempted: bool,
    pub vm_created: bool,
    pub memory_allocated: bool,
    pub memory_mapped: bool,
    pub vcpu_created: bool,
    pub pc_set: bool,
    pub cpsr_set: bool,
    pub write_value_register_set: bool,
    pub data_address_register_set: bool,
    pub status_address_register_set: bool,
    pub device_bus_created: bool,
    pub device_bus_device_count: usize,
    pub write_run_attempted: bool,
    pub write_exit_observed: bool,
    pub write_handled_by_device: bool,
    pub write_value_captured: bool,
    pub pc_advanced_after_write: bool,
    pub status_run_attempted: bool,
    pub status_exit_observed: bool,
    pub status_handled_by_device: bool,
    pub status_value_injected: bool,
    pub pc_advanced_after_status: bool,
    pub continuation_run_attempted: bool,
    pub continuation_exit_observed: bool,
    pub status_value_preserved: bool,
    pub watchdog_cancel_fired: bool,
    pub vcpu_destroyed: bool,
    pub memory_unmapped: bool,
    pub vm_destroyed: bool,
    pub memory_deallocated: bool,
    pub host: HvfHostCapabilities,
    pub device_model: &'static str,
    pub code_ipa_start: u64,
    pub data_ipa: u64,
    pub status_ipa: u64,
    pub bytes: usize,
    pub instructions: &'static str,
    pub serial_write_value: u64,
    pub serial_status_value: u64,
    pub captured_write_value: Option<u64>,
    pub captured_byte: Option<u8>,
    pub vm_create_status: Option<i32>,
    pub allocate_status: Option<i32>,
    pub map_status: Option<i32>,
    pub vcpu_create_status: Option<i32>,
    pub pc_set_status: Option<i32>,
    pub cpsr_set_status: Option<i32>,
    pub write_value_register_set_status: Option<i32>,
    pub data_address_register_set_status: Option<i32>,
    pub status_address_register_set_status: Option<i32>,
    pub write_run_status: Option<i32>,
    pub write_exit_reason: Option<u32>,
    pub write_exit_syndrome: Option<u64>,
    pub write_exit_virtual_address: Option<u64>,
    pub write_exit_physical_address: Option<u64>,
    pub write_watchdog_cancel_status: Option<i32>,
    pub write_value_capture_status: Option<i32>,
    pub pc_read_after_write_status: Option<i32>,
    pub pc_after_write_exit: Option<u64>,
    pub pc_advance_after_write_status: Option<i32>,
    pub status_run_status: Option<i32>,
    pub status_exit_reason: Option<u32>,
    pub status_exit_syndrome: Option<u64>,
    pub status_exit_virtual_address: Option<u64>,
    pub status_exit_physical_address: Option<u64>,
    pub status_watchdog_cancel_status: Option<i32>,
    pub status_value_set_status: Option<i32>,
    pub pc_read_after_status_status: Option<i32>,
    pub pc_after_status_exit: Option<u64>,
    pub pc_advance_after_status_status: Option<i32>,
    pub continuation_run_status: Option<i32>,
    pub continuation_exit_reason: Option<u32>,
    pub continuation_exit_syndrome: Option<u64>,
    pub continuation_exit_virtual_address: Option<u64>,
    pub continuation_exit_physical_address: Option<u64>,
    pub continuation_watchdog_cancel_status: Option<i32>,
    pub status_value_after_continue_status: Option<i32>,
    pub status_value_after_continue: Option<u64>,
    pub vcpu_destroy_status: Option<i32>,
    pub unmap_status: Option<i32>,
    pub vm_destroy_status: Option<i32>,
    pub deallocate_status: Option<i32>,
    pub blockers: Vec<String>,
}

impl HvfMmioSerialDeviceProbe {
    pub fn render_text(&self) -> String {
        let mut output = String::new();
        output.push_str("HVF MMIO serial device probe\n");
        output.push_str("QEMU: not used\n");
        output.push_str("Apple VZ: not used\n");
        output.push_str("Guest execution: STR data register, LDR status register, then HVC\n");
        output.push_str(&format!("Device model: {}\n", self.device_model));
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
            "Write value register set: {}\n",
            self.write_value_register_set
        ));
        output.push_str(&format!(
            "Data address register set: {}\n",
            self.data_address_register_set
        ));
        output.push_str(&format!(
            "Status address register set: {}\n",
            self.status_address_register_set
        ));
        output.push_str(&format!(
            "Device bus created: {}\n",
            self.device_bus_created
        ));
        output.push_str(&format!(
            "Device bus device count: {}\n",
            self.device_bus_device_count
        ));
        output.push_str(&format!(
            "Write run attempted: {}\n",
            self.write_run_attempted
        ));
        output.push_str(&format!(
            "Write exit observed: {}\n",
            self.write_exit_observed
        ));
        output.push_str(&format!(
            "Write handled by device: {}\n",
            self.write_handled_by_device
        ));
        output.push_str(&format!(
            "Write value captured: {}\n",
            self.write_value_captured
        ));
        output.push_str(&format!(
            "PC advanced after write: {}\n",
            self.pc_advanced_after_write
        ));
        output.push_str(&format!(
            "Status run attempted: {}\n",
            self.status_run_attempted
        ));
        output.push_str(&format!(
            "Status exit observed: {}\n",
            self.status_exit_observed
        ));
        output.push_str(&format!(
            "Status handled by device: {}\n",
            self.status_handled_by_device
        ));
        output.push_str(&format!(
            "Status value injected: {}\n",
            self.status_value_injected
        ));
        output.push_str(&format!(
            "PC advanced after status: {}\n",
            self.pc_advanced_after_status
        ));
        output.push_str(&format!(
            "Continuation run attempted: {}\n",
            self.continuation_run_attempted
        ));
        output.push_str(&format!(
            "Continuation exit observed: {}\n",
            self.continuation_exit_observed
        ));
        output.push_str(&format!(
            "Status value preserved: {}\n",
            self.status_value_preserved
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
        output.push_str(&format!("Serial data IPA: {:#x}\n", self.data_ipa));
        output.push_str(&format!("Serial status IPA: {:#x}\n", self.status_ipa));
        output.push_str(&format!("Bytes: {}\n", self.bytes));
        output.push_str(&format!("Instructions: {}\n", self.instructions));
        output.push_str(&format!(
            "Serial write value: {:#x}\n",
            self.serial_write_value
        ));
        output.push_str(&format!(
            "Serial status value: {:#x}\n",
            self.serial_status_value
        ));
        output.push_str(&format!(
            "Captured write value: {}\n",
            render_optional_u64(self.captured_write_value)
        ));
        output.push_str(&format!(
            "Captured byte: {}\n",
            self.captured_byte
                .map_or_else(|| "not observed".to_string(), |value| format!("{value:#x}"))
        ));
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
            "Write value register set status name: {}\n",
            render_optional_status_name(self.write_value_register_set_status)
        ));
        output.push_str(&format!(
            "Data address register set status name: {}\n",
            render_optional_status_name(self.data_address_register_set_status)
        ));
        output.push_str(&format!(
            "Status address register set status name: {}\n",
            render_optional_status_name(self.status_address_register_set_status)
        ));
        output.push_str(&format!(
            "Write run status name: {}\n",
            render_optional_status_name(self.write_run_status)
        ));
        output.push_str(&format!(
            "Write exit reason name: {}\n",
            render_optional_exit_reason_name(self.write_exit_reason)
        ));
        output.push_str(&format!(
            "Write exit syndrome: {}\n",
            render_optional_u64(self.write_exit_syndrome)
        ));
        output.push_str(&format!(
            "Write exit virtual address: {}\n",
            render_optional_u64(self.write_exit_virtual_address)
        ));
        output.push_str(&format!(
            "Write exit physical address: {}\n",
            render_optional_u64(self.write_exit_physical_address)
        ));
        output.push_str(&format!(
            "Write watchdog cancel status name: {}\n",
            render_optional_status_name(self.write_watchdog_cancel_status)
        ));
        output.push_str(&format!(
            "Write value capture status name: {}\n",
            render_optional_status_name(self.write_value_capture_status)
        ));
        output.push_str(&format!(
            "PC read after write status name: {}\n",
            render_optional_status_name(self.pc_read_after_write_status)
        ));
        output.push_str(&format!(
            "PC after write exit: {}\n",
            render_optional_u64(self.pc_after_write_exit)
        ));
        output.push_str(&format!(
            "PC advance after write status name: {}\n",
            render_optional_status_name(self.pc_advance_after_write_status)
        ));
        output.push_str(&format!(
            "Status run status name: {}\n",
            render_optional_status_name(self.status_run_status)
        ));
        output.push_str(&format!(
            "Status exit reason name: {}\n",
            render_optional_exit_reason_name(self.status_exit_reason)
        ));
        output.push_str(&format!(
            "Status exit syndrome: {}\n",
            render_optional_u64(self.status_exit_syndrome)
        ));
        output.push_str(&format!(
            "Status exit virtual address: {}\n",
            render_optional_u64(self.status_exit_virtual_address)
        ));
        output.push_str(&format!(
            "Status exit physical address: {}\n",
            render_optional_u64(self.status_exit_physical_address)
        ));
        output.push_str(&format!(
            "Status watchdog cancel status name: {}\n",
            render_optional_status_name(self.status_watchdog_cancel_status)
        ));
        output.push_str(&format!(
            "Status value set status name: {}\n",
            render_optional_status_name(self.status_value_set_status)
        ));
        output.push_str(&format!(
            "PC read after status status name: {}\n",
            render_optional_status_name(self.pc_read_after_status_status)
        ));
        output.push_str(&format!(
            "PC after status exit: {}\n",
            render_optional_u64(self.pc_after_status_exit)
        ));
        output.push_str(&format!(
            "PC advance after status status name: {}\n",
            render_optional_status_name(self.pc_advance_after_status_status)
        ));
        output.push_str(&format!(
            "Continuation run status name: {}\n",
            render_optional_status_name(self.continuation_run_status)
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
            "Continuation watchdog cancel status name: {}\n",
            render_optional_status_name(self.continuation_watchdog_cancel_status)
        ));
        output.push_str(&format!(
            "Status value after continue status name: {}\n",
            render_optional_status_name(self.status_value_after_continue_status)
        ));
        output.push_str(&format!(
            "Status value after continue: {}\n",
            render_optional_u64(self.status_value_after_continue)
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

pub fn probe_hvf_mmio_serial_device(allow_device: bool) -> HvfMmioSerialDeviceProbe {
    let host = query_hvf_host_capabilities();
    platform::probe_hvf_mmio_serial_device(allow_device, host)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HvfMmioRtcDeviceProbe {
    pub allowed: bool,
    pub attempted: bool,
    pub vm_created: bool,
    pub memory_allocated: bool,
    pub memory_mapped: bool,
    pub vcpu_created: bool,
    pub pc_set: bool,
    pub cpsr_set: bool,
    pub rtc_address_register_set: bool,
    pub device_bus_created: bool,
    pub device_bus_device_count: usize,
    pub first_run_attempted: bool,
    pub rtc_exit_observed: bool,
    pub rtc_handled_by_device: bool,
    pub rtc_value_injected: bool,
    pub pc_read_after_rtc_exit: bool,
    pub pc_advanced: bool,
    pub second_run_attempted: bool,
    pub continuation_exit_observed: bool,
    pub rtc_value_preserved: bool,
    pub watchdog_cancel_fired: bool,
    pub vcpu_destroyed: bool,
    pub memory_unmapped: bool,
    pub vm_destroyed: bool,
    pub memory_deallocated: bool,
    pub host: HvfHostCapabilities,
    pub device_models: &'static str,
    pub code_ipa_start: u64,
    pub uart_ipa: u64,
    pub rtc_ipa: u64,
    pub bytes: usize,
    pub instructions: &'static str,
    pub rtc_value: u64,
    pub vm_create_status: Option<i32>,
    pub allocate_status: Option<i32>,
    pub map_status: Option<i32>,
    pub vcpu_create_status: Option<i32>,
    pub pc_set_status: Option<i32>,
    pub cpsr_set_status: Option<i32>,
    pub rtc_address_register_set_status: Option<i32>,
    pub first_run_status: Option<i32>,
    pub rtc_exit_reason: Option<u32>,
    pub rtc_exit_syndrome: Option<u64>,
    pub rtc_exit_virtual_address: Option<u64>,
    pub rtc_exit_physical_address: Option<u64>,
    pub first_watchdog_cancel_status: Option<i32>,
    pub rtc_value_set_status: Option<i32>,
    pub pc_read_status: Option<i32>,
    pub pc_after_rtc_exit: Option<u64>,
    pub pc_advance_status: Option<i32>,
    pub second_run_status: Option<i32>,
    pub continuation_exit_reason: Option<u32>,
    pub continuation_exit_syndrome: Option<u64>,
    pub continuation_exit_virtual_address: Option<u64>,
    pub continuation_exit_physical_address: Option<u64>,
    pub second_watchdog_cancel_status: Option<i32>,
    pub rtc_value_after_continue_status: Option<i32>,
    pub rtc_value_after_continue: Option<u64>,
    pub vcpu_destroy_status: Option<i32>,
    pub unmap_status: Option<i32>,
    pub vm_destroy_status: Option<i32>,
    pub deallocate_status: Option<i32>,
    pub blockers: Vec<String>,
}

impl HvfMmioRtcDeviceProbe {
    pub fn render_text(&self) -> String {
        let mut output = String::new();
        output.push_str("HVF MMIO RTC device probe\n");
        output.push_str("QEMU: not used\n");
        output.push_str("Apple VZ: not used\n");
        output.push_str("Guest execution: LDR RTC data register, then HVC\n");
        output.push_str(&format!("Device models: {}\n", self.device_models));
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
            "RTC address register set: {}\n",
            self.rtc_address_register_set
        ));
        output.push_str(&format!(
            "Device bus created: {}\n",
            self.device_bus_created
        ));
        output.push_str(&format!(
            "Device bus device count: {}\n",
            self.device_bus_device_count
        ));
        output.push_str(&format!(
            "First run attempted: {}\n",
            self.first_run_attempted
        ));
        output.push_str(&format!("RTC exit observed: {}\n", self.rtc_exit_observed));
        output.push_str(&format!(
            "RTC handled by device: {}\n",
            self.rtc_handled_by_device
        ));
        output.push_str(&format!(
            "RTC value injected: {}\n",
            self.rtc_value_injected
        ));
        output.push_str(&format!(
            "PC read after RTC exit: {}\n",
            self.pc_read_after_rtc_exit
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
            "RTC value preserved: {}\n",
            self.rtc_value_preserved
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
        output.push_str(&format!("UART IPA: {:#x}\n", self.uart_ipa));
        output.push_str(&format!("RTC IPA: {:#x}\n", self.rtc_ipa));
        output.push_str(&format!("Bytes: {}\n", self.bytes));
        output.push_str(&format!("Instructions: {}\n", self.instructions));
        output.push_str(&format!("RTC value: {:#x}\n", self.rtc_value));
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
            "RTC address register set status name: {}\n",
            render_optional_status_name(self.rtc_address_register_set_status)
        ));
        output.push_str(&format!(
            "First run status name: {}\n",
            render_optional_status_name(self.first_run_status)
        ));
        output.push_str(&format!(
            "RTC exit reason name: {}\n",
            render_optional_exit_reason_name(self.rtc_exit_reason)
        ));
        output.push_str(&format!(
            "RTC exit syndrome: {}\n",
            render_optional_u64(self.rtc_exit_syndrome)
        ));
        output.push_str(&format!(
            "RTC exit virtual address: {}\n",
            render_optional_u64(self.rtc_exit_virtual_address)
        ));
        output.push_str(&format!(
            "RTC exit physical address: {}\n",
            render_optional_u64(self.rtc_exit_physical_address)
        ));
        output.push_str(&format!(
            "First watchdog cancel status name: {}\n",
            render_optional_status_name(self.first_watchdog_cancel_status)
        ));
        output.push_str(&format!(
            "RTC value set status name: {}\n",
            render_optional_status_name(self.rtc_value_set_status)
        ));
        output.push_str(&format!(
            "PC read status name: {}\n",
            render_optional_status_name(self.pc_read_status)
        ));
        output.push_str(&format!(
            "PC after RTC exit: {}\n",
            render_optional_u64(self.pc_after_rtc_exit)
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
            "RTC value after continue status name: {}\n",
            render_optional_status_name(self.rtc_value_after_continue_status)
        ));
        output.push_str(&format!(
            "RTC value after continue: {}\n",
            render_optional_u64(self.rtc_value_after_continue)
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

pub fn probe_hvf_mmio_rtc_device(allow_device: bool) -> HvfMmioRtcDeviceProbe {
    let host = query_hvf_host_capabilities();
    platform::probe_hvf_mmio_rtc_device(allow_device, host)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HvfMmioBlockRegisterProbe {
    pub name: &'static str,
    pub ipa: u64,
    pub expected_value: u64,
    pub run_attempted: bool,
    pub exit_observed: bool,
    pub handled_by_device: bool,
    pub value_injected: bool,
    pub pc_read_after_exit: bool,
    pub pc_advanced: bool,
    pub run_status: Option<i32>,
    pub exit_reason: Option<u32>,
    pub exit_syndrome: Option<u64>,
    pub exit_virtual_address: Option<u64>,
    pub exit_physical_address: Option<u64>,
    pub watchdog_cancel_status: Option<i32>,
    pub value_set_status: Option<i32>,
    pub pc_read_status: Option<i32>,
    pub pc_after_exit: Option<u64>,
    pub pc_advance_status: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HvfMmioBlockDeviceProbe {
    pub allowed: bool,
    pub attempted: bool,
    pub vm_created: bool,
    pub memory_allocated: bool,
    pub memory_mapped: bool,
    pub vcpu_created: bool,
    pub pc_set: bool,
    pub cpsr_set: bool,
    pub register_address_registers_set: bool,
    pub device_bus_created: bool,
    pub device_bus_device_count: usize,
    pub register_reads: Vec<HvfMmioBlockRegisterProbe>,
    pub continuation_run_attempted: bool,
    pub continuation_exit_observed: bool,
    pub vendor_value_preserved: bool,
    pub watchdog_cancel_fired: bool,
    pub vcpu_destroyed: bool,
    pub memory_unmapped: bool,
    pub vm_destroyed: bool,
    pub memory_deallocated: bool,
    pub host: HvfHostCapabilities,
    pub device_models: &'static str,
    pub code_ipa_start: u64,
    pub block_ipa: u64,
    pub bytes: usize,
    pub instructions: &'static str,
    pub magic_value: u64,
    pub version_value: u64,
    pub device_id_value: u64,
    pub vendor_id_value: u64,
    pub vm_create_status: Option<i32>,
    pub allocate_status: Option<i32>,
    pub map_status: Option<i32>,
    pub vcpu_create_status: Option<i32>,
    pub pc_set_status: Option<i32>,
    pub cpsr_set_status: Option<i32>,
    pub register_address_registers_set_status: Vec<Option<i32>>,
    pub continuation_run_status: Option<i32>,
    pub continuation_exit_reason: Option<u32>,
    pub continuation_exit_syndrome: Option<u64>,
    pub continuation_exit_virtual_address: Option<u64>,
    pub continuation_exit_physical_address: Option<u64>,
    pub continuation_watchdog_cancel_status: Option<i32>,
    pub vendor_value_after_continue_status: Option<i32>,
    pub vendor_value_after_continue: Option<u64>,
    pub vcpu_destroy_status: Option<i32>,
    pub unmap_status: Option<i32>,
    pub vm_destroy_status: Option<i32>,
    pub deallocate_status: Option<i32>,
    pub blockers: Vec<String>,
}

impl HvfMmioBlockDeviceProbe {
    pub fn render_text(&self) -> String {
        let mut output = String::new();
        output.push_str("HVF MMIO block device probe\n");
        output.push_str("QEMU: not used\n");
        output.push_str("Apple VZ: not used\n");
        output.push_str("Guest execution: LDR W0 VirtIO-MMIO identity registers, then HVC\n");
        output.push_str(&format!("Device models: {}\n", self.device_models));
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
            "Register address registers set: {}\n",
            self.register_address_registers_set
        ));
        output.push_str(&format!(
            "Device bus created: {}\n",
            self.device_bus_created
        ));
        output.push_str(&format!(
            "Device bus device count: {}\n",
            self.device_bus_device_count
        ));
        output.push_str("VirtIO-MMIO block identity reads:\n");
        for read in &self.register_reads {
            output.push_str(&format!(
                "- {} at {:#x}: expected {:#x}, run={}, exit={}, handled={}, injected={}, pc_advanced={}\n",
                read.name,
                read.ipa,
                read.expected_value,
                read.run_attempted,
                read.exit_observed,
                read.handled_by_device,
                read.value_injected,
                read.pc_advanced
            ));
            output.push_str(&format!(
                "  {} run status name: {}\n",
                read.name,
                render_optional_status_name(read.run_status)
            ));
            output.push_str(&format!(
                "  {} exit reason name: {}\n",
                read.name,
                render_optional_exit_reason_name(read.exit_reason)
            ));
            output.push_str(&format!(
                "  {} exit syndrome: {}\n",
                read.name,
                render_optional_u64(read.exit_syndrome)
            ));
            output.push_str(&format!(
                "  {} exit virtual address: {}\n",
                read.name,
                render_optional_u64(read.exit_virtual_address)
            ));
            output.push_str(&format!(
                "  {} exit physical address: {}\n",
                read.name,
                render_optional_u64(read.exit_physical_address)
            ));
            output.push_str(&format!(
                "  {} watchdog cancel status name: {}\n",
                read.name,
                render_optional_status_name(read.watchdog_cancel_status)
            ));
            output.push_str(&format!(
                "  {} value set status name: {}\n",
                read.name,
                render_optional_status_name(read.value_set_status)
            ));
            output.push_str(&format!(
                "  {} PC read status name: {}\n",
                read.name,
                render_optional_status_name(read.pc_read_status)
            ));
            output.push_str(&format!(
                "  {} PC after exit: {}\n",
                read.name,
                render_optional_u64(read.pc_after_exit)
            ));
            output.push_str(&format!(
                "  {} PC advance status name: {}\n",
                read.name,
                render_optional_status_name(read.pc_advance_status)
            ));
        }
        output.push_str(&format!(
            "Continuation run attempted: {}\n",
            self.continuation_run_attempted
        ));
        output.push_str(&format!(
            "Continuation exit observed: {}\n",
            self.continuation_exit_observed
        ));
        output.push_str(&format!(
            "Vendor value preserved: {}\n",
            self.vendor_value_preserved
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
        output.push_str(&format!("Block IPA: {:#x}\n", self.block_ipa));
        output.push_str(&format!("Bytes: {}\n", self.bytes));
        output.push_str(&format!("Instructions: {}\n", self.instructions));
        output.push_str(&format!("VirtIO magic value: {:#x}\n", self.magic_value));
        output.push_str(&format!(
            "VirtIO version value: {:#x}\n",
            self.version_value
        ));
        output.push_str(&format!(
            "VirtIO block device ID value: {:#x}\n",
            self.device_id_value
        ));
        output.push_str(&format!(
            "VirtIO vendor ID value: {:#x}\n",
            self.vendor_id_value
        ));
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
        output.push_str("Register address set status names:\n");
        for (index, status) in self
            .register_address_registers_set_status
            .iter()
            .enumerate()
        {
            output.push_str(&format!(
                "- X{}: {}\n",
                index + 1,
                render_optional_status_name(*status)
            ));
        }
        output.push_str(&format!(
            "Continuation run status name: {}\n",
            render_optional_status_name(self.continuation_run_status)
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
            "Continuation watchdog cancel status name: {}\n",
            render_optional_status_name(self.continuation_watchdog_cancel_status)
        ));
        output.push_str(&format!(
            "Vendor value after continue status name: {}\n",
            render_optional_status_name(self.vendor_value_after_continue_status)
        ));
        output.push_str(&format!(
            "Vendor value after continue: {}\n",
            render_optional_u64(self.vendor_value_after_continue)
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

pub fn probe_hvf_mmio_block_device(allow_device: bool) -> HvfMmioBlockDeviceProbe {
    let host = query_hvf_host_capabilities();
    platform::probe_hvf_mmio_block_device(allow_device, host)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HvfMmioBlockQueueStepProbe {
    pub name: &'static str,
    pub access: &'static str,
    pub ipa: u64,
    pub expected_value: Option<u64>,
    pub write_value: Option<u64>,
    pub run_attempted: bool,
    pub address_register_set: bool,
    pub write_value_register_set: bool,
    pub exit_observed: bool,
    pub handled_by_device: bool,
    pub value_injected: bool,
    pub write_accepted: bool,
    pub pc_read_after_exit: bool,
    pub pc_advanced: bool,
    pub captured_write_value: Option<u64>,
    pub run_status: Option<i32>,
    pub address_register_set_status: Option<i32>,
    pub write_value_register_set_status: Option<i32>,
    pub exit_reason: Option<u32>,
    pub exit_syndrome: Option<u64>,
    pub exit_virtual_address: Option<u64>,
    pub exit_physical_address: Option<u64>,
    pub watchdog_cancel_status: Option<i32>,
    pub value_set_status: Option<i32>,
    pub pc_read_status: Option<i32>,
    pub pc_after_exit: Option<u64>,
    pub pc_advance_status: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HvfMmioBlockQueueProbe {
    pub allowed: bool,
    pub attempted: bool,
    pub vm_created: bool,
    pub memory_allocated: bool,
    pub memory_mapped: bool,
    pub vcpu_created: bool,
    pub pc_set: bool,
    pub cpsr_set: bool,
    pub device_bus_created: bool,
    pub device_bus_device_count: usize,
    pub steps: Vec<HvfMmioBlockQueueStepProbe>,
    pub continuation_run_attempted: bool,
    pub continuation_exit_observed: bool,
    pub capacity_high_value_preserved: bool,
    pub watchdog_cancel_fired: bool,
    pub vcpu_destroyed: bool,
    pub memory_unmapped: bool,
    pub vm_destroyed: bool,
    pub memory_deallocated: bool,
    pub host: HvfHostCapabilities,
    pub device_models: &'static str,
    pub code_ipa_start: u64,
    pub block_ipa: u64,
    pub bytes: usize,
    pub instructions: &'static str,
    pub device_features_value: u64,
    pub driver_features_value: u64,
    pub queue_select_value: u64,
    pub queue_num_max_value: u64,
    pub queue_num_value: u64,
    pub queue_ready_value: u64,
    pub queue_desc_address: u64,
    pub queue_driver_address: u64,
    pub queue_device_address: u64,
    pub queue_notify_value: u64,
    pub interrupt_status_value: u64,
    pub block_backing_kind: &'static str,
    pub block_backing_path: Option<PathBuf>,
    pub request_ring_seeded: bool,
    pub request_completed_after_notify: bool,
    pub request_descriptor_index: Option<u16>,
    pub request_sector: Option<u64>,
    pub request_byte_offset: Option<u64>,
    pub request_data_bytes: Option<u32>,
    pub request_data_prefix: Vec<u8>,
    pub request_status: Option<u8>,
    pub request_used_index: Option<u16>,
    pub request_used_len: Option<u32>,
    pub request_interrupt_status: Option<u64>,
    pub write_completed_after_notify: bool,
    pub write_request_type: Option<u32>,
    pub write_sector: Option<u64>,
    pub write_byte_offset: Option<u64>,
    pub write_data_bytes: Option<u32>,
    pub write_data_prefix: Vec<u8>,
    pub write_status: Option<u8>,
    pub write_used_index: Option<u16>,
    pub write_used_len: Option<u32>,
    pub flush_completed_after_notify: bool,
    pub flush_request_type: Option<u32>,
    pub flush_status: Option<u8>,
    pub flush_used_index: Option<u16>,
    pub flush_used_len: Option<u32>,
    pub persisted_data_prefix: Vec<u8>,
    pub status_value: u64,
    pub capacity_sectors: u64,
    pub vm_create_status: Option<i32>,
    pub allocate_status: Option<i32>,
    pub map_status: Option<i32>,
    pub vcpu_create_status: Option<i32>,
    pub pc_set_status: Option<i32>,
    pub cpsr_set_status: Option<i32>,
    pub continuation_run_status: Option<i32>,
    pub continuation_exit_reason: Option<u32>,
    pub continuation_exit_syndrome: Option<u64>,
    pub continuation_exit_virtual_address: Option<u64>,
    pub continuation_exit_physical_address: Option<u64>,
    pub continuation_watchdog_cancel_status: Option<i32>,
    pub capacity_high_after_continue_status: Option<i32>,
    pub capacity_high_after_continue: Option<u64>,
    pub vcpu_destroy_status: Option<i32>,
    pub unmap_status: Option<i32>,
    pub vm_destroy_status: Option<i32>,
    pub deallocate_status: Option<i32>,
    pub blockers: Vec<String>,
}

impl HvfMmioBlockQueueProbe {
    pub fn render_text(&self) -> String {
        let mut output = String::new();
        output.push_str("HVF MMIO block queue/config/address/notify probe\n");
        output.push_str("QEMU: not used\n");
        output.push_str("Apple VZ: not used\n");
        output.push_str(
            "Guest execution: VirtIO-MMIO feature, queue, ring address, notify, status, and capacity registers, then HVC\n",
        );
        output.push_str(&format!("Device models: {}\n", self.device_models));
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
            "Device bus created: {}\n",
            self.device_bus_created
        ));
        output.push_str(&format!(
            "Device bus device count: {}\n",
            self.device_bus_device_count
        ));
        output.push_str("VirtIO-MMIO block queue/config steps:\n");
        for step in &self.steps {
            output.push_str(&format!(
                "- {} {} at {:#x}: expected={}, write={}, run={}, address_set={}, write_value_set={}, exit={}, handled={}, injected={}, write_accepted={}, pc_advanced={}, captured={}\n",
                step.access,
                step.name,
                step.ipa,
                render_optional_u64(step.expected_value),
                render_optional_u64(step.write_value),
                step.run_attempted,
                step.address_register_set,
                step.write_value_register_set,
                step.exit_observed,
                step.handled_by_device,
                step.value_injected,
                step.write_accepted,
                step.pc_advanced,
                render_optional_u64(step.captured_write_value)
            ));
            output.push_str(&format!(
                "  {} run status name: {}\n",
                step.name,
                render_optional_status_name(step.run_status)
            ));
            output.push_str(&format!(
                "  {} address register set status name: {}\n",
                step.name,
                render_optional_status_name(step.address_register_set_status)
            ));
            output.push_str(&format!(
                "  {} write value register set status name: {}\n",
                step.name,
                render_optional_status_name(step.write_value_register_set_status)
            ));
            output.push_str(&format!(
                "  {} exit reason name: {}\n",
                step.name,
                render_optional_exit_reason_name(step.exit_reason)
            ));
            output.push_str(&format!(
                "  {} exit syndrome: {}\n",
                step.name,
                render_optional_u64(step.exit_syndrome)
            ));
            output.push_str(&format!(
                "  {} exit virtual address: {}\n",
                step.name,
                render_optional_u64(step.exit_virtual_address)
            ));
            output.push_str(&format!(
                "  {} exit physical address: {}\n",
                step.name,
                render_optional_u64(step.exit_physical_address)
            ));
            output.push_str(&format!(
                "  {} watchdog cancel status name: {}\n",
                step.name,
                render_optional_status_name(step.watchdog_cancel_status)
            ));
            output.push_str(&format!(
                "  {} value set status name: {}\n",
                step.name,
                render_optional_status_name(step.value_set_status)
            ));
            output.push_str(&format!(
                "  {} PC read status name: {}\n",
                step.name,
                render_optional_status_name(step.pc_read_status)
            ));
            output.push_str(&format!(
                "  {} PC after exit: {}\n",
                step.name,
                render_optional_u64(step.pc_after_exit)
            ));
            output.push_str(&format!(
                "  {} PC advance status name: {}\n",
                step.name,
                render_optional_status_name(step.pc_advance_status)
            ));
        }
        output.push_str(&format!(
            "Continuation run attempted: {}\n",
            self.continuation_run_attempted
        ));
        output.push_str(&format!(
            "Continuation exit observed: {}\n",
            self.continuation_exit_observed
        ));
        output.push_str(&format!(
            "Capacity high value preserved: {}\n",
            self.capacity_high_value_preserved
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
        output.push_str(&format!("Block IPA: {:#x}\n", self.block_ipa));
        output.push_str(&format!("Bytes: {}\n", self.bytes));
        output.push_str(&format!("Instructions: {}\n", self.instructions));
        output.push_str(&format!(
            "Device features value: {:#x}\n",
            self.device_features_value
        ));
        output.push_str(&format!(
            "Driver features value: {:#x}\n",
            self.driver_features_value
        ));
        output.push_str(&format!(
            "Queue select value: {:#x}\n",
            self.queue_select_value
        ));
        output.push_str(&format!(
            "Queue num max value: {:#x}\n",
            self.queue_num_max_value
        ));
        output.push_str(&format!("Queue num value: {:#x}\n", self.queue_num_value));
        output.push_str(&format!(
            "Queue ready value: {:#x}\n",
            self.queue_ready_value
        ));
        output.push_str(&format!(
            "Queue descriptor address: {:#x}\n",
            self.queue_desc_address
        ));
        output.push_str(&format!(
            "Queue driver address: {:#x}\n",
            self.queue_driver_address
        ));
        output.push_str(&format!(
            "Queue device address: {:#x}\n",
            self.queue_device_address
        ));
        output.push_str(&format!(
            "Queue notify value: {:#x}\n",
            self.queue_notify_value
        ));
        output.push_str(&format!(
            "Interrupt status value: {:#x}\n",
            self.interrupt_status_value
        ));
        output.push_str(&format!(
            "Block backing kind: {}\n",
            self.block_backing_kind
        ));
        output.push_str(&format!(
            "Block backing path: {}\n",
            self.block_backing_path.as_ref().map_or_else(
                || "not observed".to_string(),
                |path| path.display().to_string()
            )
        ));
        output.push_str(&format!(
            "Request ring seeded: {}\n",
            self.request_ring_seeded
        ));
        output.push_str(&format!(
            "Request completed after notify: {}\n",
            self.request_completed_after_notify
        ));
        output.push_str(&format!(
            "Request descriptor index: {}\n",
            render_optional_u64(self.request_descriptor_index.map(u64::from))
        ));
        output.push_str(&format!(
            "Request sector: {}\n",
            render_optional_u64(self.request_sector)
        ));
        output.push_str(&format!(
            "Request byte offset: {}\n",
            render_optional_u64(self.request_byte_offset)
        ));
        output.push_str(&format!(
            "Request data bytes: {}\n",
            render_optional_u64(self.request_data_bytes.map(u64::from))
        ));
        output.push_str(&format!(
            "Request data prefix: {}\n",
            render_hex_bytes(&self.request_data_prefix)
        ));
        output.push_str(&format!(
            "Request status byte: {}\n",
            render_optional_u64(self.request_status.map(u64::from))
        ));
        output.push_str(&format!(
            "Request used index: {}\n",
            render_optional_u64(self.request_used_index.map(u64::from))
        ));
        output.push_str(&format!(
            "Request used length: {}\n",
            render_optional_u64(self.request_used_len.map(u64::from))
        ));
        output.push_str(&format!(
            "Request interrupt status: {}\n",
            render_optional_u64(self.request_interrupt_status)
        ));
        output.push_str(&format!(
            "Write completed after notify: {}\n",
            self.write_completed_after_notify
        ));
        output.push_str(&format!(
            "Write request type: {}\n",
            render_optional_u64(self.write_request_type.map(u64::from))
        ));
        output.push_str(&format!(
            "Write sector: {}\n",
            render_optional_u64(self.write_sector)
        ));
        output.push_str(&format!(
            "Write byte offset: {}\n",
            render_optional_u64(self.write_byte_offset)
        ));
        output.push_str(&format!(
            "Write data bytes: {}\n",
            render_optional_u64(self.write_data_bytes.map(u64::from))
        ));
        output.push_str(&format!(
            "Write data prefix: {}\n",
            render_hex_bytes(&self.write_data_prefix)
        ));
        output.push_str(&format!(
            "Write status byte: {}\n",
            render_optional_u64(self.write_status.map(u64::from))
        ));
        output.push_str(&format!(
            "Write used index: {}\n",
            render_optional_u64(self.write_used_index.map(u64::from))
        ));
        output.push_str(&format!(
            "Write used length: {}\n",
            render_optional_u64(self.write_used_len.map(u64::from))
        ));
        output.push_str(&format!(
            "Flush completed after notify: {}\n",
            self.flush_completed_after_notify
        ));
        output.push_str(&format!(
            "Flush request type: {}\n",
            render_optional_u64(self.flush_request_type.map(u64::from))
        ));
        output.push_str(&format!(
            "Flush status byte: {}\n",
            render_optional_u64(self.flush_status.map(u64::from))
        ));
        output.push_str(&format!(
            "Flush used index: {}\n",
            render_optional_u64(self.flush_used_index.map(u64::from))
        ));
        output.push_str(&format!(
            "Flush used length: {}\n",
            render_optional_u64(self.flush_used_len.map(u64::from))
        ));
        output.push_str(&format!(
            "Persisted data prefix: {}\n",
            render_hex_bytes(&self.persisted_data_prefix)
        ));
        output.push_str(&format!("Status value: {:#x}\n", self.status_value));
        output.push_str(&format!("Capacity sectors: {:#x}\n", self.capacity_sectors));
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
            "Continuation run status name: {}\n",
            render_optional_status_name(self.continuation_run_status)
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
            "Continuation watchdog cancel status name: {}\n",
            render_optional_status_name(self.continuation_watchdog_cancel_status)
        ));
        output.push_str(&format!(
            "Capacity high after continue status name: {}\n",
            render_optional_status_name(self.capacity_high_after_continue_status)
        ));
        output.push_str(&format!(
            "Capacity high after continue: {}\n",
            render_optional_u64(self.capacity_high_after_continue)
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

pub fn probe_hvf_mmio_block_queue(
    allow_device: bool,
    disk_path: Option<PathBuf>,
    iso_path: Option<PathBuf>,
    writable_disk_path: Option<PathBuf>,
) -> HvfMmioBlockQueueProbe {
    let host = query_hvf_host_capabilities();
    platform::probe_hvf_mmio_block_queue(
        allow_device,
        disk_path,
        iso_path,
        writable_disk_path,
        host,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mmio_read_probe_is_opt_in_and_qemu_free() {
        let probe = probe_hvf_mmio_read_exit(false);
        let output = probe.render_text();

        assert!(!probe.allowed);
        assert!(!probe.attempted);
        assert!(!probe.vm_created);
        assert!(!probe.memory_mapped);
        assert!(!probe.vcpu_created);
        assert!(!probe.address_register_set);
        assert!(!probe.run_attempted);
        assert!(!probe.mmio_exit_observed);
        assert!(output.contains("HVF MMIO read exit probe"));
        assert!(output.contains("QEMU: not used"));
        assert!(output.contains("Apple VZ: not used"));
        assert!(output.contains("Guest execution: one unmapped LDR read with watchdog"));
        assert!(output.contains("Allowed: false"));
        assert!(output.contains("Attempted: false"));
        assert!(output.contains("MMIO IPA: 0x50000000"));
        assert!(output.contains("Instruction: LDR X0, [X1]"));
        assert!(output.contains("Run status name: not attempted"));
        assert!(!output.contains("qemu-system"));
        assert!(!output.contains('%'));
    }

    #[test]
    fn mmio_read_emulation_probe_is_opt_in_and_qemu_free() {
        let probe = probe_hvf_mmio_read_emulation(false);
        let output = probe.render_text();

        assert!(!probe.allowed);
        assert!(!probe.attempted);
        assert!(!probe.vm_created);
        assert!(!probe.memory_mapped);
        assert!(!probe.vcpu_created);
        assert!(!probe.first_run_attempted);
        assert!(!probe.mmio_exit_observed);
        assert!(!probe.emulated_value_injected);
        assert!(!probe.pc_advanced);
        assert!(!probe.second_run_attempted);
        assert!(!probe.continuation_exit_observed);
        assert!(!probe.emulated_value_preserved);
        assert!(output.contains("HVF MMIO read emulation probe"));
        assert!(output.contains("QEMU: not used"));
        assert!(output.contains("Apple VZ: not used"));
        assert!(output.contains("Guest execution: unmapped LDR, injected read value, then HVC"));
        assert!(output.contains("Allowed: false"));
        assert!(output.contains("Attempted: false"));
        assert!(output.contains("Instructions: LDR X0, [X1]; HVC #0"));
        assert!(output.contains("Emulated value: 0x123456789abcdef0"));
        assert!(output.contains("First run status name: not attempted"));
        assert!(output.contains("Second run status name: not attempted"));
        assert!(!output.contains("qemu-system"));
        assert!(!output.contains('%'));
    }

    #[test]
    fn mmio_write_emulation_probe_is_opt_in_and_qemu_free() {
        let probe = probe_hvf_mmio_write_emulation(false);
        let output = probe.render_text();

        assert!(!probe.allowed);
        assert!(!probe.attempted);
        assert!(!probe.vm_created);
        assert!(!probe.memory_mapped);
        assert!(!probe.vcpu_created);
        assert!(!probe.first_run_attempted);
        assert!(!probe.mmio_exit_observed);
        assert!(!probe.write_value_captured);
        assert!(!probe.pc_advanced);
        assert!(!probe.second_run_attempted);
        assert!(!probe.continuation_exit_observed);
        assert!(!probe.write_value_preserved);
        assert!(output.contains("HVF MMIO write emulation probe"));
        assert!(output.contains("QEMU: not used"));
        assert!(output.contains("Apple VZ: not used"));
        assert!(output.contains("Guest execution: unmapped STR, captured write value, then HVC"));
        assert!(output.contains("Allowed: false"));
        assert!(output.contains("Attempted: false"));
        assert!(output.contains("Instructions: STR X0, [X1]; HVC #0"));
        assert!(output.contains("Write value: 0xfedcba987654321"));
        assert!(output.contains("First run status name: not attempted"));
        assert!(output.contains("Second run status name: not attempted"));
        assert!(!output.contains("qemu-system"));
        assert!(!output.contains('%'));
    }

    #[test]
    fn mmio_serial_device_probe_is_opt_in_and_qemu_free() {
        let probe = probe_hvf_mmio_serial_device(false);
        let output = probe.render_text();

        assert!(!probe.allowed);
        assert!(!probe.attempted);
        assert!(!probe.vm_created);
        assert!(!probe.memory_mapped);
        assert!(!probe.vcpu_created);
        assert!(!probe.write_run_attempted);
        assert!(!probe.write_exit_observed);
        assert!(!probe.device_bus_created);
        assert_eq!(probe.device_bus_device_count, 0);
        assert!(!probe.write_handled_by_device);
        assert!(!probe.write_value_captured);
        assert!(!probe.status_run_attempted);
        assert!(!probe.status_exit_observed);
        assert!(!probe.status_handled_by_device);
        assert!(!probe.status_value_injected);
        assert!(!probe.continuation_run_attempted);
        assert!(!probe.continuation_exit_observed);
        assert!(output.contains("HVF MMIO serial device probe"));
        assert!(output.contains("QEMU: not used"));
        assert!(output.contains("Apple VZ: not used"));
        assert!(
            output.contains("Guest execution: STR data register, LDR status register, then HVC")
        );
        assert!(output.contains("Device model: PL011 UART skeleton"));
        assert!(output.contains("Allowed: false"));
        assert!(output.contains("Attempted: false"));
        assert!(output.contains("Device bus created: false"));
        assert!(output.contains("Device bus device count: 0"));
        assert!(output.contains("Write handled by device: false"));
        assert!(output.contains("Status handled by device: false"));
        assert!(output.contains("Serial data IPA: 0x50000000"));
        assert!(output.contains("Serial status IPA: 0x50000018"));
        assert!(output.contains("Instructions: STR X0, [X1]; LDR X0, [X2]; HVC #0"));
        assert!(output.contains("Serial write value: 0x41"));
        assert!(output.contains("Serial status value: 0x90"));
        assert!(output.contains("Write run status name: not attempted"));
        assert!(output.contains("Status run status name: not attempted"));
        assert!(output.contains("Continuation run status name: not attempted"));
        assert!(!output.contains("qemu-system"));
        assert!(!output.contains('%'));
    }

    #[test]
    fn mmio_rtc_device_probe_is_opt_in_and_qemu_free() {
        let probe = probe_hvf_mmio_rtc_device(false);
        let output = probe.render_text();

        assert!(!probe.allowed);
        assert!(!probe.attempted);
        assert!(!probe.vm_created);
        assert!(!probe.memory_mapped);
        assert!(!probe.vcpu_created);
        assert!(!probe.device_bus_created);
        assert_eq!(probe.device_bus_device_count, 0);
        assert!(!probe.first_run_attempted);
        assert!(!probe.rtc_exit_observed);
        assert!(!probe.rtc_handled_by_device);
        assert!(!probe.rtc_value_injected);
        assert!(!probe.second_run_attempted);
        assert!(!probe.continuation_exit_observed);
        assert!(output.contains("HVF MMIO RTC device probe"));
        assert!(output.contains("QEMU: not used"));
        assert!(output.contains("Apple VZ: not used"));
        assert!(output.contains("Guest execution: LDR RTC data register, then HVC"));
        assert!(output.contains("Device models: PL011 UART skeleton; PL031 RTC skeleton"));
        assert!(output.contains("Allowed: false"));
        assert!(output.contains("Attempted: false"));
        assert!(output.contains("Device bus created: false"));
        assert!(output.contains("Device bus device count: 0"));
        assert!(output.contains("RTC handled by device: false"));
        assert!(output.contains("RTC value injected: false"));
        assert!(output.contains("UART IPA: 0x50000000"));
        assert!(output.contains("RTC IPA: 0x50001000"));
        assert!(output.contains("RTC value: 0x20260618"));
        assert!(output.contains("First run status name: not attempted"));
        assert!(output.contains("Second run status name: not attempted"));
        assert!(!output.contains("qemu-system"));
        assert!(!output.contains('%'));
    }

    #[test]
    fn mmio_block_device_probe_is_opt_in_and_qemu_free() {
        let probe = probe_hvf_mmio_block_device(false);
        let output = probe.render_text();

        assert!(!probe.allowed);
        assert!(!probe.attempted);
        assert!(!probe.vm_created);
        assert!(!probe.memory_mapped);
        assert!(!probe.vcpu_created);
        assert!(!probe.device_bus_created);
        assert_eq!(probe.device_bus_device_count, 0);
        assert_eq!(probe.register_reads.len(), 4);
        assert!(probe.register_reads.iter().all(|read| !read.run_attempted));
        assert!(!probe.continuation_run_attempted);
        assert!(!probe.continuation_exit_observed);
        assert!(!probe.vendor_value_preserved);
        assert!(output.contains("HVF MMIO block device probe"));
        assert!(output.contains("QEMU: not used"));
        assert!(output.contains("Apple VZ: not used"));
        assert!(output.contains("Guest execution: LDR W0 VirtIO-MMIO identity registers, then HVC"));
        assert!(output.contains(
            "Device models: PL011 UART skeleton; PL031 RTC skeleton; VirtIO-MMIO block identity skeleton"
        ));
        assert!(output.contains("Allowed: false"));
        assert!(output.contains("Attempted: false"));
        assert!(output.contains("Device bus created: false"));
        assert!(output.contains("Device bus device count: 0"));
        assert!(output.contains("magic at 0x50002000: expected 0x74726976"));
        assert!(output.contains("version at 0x50002004: expected 0x2"));
        assert!(output.contains("device_id at 0x50002008: expected 0x2"));
        assert!(output.contains("vendor_id at 0x5000200c: expected 0x4252564d"));
        assert!(output.contains("Continuation exit observed: false"));
        assert!(output.contains("Vendor value preserved: false"));
        assert!(output.contains("Block IPA: 0x50002000"));
        assert!(output.contains("VirtIO magic value: 0x74726976"));
        assert!(output.contains("VirtIO version value: 0x2"));
        assert!(output.contains("VirtIO block device ID value: 0x2"));
        assert!(output.contains("VirtIO vendor ID value: 0x4252564d"));
        assert!(!output.contains("qemu-system"));
        assert!(!output.contains('%'));
    }

    #[test]
    fn mmio_block_queue_probe_is_opt_in_and_qemu_free() {
        let probe = probe_hvf_mmio_block_queue(false, None, None, None);
        let output = probe.render_text();

        assert!(!probe.allowed);
        assert!(!probe.attempted);
        assert!(!probe.vm_created);
        assert!(!probe.memory_mapped);
        assert!(!probe.vcpu_created);
        assert!(!probe.device_bus_created);
        assert_eq!(probe.device_bus_device_count, 0);
        assert_eq!(probe.steps.len(), 26);
        assert!(probe.steps.iter().all(|step| !step.run_attempted));
        assert!(!probe.continuation_run_attempted);
        assert!(!probe.continuation_exit_observed);
        assert!(!probe.capacity_high_value_preserved);
        assert_eq!(probe.block_backing_kind, "synthetic-sector-pattern");
        assert_eq!(probe.block_backing_path, None);
        assert!(!probe.request_ring_seeded);
        assert!(!probe.request_completed_after_notify);
        assert_eq!(probe.request_descriptor_index, None);
        assert_eq!(probe.request_sector, None);
        assert_eq!(probe.request_byte_offset, None);
        assert_eq!(probe.request_data_bytes, None);
        assert!(probe.request_data_prefix.is_empty());
        assert_eq!(probe.request_status, None);
        assert_eq!(probe.request_used_index, None);
        assert_eq!(probe.request_used_len, None);
        assert_eq!(probe.request_interrupt_status, None);
        assert!(!probe.write_completed_after_notify);
        assert_eq!(probe.write_request_type, None);
        assert_eq!(probe.write_sector, None);
        assert_eq!(probe.write_byte_offset, None);
        assert_eq!(probe.write_data_bytes, None);
        assert!(probe.write_data_prefix.is_empty());
        assert_eq!(probe.write_status, None);
        assert_eq!(probe.write_used_index, None);
        assert_eq!(probe.write_used_len, None);
        assert!(!probe.flush_completed_after_notify);
        assert_eq!(probe.flush_request_type, None);
        assert_eq!(probe.flush_status, None);
        assert_eq!(probe.flush_used_index, None);
        assert_eq!(probe.flush_used_len, None);
        assert!(probe.persisted_data_prefix.is_empty());
        assert!(output.contains("HVF MMIO block queue/config/address/notify probe"));
        assert!(output.contains("QEMU: not used"));
        assert!(output.contains("Apple VZ: not used"));
        assert!(output.contains(
            "Guest execution: VirtIO-MMIO feature, queue, ring address, notify, status, and capacity registers, then HVC"
        ));
        assert!(output.contains(
            "Device models: PL011 UART skeleton; PL031 RTC skeleton; VirtIO-MMIO block identity skeleton; VirtIO-MMIO block queue/config/address/notify skeleton"
        ));
        assert!(output.contains("Allowed: false"));
        assert!(output.contains("Attempted: false"));
        assert!(output.contains("Device bus created: false"));
        assert!(output.contains("Device bus device count: 0"));
        assert!(output.contains("read device_features at 0x50002010: expected=0x0"));
        assert!(output
            .contains("write driver_features at 0x50002020: expected=not observed, write=0x0"));
        assert!(output.contains("write status_ack at 0x50002070: expected=not observed, write=0x1"));
        assert!(
            output.contains("write status_driver at 0x50002070: expected=not observed, write=0x3")
        );
        assert!(output
            .contains("write status_features_ok at 0x50002070: expected=not observed, write=0xb"));
        assert!(
            output.contains("write queue_select at 0x50002030: expected=not observed, write=0x0")
        );
        assert!(output.contains("read queue_num_max at 0x50002034: expected=0x80"));
        assert!(output.contains("write queue_num at 0x50002038: expected=not observed, write=0x8"));
        assert!(output.contains(
            "write queue_desc_low at 0x50002080: expected=not observed, write=0x40001000"
        ));
        assert!(output
            .contains("write queue_desc_high at 0x50002084: expected=not observed, write=0x0"));
        assert!(output.contains(
            "write queue_driver_low at 0x50002090: expected=not observed, write=0x40002000"
        ));
        assert!(output
            .contains("write queue_driver_high at 0x50002094: expected=not observed, write=0x0"));
        assert!(output.contains(
            "write queue_device_low at 0x500020a0: expected=not observed, write=0x40003000"
        ));
        assert!(output
            .contains("write queue_device_high at 0x500020a4: expected=not observed, write=0x0"));
        assert!(
            output.contains("write queue_ready at 0x50002044: expected=not observed, write=0x1")
        );
        assert!(output
            .contains("write status_driver_ok at 0x50002070: expected=not observed, write=0xf"));
        assert!(output.contains("read status at 0x50002070: expected=0xf"));
        assert!(
            output.contains("write queue_notify at 0x50002050: expected=not observed, write=0x0")
        );
        assert!(output.contains("read queue_ready at 0x50002044: expected=0x1"));
        assert!(output.contains("read queue_desc_low at 0x50002080: expected=0x40001000"));
        assert!(output.contains("read queue_driver_low at 0x50002090: expected=0x40002000"));
        assert!(output.contains("read queue_device_low at 0x500020a0: expected=0x40003000"));
        assert!(output.contains("read interrupt_status at 0x50002060: expected=0x1"));
        assert!(output.contains("read config_generation at 0x500020fc: expected=0x0"));
        assert!(output.contains("read capacity_low at 0x50002100: expected=0x4000"));
        assert!(output.contains("read capacity_high at 0x50002104: expected=0x0"));
        assert!(output.contains("Continuation exit observed: false"));
        assert!(output.contains("Capacity high value preserved: false"));
        assert!(output.contains("Block IPA: 0x50002000"));
        assert!(output.contains(
            "Instructions: LDR/STR W0 VirtIO-MMIO queue/config/address/notify registers; HVC #0"
        ));
        assert!(output.contains("Queue num max value: 0x80"));
        assert!(output.contains("Queue descriptor address: 0x40001000"));
        assert!(output.contains("Queue driver address: 0x40002000"));
        assert!(output.contains("Queue device address: 0x40003000"));
        assert!(output.contains("Queue notify value: 0x0"));
        assert!(output.contains("Interrupt status value: 0x1"));
        assert!(output.contains("Block backing kind: synthetic-sector-pattern"));
        assert!(output.contains("Block backing path: not observed"));
        assert!(output.contains("Request ring seeded: false"));
        assert!(output.contains("Request completed after notify: false"));
        assert!(output.contains("Request descriptor index: not observed"));
        assert!(output.contains("Request sector: not observed"));
        assert!(output.contains("Request byte offset: not observed"));
        assert!(output.contains("Request data bytes: not observed"));
        assert!(output.contains("Request data prefix: not observed"));
        assert!(output.contains("Request status byte: not observed"));
        assert!(output.contains("Request used index: not observed"));
        assert!(output.contains("Request used length: not observed"));
        assert!(output.contains("Request interrupt status: not observed"));
        assert!(output.contains("Write completed after notify: false"));
        assert!(output.contains("Write request type: not observed"));
        assert!(output.contains("Write sector: not observed"));
        assert!(output.contains("Write byte offset: not observed"));
        assert!(output.contains("Write data bytes: not observed"));
        assert!(output.contains("Write data prefix: not observed"));
        assert!(output.contains("Write status byte: not observed"));
        assert!(output.contains("Write used index: not observed"));
        assert!(output.contains("Write used length: not observed"));
        assert!(output.contains("Flush completed after notify: false"));
        assert!(output.contains("Flush request type: not observed"));
        assert!(output.contains("Flush status byte: not observed"));
        assert!(output.contains("Flush used index: not observed"));
        assert!(output.contains("Flush used length: not observed"));
        assert!(output.contains("Persisted data prefix: not observed"));
        assert!(output.contains("Status value: 0xf"));
        assert!(output.contains("Capacity sectors: 0x4000"));
        assert!(!output.contains("qemu-system"));
        assert!(!output.contains('%'));
    }

    #[test]
    fn mmio_block_queue_probe_reports_file_backing_without_live_opt_in() {
        let disk_path = PathBuf::from("/tmp/bridgevm-live-block.img");
        let probe = probe_hvf_mmio_block_queue(false, Some(disk_path.clone()), None, None);
        let output = probe.render_text();

        assert!(!probe.allowed);
        assert!(!probe.attempted);
        assert_eq!(probe.block_backing_kind, "host-file");
        assert_eq!(probe.block_backing_path, Some(disk_path.clone()));
        assert!(!probe.request_completed_after_notify);
        assert_eq!(probe.request_byte_offset, None);
        assert!(output.contains("Block backing kind: host-file"));
        assert!(output.contains(&format!("Block backing path: {}", disk_path.display())));
        assert!(output.contains("Request byte offset: not observed"));
        assert!(!output.contains("qemu-system"));
        assert!(!output.contains('%'));
    }

    #[test]
    fn mmio_block_queue_probe_reports_iso_backing_without_live_opt_in() {
        let iso_path = PathBuf::from("/tmp/Win11_Arm64.iso");
        let probe = probe_hvf_mmio_block_queue(false, None, Some(iso_path.clone()), None);
        let output = probe.render_text();

        assert!(!probe.allowed);
        assert!(!probe.attempted);
        assert_eq!(probe.block_backing_kind, "host-iso-readonly");
        assert_eq!(probe.block_backing_path, Some(iso_path.clone()));
        assert!(!probe.request_completed_after_notify);
        assert_eq!(probe.request_byte_offset, None);
        assert!(output.contains("Block backing kind: host-iso-readonly"));
        assert!(output.contains(&format!("Block backing path: {}", iso_path.display())));
        assert!(output.contains("Request byte offset: not observed"));
        assert!(!output.contains("qemu-system"));
        assert!(!output.contains('%'));
    }

    #[test]
    fn mmio_block_queue_probe_reports_writable_file_backing_without_live_opt_in() {
        let disk_path = PathBuf::from("/tmp/bridgevm-writable-live-block.img");
        let probe = probe_hvf_mmio_block_queue(false, None, None, Some(disk_path.clone()));
        let output = probe.render_text();

        assert!(!probe.allowed);
        assert!(!probe.attempted);
        assert_eq!(probe.block_backing_kind, "host-file-writable");
        assert_eq!(probe.block_backing_path, Some(disk_path.clone()));
        assert!(!probe.request_completed_after_notify);
        assert!(!probe.write_completed_after_notify);
        assert!(!probe.flush_completed_after_notify);
        assert_eq!(probe.request_byte_offset, None);
        assert_eq!(probe.write_byte_offset, None);
        assert!(probe.persisted_data_prefix.is_empty());
        assert!(output.contains("Block backing kind: host-file-writable"));
        assert!(output.contains(&format!("Block backing path: {}", disk_path.display())));
        assert!(output.contains("Request byte offset: not observed"));
        assert!(output.contains("Write completed after notify: false"));
        assert!(output.contains("Flush completed after notify: false"));
        assert!(output.contains("Persisted data prefix: not observed"));
        assert!(!output.contains("qemu-system"));
        assert!(!output.contains('%'));
    }
}
