//! Split out of run_loop_render.rs: block devices, map flags and status-name fields.

use super::*;

impl WindowsArmUefiFirmwareRunLoopProbe {
    pub(crate) fn render_status_fields(&self, output: &mut String) {
        for device in &self.block_devices {
            output.push_str(&format!(
                "- role={}, label={}, node={}, base={:#x}, bytes={:#x}, read_only={}, backing_kind={}, backing_path={}, device_features={:#x}, capacity_sectors={:#x}\n",
                device.role,
                device.label,
                device.node_name,
                device.base_ipa,
                device.bytes,
                device.read_only,
                device.backing_kind,
                device
                    .backing_path
                    .as_ref()
                    .map_or_else(|| "not provided".to_string(), |path| path.display().to_string()),
                device.device_features,
                device.capacity_sectors,
            ));
        }
        output.push_str(&format!(
            "Firmware map flags: {}\n",
            self.firmware_map_flags
        ));
        output.push_str(&format!("Vars map flags: {}\n", self.vars_map_flags));
        output.push_str(&format!(
            "Low firmware alias map flags: {}\n",
            self.low_firmware_alias_map_flags
        ));
        output.push_str(&format!(
            "Low vars alias map flags: {}\n",
            self.low_vars_alias_map_flags
        ));
        output.push_str(&format!(
            "Guest RAM map flags: {}\n",
            self.guest_ram_map_flags
        ));
        output.push_str(&format!(
            "Low pflash alias requested: {}\n",
            self.low_pflash_alias_requested
        ));
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
            "Guest RAM allocate status name: {}\n",
            render_optional_status_name(self.guest_ram_allocate_status)
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
            "Low firmware alias map status name: {}\n",
            render_optional_status_name(self.low_firmware_alias_map_status)
        ));
        output.push_str(&format!(
            "Low vars alias map status name: {}\n",
            render_optional_status_name(self.low_vars_alias_map_status)
        ));
        output.push_str(&format!(
            "Guest RAM map status name: {}\n",
            render_optional_status_name(self.guest_ram_map_status)
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
            "X0 DTB IPA set status name: {}\n",
            render_optional_status_name(self.x0_dtb_ipa_set_status)
        ));
        output.push_str(&format!(
            "CPSR set status name: {}\n",
            render_optional_status_name(self.cpsr_set_status)
        ));
        output.push_str(&format!(
            "SP_EL1 set status name: {}\n",
            render_optional_status_name(self.sp_el1_set_status)
        ));
        output.push_str(&format!(
            "Diagnostic vector VBAR_EL1 set status name: {}\n",
            render_optional_status_name(self.diagnostic_vector_vbar_el1_set_status)
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR set status name: {}\n",
            render_optional_status_name(self.recommended_vector_base_vbar_set_status)
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR resume ELR_EL1 set status name: {}\n",
            render_optional_status_name(
                self.recommended_vector_base_vbar_resume_elr_el1_set_status,
            )
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR resume VBAR_EL1 set status name: {}\n",
            render_optional_status_name(
                self.recommended_vector_base_vbar_resume_vbar_el1_set_status,
            )
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR resume SPSR_EL1 set status name: {}\n",
            render_optional_status_name(
                self.recommended_vector_base_vbar_resume_spsr_el1_set_status,
            )
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR resume PC set status name: {}\n",
            render_optional_status_name(self.recommended_vector_base_vbar_resume_pc_set_status)
        ));
        output.push_str(&format!(
            "Low vector diagnostic page resume ELR_EL1 set status name: {}\n",
            render_optional_status_name(self.low_vector_diagnostic_page_resume_elr_el1_set_status)
        ));
        output.push_str(&format!(
            "Low vector diagnostic page resume SPSR_EL1 set status name: {}\n",
            render_optional_status_name(self.low_vector_diagnostic_page_resume_spsr_el1_set_status)
        ));
        output.push_str(&format!(
            "Low vector diagnostic page resume CPSR set status name: {}\n",
            render_optional_status_name(self.low_vector_diagnostic_page_resume_cpsr_set_status)
        ));
        output.push_str(&format!(
            "Low vector diagnostic page resume PC set status name: {}\n",
            render_optional_status_name(self.low_vector_diagnostic_page_resume_pc_set_status)
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
            "VTimer initial unmask status name: {}\n",
            render_optional_status_name(self.vtimer_initial_unmask_status)
        ));
        output.push_str(&format!(
            "Last pending IRQ set status name: {}\n",
            render_optional_status_name(self.last_pending_irq_set_status)
        ));
        output.push_str(&format!(
            "Last device IRQ line assert status name: {}\n",
            render_optional_status_name(self.last_device_irq_set_status)
        ));
        output.push_str(&format!(
            "Last device IRQ line deassert status name: {}\n",
            render_optional_status_name(self.last_device_irq_clear_status)
        ));
        output.push_str(&format!(
            "Last VTimer unmask status name: {}\n",
            render_optional_status_name(self.last_vtimer_unmask_status)
        ));
        output.push_str(&format!(
            "Final PC status name: {}\n",
            render_optional_status_name(self.final_pc_status)
        ));
        output.push_str(&format!(
            "Final PC: {}\n",
            render_optional_u64(self.final_pc)
        ));
        output.push_str("Run-loop exits:\n");
    }
}
