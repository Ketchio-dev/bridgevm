//! Split out of windows_arm.rs by responsibility.

use super::*;
use crate::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsArmUefiResetVectorEntryProbe {
    pub allowed: bool,
    pub attempted: bool,
    pub vm_created: bool,
    pub firmware_memory_allocated: bool,
    pub vars_memory_allocated: bool,
    pub firmware_memory_populated: bool,
    pub vars_memory_populated: bool,
    pub firmware_memory_mapped: bool,
    pub vars_memory_mapped: bool,
    pub vcpu_created: bool,
    pub pc_set: bool,
    pub cpsr_set: bool,
    pub run_attempted: bool,
    pub reset_vector_entry_observed: bool,
    pub firmware_progress_observed: bool,
    pub watchdog_cancel_fired: bool,
    pub vcpu_destroyed: bool,
    pub firmware_memory_unmapped: bool,
    pub vars_memory_unmapped: bool,
    pub firmware_memory_deallocated: bool,
    pub vars_memory_deallocated: bool,
    pub vm_destroyed: bool,
    pub host: HvfHostCapabilities,
    pub pflash_map_verified: bool,
    pub reset_vector_ipa: u64,
    pub firmware_slot_ipa: u64,
    pub vars_slot_ipa: u64,
    pub slot_bytes: u64,
    pub firmware_source_bytes: Option<u64>,
    pub vars_source_bytes: Option<u64>,
    pub firmware_map_flags: &'static str,
    pub vars_map_flags: &'static str,
    pub vm_create_status: Option<i32>,
    pub firmware_allocate_status: Option<i32>,
    pub vars_allocate_status: Option<i32>,
    pub firmware_map_status: Option<i32>,
    pub vars_map_status: Option<i32>,
    pub vcpu_create_status: Option<i32>,
    pub pc_set_status: Option<i32>,
    pub cpsr_set_status: Option<i32>,
    pub run_status: Option<i32>,
    pub exit_reason: Option<u32>,
    pub exit_syndrome: Option<u64>,
    pub exit_exception_class: Option<u64>,
    pub exit_virtual_address: Option<u64>,
    pub exit_physical_address: Option<u64>,
    pub pc_after_run_status: Option<i32>,
    pub pc_after_run: Option<u64>,
    pub watchdog_cancel_status: Option<i32>,
    pub vcpu_destroy_status: Option<i32>,
    pub firmware_unmap_status: Option<i32>,
    pub vars_unmap_status: Option<i32>,
    pub firmware_deallocate_status: Option<i32>,
    pub vars_deallocate_status: Option<i32>,
    pub vm_destroy_status: Option<i32>,
    pub blockers: Vec<String>,
}

impl WindowsArmUefiResetVectorEntryProbe {
    pub fn render_text(&self) -> String {
        let mut output = String::new();
        output.push_str("Windows 11 Arm HVF UEFI reset-vector entry probe\n");
        output.push_str("QEMU: not used\n");
        output.push_str("Apple VZ: not used\n");
        output.push_str("Guest execution: UEFI reset vector entered under watchdog\n");
        output.push_str("Windows boot: not claimed\n");
        output.push_str(&format!("Host: {}\n", self.host.host));
        output.push_str(&format!("Host HVF available: {}\n", self.host.available));
        output.push_str(&format!("Allowed: {}\n", self.allowed));
        output.push_str(&format!("Attempted: {}\n", self.attempted));
        output.push_str(&format!("VM created: {}\n", self.vm_created));
        output.push_str(&format!(
            "Firmware memory allocated: {}\n",
            self.firmware_memory_allocated
        ));
        output.push_str(&format!(
            "Vars memory allocated: {}\n",
            self.vars_memory_allocated
        ));
        output.push_str(&format!(
            "Firmware memory populated: {}\n",
            self.firmware_memory_populated
        ));
        output.push_str(&format!(
            "Vars memory populated: {}\n",
            self.vars_memory_populated
        ));
        output.push_str(&format!(
            "Firmware memory mapped: {}\n",
            self.firmware_memory_mapped
        ));
        output.push_str(&format!(
            "Vars memory mapped: {}\n",
            self.vars_memory_mapped
        ));
        output.push_str(&format!("vCPU created: {}\n", self.vcpu_created));
        output.push_str(&format!("PC set: {}\n", self.pc_set));
        output.push_str(&format!("CPSR set: {}\n", self.cpsr_set));
        output.push_str(&format!("Run attempted: {}\n", self.run_attempted));
        output.push_str(&format!(
            "Reset-vector entry observed: {}\n",
            self.reset_vector_entry_observed
        ));
        output.push_str(&format!(
            "Firmware progress observed: {}\n",
            self.firmware_progress_observed
        ));
        output.push_str(&format!(
            "Watchdog cancel fired: {}\n",
            self.watchdog_cancel_fired
        ));
        output.push_str(&format!("vCPU destroyed: {}\n", self.vcpu_destroyed));
        output.push_str(&format!(
            "Firmware memory unmapped: {}\n",
            self.firmware_memory_unmapped
        ));
        output.push_str(&format!(
            "Vars memory unmapped: {}\n",
            self.vars_memory_unmapped
        ));
        output.push_str(&format!(
            "Firmware memory deallocated: {}\n",
            self.firmware_memory_deallocated
        ));
        output.push_str(&format!(
            "Vars memory deallocated: {}\n",
            self.vars_memory_deallocated
        ));
        output.push_str(&format!("VM destroyed: {}\n", self.vm_destroyed));
        output.push_str(&format!(
            "Pflash map verified: {}\n",
            self.pflash_map_verified
        ));
        output.push_str(&format!("Reset vector IPA: {:#x}\n", self.reset_vector_ipa));
        output.push_str(&format!(
            "Firmware slot IPA: {:#x}\n",
            self.firmware_slot_ipa
        ));
        output.push_str(&format!("Vars slot IPA: {:#x}\n", self.vars_slot_ipa));
        output.push_str(&format!("Slot bytes: {:#x}\n", self.slot_bytes));
        output.push_str(&format!(
            "Firmware source bytes: {}\n",
            render_optional_u64(self.firmware_source_bytes)
        ));
        output.push_str(&format!(
            "Vars source bytes: {}\n",
            render_optional_u64(self.vars_source_bytes)
        ));
        output.push_str(&format!(
            "Firmware map flags: {}\n",
            self.firmware_map_flags
        ));
        output.push_str(&format!("Vars map flags: {}\n", self.vars_map_flags));
        output.push_str(&format!(
            "VM create status name: {}\n",
            render_optional_status_name(self.vm_create_status)
        ));
        output.push_str(&format!(
            "Firmware allocate status name: {}\n",
            render_optional_status_name(self.firmware_allocate_status)
        ));
        output.push_str(&format!(
            "Vars allocate status name: {}\n",
            render_optional_status_name(self.vars_allocate_status)
        ));
        output.push_str(&format!(
            "Firmware map status name: {}\n",
            render_optional_status_name(self.firmware_map_status)
        ));
        output.push_str(&format!(
            "Vars map status name: {}\n",
            render_optional_status_name(self.vars_map_status)
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
            "Run status: {}\n",
            render_optional_status(self.run_status)
        ));
        output.push_str(&format!(
            "Run status name: {}\n",
            render_optional_status_name(self.run_status)
        ));
        output.push_str(&format!(
            "Exit reason: {}\n",
            render_optional_exit_reason(self.exit_reason)
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
            "Exit exception class: {}\n",
            render_optional_u64(self.exit_exception_class)
        ));
        output.push_str(&format!(
            "Exit exception class name: {}\n",
            render_optional_exception_class_name(self.exit_exception_class)
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
            "PC after run status name: {}\n",
            render_optional_status_name(self.pc_after_run_status)
        ));
        output.push_str(&format!(
            "PC after run: {}\n",
            render_optional_u64(self.pc_after_run)
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
            "Firmware unmap status name: {}\n",
            render_optional_status_name(self.firmware_unmap_status)
        ));
        output.push_str(&format!(
            "Vars unmap status name: {}\n",
            render_optional_status_name(self.vars_unmap_status)
        ));
        output.push_str(&format!(
            "Firmware deallocate status name: {}\n",
            render_optional_status_name(self.firmware_deallocate_status)
        ));
        output.push_str(&format!(
            "Vars deallocate status name: {}\n",
            render_optional_status_name(self.vars_deallocate_status)
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
