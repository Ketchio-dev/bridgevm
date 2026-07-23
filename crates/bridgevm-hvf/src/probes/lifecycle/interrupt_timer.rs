//! Split out of lifecycle.rs by responsibility.

use super::super::*;
use crate::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HvfInterruptTimerProbe {
    pub allowed: bool,
    pub attempted: bool,
    pub vm_created: bool,
    pub vcpu_created: bool,
    pub pending_irq_set: bool,
    pub pending_irq_cleared: bool,
    pub vtimer_masked: bool,
    pub vtimer_unmasked: bool,
    pub vtimer_offset_set: bool,
    pub boundary_observed: bool,
    pub vcpu_destroyed: bool,
    pub vm_destroyed: bool,
    pub host: HvfHostCapabilities,
    pub vtimer_offset_value: u64,
    pub vm_create_status: Option<i32>,
    pub vcpu_create_status: Option<i32>,
    pub irq_set_status: Option<i32>,
    pub irq_get_after_set_status: Option<i32>,
    pub irq_pending_after_set: Option<bool>,
    pub irq_clear_status: Option<i32>,
    pub irq_get_after_clear_status: Option<i32>,
    pub irq_pending_after_clear: Option<bool>,
    pub vtimer_mask_set_status: Option<i32>,
    pub vtimer_mask_get_status: Option<i32>,
    pub vtimer_mask_after_set: Option<bool>,
    pub vtimer_unmask_status: Option<i32>,
    pub vtimer_unmask_get_status: Option<i32>,
    pub vtimer_mask_after_clear: Option<bool>,
    pub vtimer_offset_set_status: Option<i32>,
    pub vtimer_offset_get_status: Option<i32>,
    pub vtimer_offset_after_set: Option<u64>,
    pub vcpu_destroy_status: Option<i32>,
    pub vm_destroy_status: Option<i32>,
    pub blockers: Vec<String>,
}

impl HvfInterruptTimerProbe {
    pub fn render_text(&self) -> String {
        let mut output = String::new();
        output.push_str("HVF interrupt/timer probe\n");
        output.push_str("QEMU: not used\n");
        output.push_str("Apple VZ: not used\n");
        output.push_str("Guest execution: not entered\n");
        output.push_str(&format!("Host: {}\n", self.host.host));
        output.push_str(&format!("Host HVF available: {}\n", self.host.available));
        output.push_str(&format!("Allowed: {}\n", self.allowed));
        output.push_str(&format!("Attempted: {}\n", self.attempted));
        output.push_str(&format!("VM created: {}\n", self.vm_created));
        output.push_str(&format!("vCPU created: {}\n", self.vcpu_created));
        output.push_str(&format!("Pending IRQ set: {}\n", self.pending_irq_set));
        output.push_str(&format!(
            "Pending IRQ after set: {}\n",
            self.irq_pending_after_set
                .map_or_else(|| "not observed".to_string(), |value| value.to_string())
        ));
        output.push_str(&format!(
            "Pending IRQ cleared: {}\n",
            self.pending_irq_cleared
        ));
        output.push_str(&format!(
            "Pending IRQ after clear: {}\n",
            self.irq_pending_after_clear
                .map_or_else(|| "not observed".to_string(), |value| value.to_string())
        ));
        output.push_str(&format!("VTimer masked: {}\n", self.vtimer_masked));
        output.push_str(&format!(
            "VTimer mask after set: {}\n",
            self.vtimer_mask_after_set
                .map_or_else(|| "not observed".to_string(), |value| value.to_string())
        ));
        output.push_str(&format!("VTimer unmasked: {}\n", self.vtimer_unmasked));
        output.push_str(&format!(
            "VTimer mask after clear: {}\n",
            self.vtimer_mask_after_clear
                .map_or_else(|| "not observed".to_string(), |value| value.to_string())
        ));
        output.push_str(&format!("VTimer offset set: {}\n", self.vtimer_offset_set));
        output.push_str(&format!(
            "VTimer offset requested: {:#x}\n",
            self.vtimer_offset_value
        ));
        output.push_str(&format!(
            "VTimer offset after set: {}\n",
            self.vtimer_offset_after_set
                .map_or_else(|| "not observed".to_string(), |value| format!("{value:#x}"))
        ));
        output.push_str(&format!(
            "Interrupt/timer boundary observed: {}\n",
            self.boundary_observed
        ));
        output.push_str(&format!("vCPU destroyed: {}\n", self.vcpu_destroyed));
        output.push_str(&format!("VM destroyed: {}\n", self.vm_destroyed));
        output.push_str(&format!(
            "VM create status name: {}\n",
            render_optional_status_name(self.vm_create_status)
        ));
        output.push_str(&format!(
            "vCPU create status name: {}\n",
            render_optional_status_name(self.vcpu_create_status)
        ));
        output.push_str(&format!(
            "IRQ set status name: {}\n",
            render_optional_status_name(self.irq_set_status)
        ));
        output.push_str(&format!(
            "IRQ get after set status name: {}\n",
            render_optional_status_name(self.irq_get_after_set_status)
        ));
        output.push_str(&format!(
            "IRQ clear status name: {}\n",
            render_optional_status_name(self.irq_clear_status)
        ));
        output.push_str(&format!(
            "IRQ get after clear status name: {}\n",
            render_optional_status_name(self.irq_get_after_clear_status)
        ));
        output.push_str(&format!(
            "VTimer mask set status name: {}\n",
            render_optional_status_name(self.vtimer_mask_set_status)
        ));
        output.push_str(&format!(
            "VTimer mask get status name: {}\n",
            render_optional_status_name(self.vtimer_mask_get_status)
        ));
        output.push_str(&format!(
            "VTimer unmask status name: {}\n",
            render_optional_status_name(self.vtimer_unmask_status)
        ));
        output.push_str(&format!(
            "VTimer unmask get status name: {}\n",
            render_optional_status_name(self.vtimer_unmask_get_status)
        ));
        output.push_str(&format!(
            "VTimer offset set status name: {}\n",
            render_optional_status_name(self.vtimer_offset_set_status)
        ));
        output.push_str(&format!(
            "VTimer offset get status name: {}\n",
            render_optional_status_name(self.vtimer_offset_get_status)
        ));
        output.push_str(&format!(
            "vCPU destroy status name: {}\n",
            render_optional_status_name(self.vcpu_destroy_status)
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

pub fn probe_hvf_interrupt_timer(allow_probe: bool) -> HvfInterruptTimerProbe {
    let host = query_hvf_host_capabilities();
    platform::probe_hvf_interrupt_timer(allow_probe, host)
}
