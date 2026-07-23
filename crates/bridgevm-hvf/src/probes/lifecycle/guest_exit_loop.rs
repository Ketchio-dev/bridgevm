//! Split out of lifecycle.rs by responsibility.

use super::super::*;
use crate::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HvfGuestExitLoopProbe {
    pub allowed: bool,
    pub attempted: bool,
    pub vm_created: bool,
    pub memory_allocated: bool,
    pub memory_mapped: bool,
    pub vcpu_created: bool,
    pub initial_pc_set: bool,
    pub cpsr_set: bool,
    pub first_run_attempted: bool,
    pub first_exit_observed: bool,
    pub pc_read_after_first_exit: bool,
    pub pc_advanced: bool,
    pub second_run_attempted: bool,
    pub second_exit_observed: bool,
    pub exit_loop_observed: bool,
    pub watchdog_cancel_fired: bool,
    pub vcpu_destroyed: bool,
    pub memory_unmapped: bool,
    pub vm_destroyed: bool,
    pub memory_deallocated: bool,
    pub host: HvfHostCapabilities,
    pub ipa_start: u64,
    pub bytes: usize,
    pub instructions: &'static str,
    pub vm_create_status: Option<i32>,
    pub allocate_status: Option<i32>,
    pub map_status: Option<i32>,
    pub vcpu_create_status: Option<i32>,
    pub initial_pc_set_status: Option<i32>,
    pub cpsr_set_status: Option<i32>,
    pub first_run_status: Option<i32>,
    pub first_exit_reason: Option<u32>,
    pub first_exit_syndrome: Option<u64>,
    pub first_exit_virtual_address: Option<u64>,
    pub first_exit_physical_address: Option<u64>,
    pub first_watchdog_cancel_status: Option<i32>,
    pub pc_read_status: Option<i32>,
    pub pc_after_first_exit: Option<u64>,
    pub pc_advance_status: Option<i32>,
    pub second_run_status: Option<i32>,
    pub second_exit_reason: Option<u32>,
    pub second_exit_syndrome: Option<u64>,
    pub second_exit_virtual_address: Option<u64>,
    pub second_exit_physical_address: Option<u64>,
    pub second_watchdog_cancel_status: Option<i32>,
    pub vcpu_destroy_status: Option<i32>,
    pub unmap_status: Option<i32>,
    pub vm_destroy_status: Option<i32>,
    pub deallocate_status: Option<i32>,
    pub blockers: Vec<String>,
}

impl HvfGuestExitLoopProbe {
    pub fn render_text(&self) -> String {
        let mut output = String::new();
        output.push_str("HVF guest exit loop probe\n");
        output.push_str("QEMU: not used\n");
        output.push_str("Apple VZ: not used\n");
        output.push_str("Guest execution: two HVC instructions with PC advance watchdog\n");
        output.push_str(&format!("Host: {}\n", self.host.host));
        output.push_str(&format!("Host HVF available: {}\n", self.host.available));
        output.push_str(&format!("Allowed: {}\n", self.allowed));
        output.push_str(&format!("Attempted: {}\n", self.attempted));
        output.push_str(&format!("VM created: {}\n", self.vm_created));
        output.push_str(&format!("Memory allocated: {}\n", self.memory_allocated));
        output.push_str(&format!("Memory mapped: {}\n", self.memory_mapped));
        output.push_str(&format!("vCPU created: {}\n", self.vcpu_created));
        output.push_str(&format!("Initial PC set: {}\n", self.initial_pc_set));
        output.push_str(&format!("CPSR set: {}\n", self.cpsr_set));
        output.push_str(&format!(
            "First run attempted: {}\n",
            self.first_run_attempted
        ));
        output.push_str(&format!(
            "First exit observed: {}\n",
            self.first_exit_observed
        ));
        output.push_str(&format!(
            "PC read after first exit: {}\n",
            self.pc_read_after_first_exit
        ));
        output.push_str(&format!("PC advanced: {}\n", self.pc_advanced));
        output.push_str(&format!(
            "Second run attempted: {}\n",
            self.second_run_attempted
        ));
        output.push_str(&format!(
            "Second exit observed: {}\n",
            self.second_exit_observed
        ));
        output.push_str(&format!(
            "Exit loop observed: {}\n",
            self.exit_loop_observed
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
        output.push_str(&format!("Instructions: {}\n", self.instructions));
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
            "Initial PC set status name: {}\n",
            render_optional_status_name(self.initial_pc_set_status)
        ));
        output.push_str(&format!(
            "CPSR set status name: {}\n",
            render_optional_status_name(self.cpsr_set_status)
        ));
        output.push_str(&format!(
            "First run status name: {}\n",
            render_optional_status_name(self.first_run_status)
        ));
        output.push_str(&format!(
            "First exit reason name: {}\n",
            render_optional_exit_reason_name(self.first_exit_reason)
        ));
        output.push_str(&format!(
            "First exit syndrome: {}\n",
            render_optional_u64(self.first_exit_syndrome)
        ));
        output.push_str(&format!(
            "First exit virtual address: {}\n",
            render_optional_u64(self.first_exit_virtual_address)
        ));
        output.push_str(&format!(
            "First exit physical address: {}\n",
            render_optional_u64(self.first_exit_physical_address)
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
            "PC after first exit: {}\n",
            render_optional_u64(self.pc_after_first_exit)
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
            "Second exit reason name: {}\n",
            render_optional_exit_reason_name(self.second_exit_reason)
        ));
        output.push_str(&format!(
            "Second exit syndrome: {}\n",
            render_optional_u64(self.second_exit_syndrome)
        ));
        output.push_str(&format!(
            "Second exit virtual address: {}\n",
            render_optional_u64(self.second_exit_virtual_address)
        ));
        output.push_str(&format!(
            "Second exit physical address: {}\n",
            render_optional_u64(self.second_exit_physical_address)
        ));
        output.push_str(&format!(
            "Second watchdog cancel status name: {}\n",
            render_optional_status_name(self.second_watchdog_cancel_status)
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

pub fn probe_hvf_guest_exit_loop(allow_loop: bool) -> HvfGuestExitLoopProbe {
    let host = query_hvf_host_capabilities();
    platform::probe_hvf_guest_exit_loop(allow_loop, host)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vm_create_probe_is_opt_in_and_qemu_free() {
        let probe = probe_hvf_vm_create(false);
        let output = probe.render_text();

        assert!(!probe.allowed);
        assert!(!probe.attempted);
        assert!(!probe.created);
        assert!(!probe.destroyed);
        assert!(output.contains("HVF VM create/destroy probe"));
        assert!(output.contains("QEMU: not used"));
        assert!(output.contains("Apple VZ: not used"));
        assert!(output.contains("Allowed: false"));
        assert!(output.contains("Attempted: false"));
        assert!(output.contains("Created: false"));
        assert!(output.contains("Destroyed: false"));
        assert!(output.contains("Create status: not attempted"));
        assert!(output.contains("Create status name: not attempted"));
        assert!(output.contains("Destroy status: not attempted"));
        assert!(output.contains("Destroy status name: not attempted"));
        assert!(!output.contains("qemu-system"));
        assert!(!output.contains('%'));
    }

    #[test]
    fn vcpu_create_probe_is_opt_in_and_qemu_free() {
        let probe = probe_hvf_vcpu_create(false);
        let output = probe.render_text();

        assert!(!probe.allowed);
        assert!(!probe.attempted);
        assert!(!probe.vm_created);
        assert!(!probe.vcpu_created);
        assert!(!probe.vcpu_destroyed);
        assert!(!probe.vm_destroyed);
        assert!(output.contains("HVF vCPU create/destroy probe"));
        assert!(output.contains("QEMU: not used"));
        assert!(output.contains("Apple VZ: not used"));
        assert!(output.contains("Allowed: false"));
        assert!(output.contains("Attempted: false"));
        assert!(output.contains("VM created: false"));
        assert!(output.contains("vCPU created: false"));
        assert!(output.contains("vCPU destroyed: false"));
        assert!(output.contains("VM destroyed: false"));
        assert!(output.contains("VM create status name: not attempted"));
        assert!(output.contains("vCPU create status name: not attempted"));
        assert!(!output.contains("qemu-system"));
        assert!(!output.contains('%'));
    }

    #[test]
    fn vcpu_run_probe_is_opt_in_and_qemu_free() {
        let probe = probe_hvf_vcpu_run(false);
        let output = probe.render_text();

        assert!(!probe.allowed);
        assert!(!probe.attempted);
        assert!(!probe.vm_created);
        assert!(!probe.vcpu_created);
        assert!(!probe.cancel_requested);
        assert!(!probe.run_attempted);
        assert!(!probe.run_boundary_observed);
        assert!(!probe.vcpu_destroyed);
        assert!(!probe.vm_destroyed);
        assert!(output.contains("HVF vCPU run/cancel probe"));
        assert!(output.contains("QEMU: not used"));
        assert!(output.contains("Apple VZ: not used"));
        assert!(output.contains("Guest execution: pre-canceled before entry"));
        assert!(output.contains("Allowed: false"));
        assert!(output.contains("Attempted: false"));
        assert!(output.contains("Cancel status name: not attempted"));
        assert!(output.contains("Run status name: not attempted"));
        assert!(output.contains("Exit reason name: not observed"));
        assert!(!output.contains("qemu-system"));
        assert!(!output.contains('%'));
    }

    #[test]
    fn interrupt_timer_probe_is_opt_in_and_qemu_free() {
        let probe = probe_hvf_interrupt_timer(false);
        let output = probe.render_text();

        assert!(!probe.allowed);
        assert!(!probe.attempted);
        assert!(!probe.vm_created);
        assert!(!probe.vcpu_created);
        assert!(!probe.pending_irq_set);
        assert!(!probe.pending_irq_cleared);
        assert!(!probe.vtimer_masked);
        assert!(!probe.vtimer_unmasked);
        assert!(!probe.vtimer_offset_set);
        assert!(!probe.boundary_observed);
        assert!(output.contains("HVF interrupt/timer probe"));
        assert!(output.contains("QEMU: not used"));
        assert!(output.contains("Apple VZ: not used"));
        assert!(output.contains("Guest execution: not entered"));
        assert!(output.contains("Allowed: false"));
        assert!(output.contains("Attempted: false"));
        assert!(output.contains("Pending IRQ after set: not observed"));
        assert!(output.contains("VTimer offset after set: not observed"));
        assert!(output.contains("Interrupt/timer boundary observed: false"));
        assert!(output.contains("IRQ set status name: not attempted"));
        assert!(output.contains("VTimer offset get status name: not attempted"));
        assert!(!output.contains("qemu-system"));
        assert!(!output.contains('%'));
    }

    #[test]
    fn vtimer_exit_probe_is_opt_in_and_qemu_free() {
        let probe = probe_hvf_vtimer_exit(false);
        let output = probe.render_text();

        assert!(!probe.allowed);
        assert!(!probe.attempted);
        assert!(!probe.vm_created);
        assert!(!probe.memory_mapped);
        assert!(!probe.vcpu_created);
        assert!(!probe.vtimer_offset_set);
        assert!(!probe.cntv_cval_set);
        assert!(!probe.cntv_ctl_set);
        assert!(!probe.vtimer_unmasked);
        assert!(!probe.run_attempted);
        assert!(!probe.vtimer_exit_observed);
        assert!(!probe.pending_irq_injected);
        assert_eq!(probe.vtimer_mask_observed_after_exit, None);
        assert!(!probe.vtimer_unmasked_after_exit);
        assert!(output.contains("HVF VTimer exit probe"));
        assert!(output.contains("QEMU: not used"));
        assert!(output.contains("Apple VZ: not used"));
        assert!(
            output.contains("Guest execution: WFI wait loop with host-programmed virtual timer")
        );
        assert!(output.contains("Allowed: false"));
        assert!(output.contains("Attempted: false"));
        assert!(output.contains("VTimer exit observed: false"));
        assert!(output.contains("Pending IRQ injected: false"));
        assert!(output.contains("VTimer mask observed after exit: not observed"));
        assert!(output.contains("Instructions: WFI; HVC #0"));
        assert!(output.contains("CNTV_CTL_EL0 requested: 0x1"));
        assert!(output.contains("Run status name: not attempted"));
        assert!(output.contains("Exit reason name: not observed"));
        assert!(output.contains("BRIDGEVM_HVF_ALLOW_VTIMER_EXIT"));
        assert!(!output.contains("qemu-system"));
        assert!(!output.contains('%'));
    }

    #[test]
    fn memory_map_probe_is_opt_in_and_qemu_free() {
        let probe = probe_hvf_memory_map(false);
        let output = probe.render_text();

        assert!(!probe.allowed);
        assert!(!probe.attempted);
        assert!(!probe.vm_created);
        assert!(!probe.memory_allocated);
        assert!(!probe.memory_mapped);
        assert!(!probe.memory_unmapped);
        assert!(!probe.memory_deallocated);
        assert!(!probe.vm_destroyed);
        assert!(output.contains("HVF memory map/unmap probe"));
        assert!(output.contains("QEMU: not used"));
        assert!(output.contains("Apple VZ: not used"));
        assert!(output.contains("Guest execution: not entered"));
        assert!(output.contains("Allowed: false"));
        assert!(output.contains("Attempted: false"));
        assert!(output.contains("Guest IPA start: 0x40000000"));
        assert!(output.contains("Bytes: 16384"));
        assert!(output.contains("Map status name: not attempted"));
        assert!(!output.contains("qemu-system"));
        assert!(!output.contains('%'));
    }

    #[test]
    fn guest_entry_probe_is_opt_in_and_qemu_free() {
        let probe = probe_hvf_guest_entry(false);
        let output = probe.render_text();

        assert!(!probe.allowed);
        assert!(!probe.attempted);
        assert!(!probe.vm_created);
        assert!(!probe.memory_mapped);
        assert!(!probe.vcpu_created);
        assert!(!probe.pc_set);
        assert!(!probe.cpsr_set);
        assert!(!probe.run_attempted);
        assert!(!probe.entry_boundary_observed);
        assert!(output.contains("HVF guest entry probe"));
        assert!(output.contains("QEMU: not used"));
        assert!(output.contains("Apple VZ: not used"));
        assert!(output.contains("Guest execution: one HVC instruction with watchdog"));
        assert!(output.contains("Allowed: false"));
        assert!(output.contains("Attempted: false"));
        assert!(output.contains("Instruction: HVC #0"));
        assert!(output.contains("Run status name: not attempted"));
        assert!(output.contains("Exit reason name: not observed"));
        assert!(!output.contains("qemu-system"));
        assert!(!output.contains('%'));
    }

    #[test]
    fn guest_exit_loop_probe_is_opt_in_and_qemu_free() {
        let probe = probe_hvf_guest_exit_loop(false);
        let output = probe.render_text();

        assert!(!probe.allowed);
        assert!(!probe.attempted);
        assert!(!probe.vm_created);
        assert!(!probe.memory_mapped);
        assert!(!probe.vcpu_created);
        assert!(!probe.first_run_attempted);
        assert!(!probe.first_exit_observed);
        assert!(!probe.pc_advanced);
        assert!(!probe.second_run_attempted);
        assert!(!probe.second_exit_observed);
        assert!(!probe.exit_loop_observed);
        assert!(output.contains("HVF guest exit loop probe"));
        assert!(output.contains("QEMU: not used"));
        assert!(output.contains("Apple VZ: not used"));
        assert!(output.contains("Guest execution: two HVC instructions with PC advance watchdog"));
        assert!(output.contains("Allowed: false"));
        assert!(output.contains("Attempted: false"));
        assert!(output.contains("Instructions: HVC #0; HVC #1"));
        assert!(output.contains("First run status name: not attempted"));
        assert!(output.contains("Second run status name: not attempted"));
        assert!(!output.contains("qemu-system"));
        assert!(!output.contains('%'));
    }
}
