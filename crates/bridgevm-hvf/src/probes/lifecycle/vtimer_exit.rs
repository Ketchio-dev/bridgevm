//! Split out of lifecycle.rs by responsibility.

use super::super::*;
use crate::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HvfVtimerExitProbe {
    pub allowed: bool,
    pub attempted: bool,
    pub vm_created: bool,
    pub memory_allocated: bool,
    pub memory_mapped: bool,
    pub vcpu_created: bool,
    pub pc_set: bool,
    pub cpsr_set: bool,
    pub vtimer_offset_set: bool,
    pub cntv_cval_set: bool,
    pub cntv_ctl_set: bool,
    pub vtimer_unmasked: bool,
    pub run_attempted: bool,
    pub vtimer_exit_observed: bool,
    pub pending_irq_injected: bool,
    pub vtimer_mask_observed_after_exit: Option<bool>,
    pub vtimer_unmasked_after_exit: bool,
    pub watchdog_cancel_fired: bool,
    pub vcpu_destroyed: bool,
    pub memory_unmapped: bool,
    pub vm_destroyed: bool,
    pub memory_deallocated: bool,
    pub host: HvfHostCapabilities,
    pub ipa_start: u64,
    pub bytes: usize,
    pub instructions: &'static str,
    pub vtimer_offset_value: u64,
    pub cntv_cval_value: u64,
    pub cntv_ctl_value: u64,
    pub vm_create_status: Option<i32>,
    pub allocate_status: Option<i32>,
    pub map_status: Option<i32>,
    pub vcpu_create_status: Option<i32>,
    pub pc_set_status: Option<i32>,
    pub cpsr_set_status: Option<i32>,
    pub vtimer_offset_set_status: Option<i32>,
    pub cntv_cval_set_status: Option<i32>,
    pub cntv_ctl_set_status: Option<i32>,
    pub vtimer_unmask_status: Option<i32>,
    pub run_status: Option<i32>,
    pub exit_reason: Option<u32>,
    pub exit_syndrome: Option<u64>,
    pub exit_virtual_address: Option<u64>,
    pub exit_physical_address: Option<u64>,
    pub watchdog_cancel_status: Option<i32>,
    pub pending_irq_set_status: Option<i32>,
    pub vtimer_mask_get_after_exit_status: Option<i32>,
    pub vtimer_unmask_after_exit_status: Option<i32>,
    pub vcpu_destroy_status: Option<i32>,
    pub unmap_status: Option<i32>,
    pub vm_destroy_status: Option<i32>,
    pub deallocate_status: Option<i32>,
    pub blockers: Vec<String>,
}

impl HvfVtimerExitProbe {
    pub fn render_text(&self) -> String {
        let mut output = String::new();
        output.push_str("HVF VTimer exit probe\n");
        output.push_str("QEMU: not used\n");
        output.push_str("Apple VZ: not used\n");
        output.push_str("Guest execution: WFI wait loop with host-programmed virtual timer\n");
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
        output.push_str(&format!("VTimer offset set: {}\n", self.vtimer_offset_set));
        output.push_str(&format!("CNTV_CVAL_EL0 set: {}\n", self.cntv_cval_set));
        output.push_str(&format!("CNTV_CTL_EL0 set: {}\n", self.cntv_ctl_set));
        output.push_str(&format!("VTimer unmasked: {}\n", self.vtimer_unmasked));
        output.push_str(&format!("Run attempted: {}\n", self.run_attempted));
        output.push_str(&format!(
            "VTimer exit observed: {}\n",
            self.vtimer_exit_observed
        ));
        output.push_str(&format!(
            "Pending IRQ injected: {}\n",
            self.pending_irq_injected
        ));
        output.push_str(&format!(
            "VTimer mask observed after exit: {}\n",
            render_optional_bool(self.vtimer_mask_observed_after_exit)
        ));
        output.push_str(&format!(
            "VTimer unmasked after exit: {}\n",
            self.vtimer_unmasked_after_exit
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
        output.push_str(&format!("IPA start: {:#x}\n", self.ipa_start));
        output.push_str(&format!("Bytes: {:#x}\n", self.bytes));
        output.push_str(&format!("Instructions: {}\n", self.instructions));
        output.push_str(&format!(
            "VTimer offset requested: {:#x}\n",
            self.vtimer_offset_value
        ));
        output.push_str(&format!(
            "CNTV_CVAL_EL0 requested: {:#x}\n",
            self.cntv_cval_value
        ));
        output.push_str(&format!(
            "CNTV_CTL_EL0 requested: {:#x}\n",
            self.cntv_ctl_value
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
            "VTimer offset set status name: {}\n",
            render_optional_status_name(self.vtimer_offset_set_status)
        ));
        output.push_str(&format!(
            "CNTV_CVAL_EL0 set status name: {}\n",
            render_optional_status_name(self.cntv_cval_set_status)
        ));
        output.push_str(&format!(
            "CNTV_CTL_EL0 set status name: {}\n",
            render_optional_status_name(self.cntv_ctl_set_status)
        ));
        output.push_str(&format!(
            "VTimer unmask status name: {}\n",
            render_optional_status_name(self.vtimer_unmask_status)
        ));
        output.push_str(&format!(
            "Watchdog cancel status name: {}\n",
            render_optional_status_name(self.watchdog_cancel_status)
        ));
        output.push_str(&format!(
            "Pending IRQ set status name: {}\n",
            render_optional_status_name(self.pending_irq_set_status)
        ));
        output.push_str(&format!(
            "VTimer mask get after exit status name: {}\n",
            render_optional_status_name(self.vtimer_mask_get_after_exit_status)
        ));
        output.push_str(&format!(
            "VTimer unmask after exit status name: {}\n",
            render_optional_status_name(self.vtimer_unmask_after_exit_status)
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

pub fn probe_hvf_vtimer_exit(allow_probe: bool) -> HvfVtimerExitProbe {
    let host = query_hvf_host_capabilities();
    platform::probe_hvf_vtimer_exit(allow_probe, host)
}
