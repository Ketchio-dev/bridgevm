//! Split out of run_loop_render.rs: report header, allocation/map and repair-request state.

use super::*;
use crate::*;

impl WindowsArmUefiFirmwareRunLoopProbe {
    pub(crate) fn render_setup_state(&self, output: &mut String) {
        output.push_str("Windows 11 Arm HVF UEFI firmware run-loop probe\n");
        output.push_str("QEMU: not used\n");
        output.push_str("Apple VZ: not used\n");
        output.push_str("Guest execution: bounded UEFI firmware exit classification loop\n");
        output.push_str("Windows boot: not claimed\n");
        output.push_str(&format!(
            "Device models: {}\n",
            WINDOWS_ARM_FIRMWARE_MMIO_DEVICE_MODELS
        ));
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
            "Guest RAM memory allocated: {}\n",
            self.guest_ram_memory_allocated
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
        output.push_str(&format!(
            "Low firmware alias mapped: {}\n",
            self.low_firmware_alias_mapped
        ));
        output.push_str(&format!(
            "Low vars alias mapped: {}\n",
            self.low_vars_alias_mapped
        ));
        output.push_str(&format!(
            "Guest RAM memory mapped: {}\n",
            self.guest_ram_memory_mapped
        ));
        output.push_str(&format!(
            "Platform DTB populated: {}\n",
            self.platform_dtb_populated
        ));
        output.push_str(&format!(
            "Diagnostic vector seed requested: {}\n",
            self.diagnostic_vector_seed_requested
        ));
        output.push_str(&format!(
            "Diagnostic vector populated: {}\n",
            self.diagnostic_vector_populated
        ));
        output.push_str(&format!(
            "Low vector diagnostic page repair requested: {}\n",
            self.low_vector_diagnostic_page_repair_requested
        ));
        output.push_str(&format!(
            "Low vector diagnostic page repaired: {}\n",
            self.low_vector_diagnostic_page_repaired
        ));
        output.push_str(&format!(
            "Low vector diagnostic page slot restored: {}\n",
            self.low_vector_diagnostic_page_slot_restored
        ));
        output.push_str(&format!(
            "Low vector diagnostic page restore before ERET requested: {}\n",
            self.low_vector_diagnostic_page_restore_before_eret_requested
        ));
        output.push_str(&format!(
            "Low vector diagnostic page restore before ERET attempted: {}\n",
            self.low_vector_diagnostic_page_restore_before_eret_attempted
        ));
        output.push_str(&format!(
            "Low vector diagnostic page entry IPA: {}\n",
            render_optional_u64(self.low_vector_diagnostic_page_entry_ipa)
        ));
        output.push_str(&format!(
            "Low vector diagnostic page previous descriptor: {}\n",
            render_optional_u64(self.low_vector_diagnostic_page_previous_descriptor)
        ));
        output.push_str(&format!(
            "Low vector diagnostic page descriptor: {}\n",
            render_optional_u64(self.low_vector_diagnostic_page_descriptor)
        ));
        output.push_str(&format!(
            "Low vector diagnostic page repeated fault observed: {}\n",
            self.low_vector_diagnostic_page_repeated_fault_observed
        ));
        output.push_str(&format!(
            "Low vector recommended-vector remap requested: {}\n",
            self.low_vector_recommended_vector_remap_requested
        ));
        output.push_str(&format!(
            "Low vector recommended-vector remap attempted: {}\n",
            self.low_vector_recommended_vector_remap_attempted
        ));
        output.push_str(&format!(
            "Low vector recommended-vector remap succeeded: {}\n",
            self.low_vector_recommended_vector_remap_succeeded
        ));
        output.push_str(&format!(
            "Low vector recommended-vector remap target PA: {}\n",
            render_optional_u64(self.low_vector_recommended_vector_remap_target_physical_address)
        ));
        output.push_str(&format!(
            "Low vector recommended-vector remap descriptor: {}\n",
            render_optional_u64(self.low_vector_recommended_vector_remap_descriptor)
        ));
        output.push_str(&format!(
            "Continue after low-vector repair requested: {}\n",
            self.low_vector_post_repair_continue_requested
        ));
        output.push_str(&format!(
            "Continue after low-vector repair attempted: {}\n",
            self.low_vector_post_repair_continue_attempted
        ));
        output.push_str(&format!(
            "Stop at first post-repair device boundary requested: {}\n",
            self.stop_at_first_post_repair_device_boundary_requested
        ));
        output.push_str(&format!(
            "Post-repair unsupported exit observed: {}\n",
            self.low_vector_post_repair_unsupported_exit_observed
        ));
        output.push_str(&format!(
            "Post-repair unsupported exit reason name: {}\n",
            render_optional_exit_reason_name(self.low_vector_post_repair_unsupported_exit_reason)
        ));
        output.push_str(&format!(
            "Post-repair unsupported exit classification: {}\n",
            self.low_vector_post_repair_unsupported_exit_diagnosis
        ));
        let post_repair_first_exit = self.low_vector_post_repair_first_exit_telemetry();
        let post_repair_first_exit_context =
            low_vector_post_repair_context_exit(&self.exits, post_repair_first_exit.index);
        append_low_vector_post_repair_exit_telemetry(
            output,
            "Post-repair first exit",
            &post_repair_first_exit,
            "Post-repair first interaction kind",
            post_repair_first_exit_context,
        );
        let post_repair_first_device_interaction =
            self.low_vector_post_repair_first_device_interaction_telemetry();
        let post_repair_first_device_interaction_context = low_vector_post_repair_context_exit(
            &self.exits,
            post_repair_first_device_interaction.index,
        );
        append_low_vector_post_repair_exit_telemetry(
            output,
            "Post-repair first device interaction",
            &post_repair_first_device_interaction,
            "Post-repair first device interaction kind",
            post_repair_first_device_interaction_context,
        );
        let post_repair_first_unhandled_access =
            self.low_vector_post_repair_first_unhandled_access_telemetry();
        append_low_vector_post_repair_unhandled_access_telemetry(
            output,
            "Post-repair first unhandled access",
            &post_repair_first_unhandled_access,
        );
        output.push_str(&format!(
            "Low vector diagnostic page resume attempted: {}\n",
            self.low_vector_diagnostic_page_resume_attempted
        ));
        output.push_str(&format!(
            "Low vector diagnostic page resume armed: {}\n",
            self.low_vector_diagnostic_page_resume_armed
        ));
        output.push_str(&format!(
            "Low vector diagnostic page resume original PC: {}\n",
            render_optional_u64(self.low_vector_diagnostic_page_resume_original_pc)
        ));
        output.push_str(&format!(
            "Low vector diagnostic page resume original ELR_EL1: {}\n",
            render_optional_u64(self.low_vector_diagnostic_page_resume_original_elr_el1)
        ));
        output.push_str(&format!(
            "Low vector diagnostic page resume original ESR_EL1: {}\n",
            render_optional_u64(self.low_vector_diagnostic_page_resume_original_esr_el1)
        ));
        output.push_str(&format!(
            "Low vector diagnostic page resume original FAR_EL1: {}\n",
            render_optional_u64(self.low_vector_diagnostic_page_resume_original_far_el1)
        ));
        output.push_str(&format!(
            "Low vector diagnostic page resume original SPSR_EL1: {}\n",
            render_optional_u64(self.low_vector_diagnostic_page_resume_original_spsr_el1)
        ));
        output.push_str(&format!(
            "Low vector diagnostic page original slot bytes: {}\n",
            self.low_vector_diagnostic_page_original_slot_bytes
                .as_ref()
                .map_or_else(
                    || "not observed".to_string(),
                    |bytes| render_hex_bytes(bytes)
                )
        ));
        let original_sync_instruction = self
            .low_vector_diagnostic_page_original_slot_bytes
            .and_then(|bytes| Some(u32::from_le_bytes(bytes[0..4].try_into().ok()?)));
        output.push_str(&format!(
            "Low vector diagnostic page original sync instruction: {}\n",
            render_optional_instruction_word(original_sync_instruction)
        ));
        output.push_str(&format!(
            "Low vector diagnostic page original sync hint: {}\n",
            original_sync_instruction
                .map(aarch64_instruction_hint)
                .unwrap_or("not observed")
        ));
        output.push_str(&format!(
            "Low vector diagnostic page resume target instruction before ERET: {}\n",
            render_optional_instruction_word(
                self.low_vector_diagnostic_page_resume_target_instruction_before_eret,
            )
        ));
        output.push_str(&format!(
            "Low vector diagnostic page resume target hint before ERET: {}\n",
            self.low_vector_diagnostic_page_resume_target_instruction_before_eret
                .map(aarch64_instruction_hint)
                .unwrap_or("not observed")
        ));
        output.push_str(&format!(
            "Low vector diagnostic page resume target stage-1 descriptor before ERET: {}\n",
            render_optional_u64(
                self.low_vector_diagnostic_page_resume_target_stage1_leaf_descriptor_before_eret,
            )
        ));
        output.push_str(&format!(
            "Low vector diagnostic page resume target stage-1 kind before ERET: {}\n",
            self.low_vector_diagnostic_page_resume_target_stage1_leaf_kind_before_eret
        ));
        output.push_str(&format!(
            "Low vector diagnostic page resume target is installed diagnostic HVC before ERET: {}\n",
            self.low_vector_diagnostic_page_resume_target_is_installed_diagnostic_hvc_before_eret
        ));
        output.push_str(&format!("vCPU created: {}\n", self.vcpu_created));
        output.push_str(&format!("PC set: {}\n", self.pc_set));
        output.push_str(&format!("X0 DTB IPA set: {}\n", self.x0_dtb_ipa_set));
        output.push_str(&format!("CPSR set: {}\n", self.cpsr_set));
        output.push_str(&format!("SP_EL1 set: {}\n", self.sp_el1_set));
        output.push_str(&format!(
            "Diagnostic vector VBAR_EL1 set: {}\n",
            self.diagnostic_vector_vbar_el1_set
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR requested: {}\n",
            self.recommended_vector_base_vbar_requested
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR attempted: {}\n",
            self.recommended_vector_base_vbar_attempted
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR set: {}\n",
            self.recommended_vector_base_vbar_set
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR diagnostic vector populated: {}\n",
            self.recommended_vector_base_vbar_diagnostic_vector_populated
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR resume requested: {}\n",
            self.recommended_vector_base_vbar_resume_requested
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR resume attempted: {}\n",
            self.recommended_vector_base_vbar_resume_attempted
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR resume armed: {}\n",
            self.recommended_vector_base_vbar_resume_armed
        ));
        output.push_str(&format!(
            "Interrupt/timer wiring requested: {}\n",
            self.interrupt_timer_wiring_requested
        ));
        output.push_str(&format!(
            "Interrupt/timer initialized: {}\n",
            self.interrupt_timer_initialized
        ));
        output.push_str(&format!(
            "Run loop attempted: {}\n",
            self.run_loop_attempted
        ));
        output.push_str(&format!(
            "Firmware progress observed: {}\n",
            self.firmware_progress_observed
        ));
        output.push_str(&format!(
            "Unsupported exit observed: {}\n",
            self.unsupported_exit_observed
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
            "Guest RAM memory unmapped: {}\n",
            self.guest_ram_memory_unmapped
        ));
        output.push_str(&format!(
            "Firmware memory deallocated: {}\n",
            self.firmware_memory_deallocated
        ));
        output.push_str(&format!(
            "Vars memory deallocated: {}\n",
            self.vars_memory_deallocated
        ));
        output.push_str(&format!(
            "Guest RAM memory deallocated: {}\n",
            self.guest_ram_memory_deallocated
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
        output.push_str(&format!(
            "Low firmware alias IPA: {:#x}\n",
            self.low_firmware_alias_ipa
        ));
        output.push_str(&format!(
            "Low vars alias IPA: {:#x}\n",
            self.low_vars_alias_ipa
        ));
        output.push_str(&format!("Guest RAM IPA: {:#x}\n", self.guest_ram_ipa));
        output.push_str(&format!("Platform DTB IPA: {:#x}\n", self.platform_dtb_ipa));
        output.push_str(&format!(
            "Platform DTB guest RAM offset: {:#x}\n",
            self.platform_dtb_guest_ram_offset
        ));
        output.push_str(&format!("SP_EL1 seed IPA: {:#x}\n", self.sp_el1_seed_ipa));
        output.push_str(&format!(
            "Diagnostic vector location: {}\n",
            self.diagnostic_vector_location
        ));
        output.push_str(&format!(
            "Diagnostic vector IPA: {:#x}\n",
            self.diagnostic_vector_ipa
        ));
        output.push_str(&format!(
            "Diagnostic vector bytes: {:#x}\n",
            self.diagnostic_vector_bytes
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR source exit: {}\n",
            render_optional_intid(self.recommended_vector_base_vbar_source_exit_index)
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR target: {}\n",
            render_optional_u64(self.recommended_vector_base_vbar_target)
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR target PA: {}\n",
            render_optional_u64(self.recommended_vector_base_vbar_target_physical_address)
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR reason: {}\n",
            self.recommended_vector_base_vbar_reason
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR current EL/SPx sync instruction: {}\n",
            render_optional_instruction_word(
                self.recommended_vector_base_vbar_current_el_spx_sync_instruction_word,
            )
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR current EL/SPx sync hint: {}\n",
            self.recommended_vector_base_vbar_current_el_spx_sync_instruction_hint
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR follow-up exit observed: {}\n",
            self.recommended_vector_base_vbar_followup_exit_observed
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR follow-up exit: {}\n",
            render_optional_intid(self.recommended_vector_base_vbar_followup_exit_index)
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR follow-up exit reason name: {}\n",
            render_optional_exit_reason_name(
                self.recommended_vector_base_vbar_followup_exit_reason
            )
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR follow-up classification: {}\n",
            self.recommended_vector_base_vbar_followup_exit_diagnosis
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR follow-up PC: {}\n",
            render_optional_u64(self.recommended_vector_base_vbar_followup_pc)
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR follow-up VBAR_EL1: {}\n",
            render_optional_u64(self.recommended_vector_base_vbar_followup_vbar_el1)
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR follow-up target still set: {}\n",
            self.recommended_vector_base_vbar_followup_target_still_set
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR resume original PC: {}\n",
            render_optional_u64(self.recommended_vector_base_vbar_resume_original_pc)
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR resume original ELR_EL1: {}\n",
            render_optional_u64(self.recommended_vector_base_vbar_resume_original_elr_el1)
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR resume original ESR_EL1: {}\n",
            render_optional_u64(self.recommended_vector_base_vbar_resume_original_esr_el1)
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR resume original FAR_EL1: {}\n",
            render_optional_u64(self.recommended_vector_base_vbar_resume_original_far_el1)
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR resume original SPSR_EL1: {}\n",
            render_optional_u64(self.recommended_vector_base_vbar_resume_original_spsr_el1)
        ));
        output.push_str(&format!("Slot bytes: {:#x}\n", self.slot_bytes));
        output.push_str(&format!("Guest RAM bytes: {:#x}\n", self.guest_ram_bytes));
        output.push_str(&format!(
            "Platform DTB bytes: {:#x}\n",
            self.platform_dtb_bytes
        ));
        output.push_str(&format!(
            "Platform DTB magic: {:#x}\n",
            self.platform_dtb_magic
        ));
        output.push_str(&format!(
            "Platform DTB magic verified: {}\n",
            self.platform_dtb_magic_verified
        ));
        output.push_str(&format!("Requested exits: {}\n", self.requested_exits));
        output.push_str(&format!("Observed exits: {}\n", self.observed_exits));
        output.push_str(&format!(
            "Watchdog timeout ms: {}\n",
            self.watchdog_timeout_ms
        ));
        output.push_str(&format!(
            "VTimer offset value: {}\n",
            render_optional_u64(self.vtimer_offset_value)
        ));
        output.push_str(&format!(
            "CNTV_CVAL_EL0 value: {}\n",
            render_optional_u64(self.cntv_cval_value)
        ));
        output.push_str(&format!(
            "CNTV_CTL_EL0 value: {}\n",
            render_optional_u64(self.cntv_ctl_value)
        ));
        output.push_str(&format!("VTimer exit count: {}\n", self.vtimer_exit_count));
        output.push_str(&format!(
            "Pending IRQ injected count: {}\n",
            self.pending_irq_injected_count
        ));
        output.push_str(&format!(
            "Device IRQ line asserted count: {}\n",
            self.device_irq_injected_count
        ));
        output.push_str(&format!(
            "Device IRQ line deasserted count: {}\n",
            self.device_irq_cleared_count
        ));
        output.push_str(&format!(
            "Handled MMIO read count: {}\n",
            self.handled_mmio_read_count
        ));
        output.push_str(&format!(
            "Handled MMIO write count: {}\n",
            self.handled_mmio_write_count
        ));
        output.push_str(&format!(
            "Handled PL011 MMIO count: {}\n",
            self.handled_pl011_mmio_count
        ));
        output.push_str(&format!(
            "Handled PL031 MMIO count: {}\n",
            self.handled_pl031_mmio_count
        ));
        output.push_str(&format!(
            "Handled GICD MMIO count: {}\n",
            self.handled_gicd_mmio_count
        ));
        output.push_str(&format!(
            "Handled GICR MMIO count: {}\n",
            self.handled_gicr_mmio_count
        ));
        output.push_str(&format!(
            "Handled VirtIO installer ISO MMIO count: {}\n",
            self.handled_virtio_installer_iso_mmio_count
        ));
        output.push_str(&format!(
            "Handled VirtIO target disk MMIO count: {}\n",
            self.handled_virtio_target_disk_mmio_count
        ));
        output.push_str(&format!(
            "VirtIO queue_notify count: {}\n",
            self.virtio_queue_notify_count
        ));
        output.push_str(&format!(
            "VirtIO request completion count: {}\n",
            self.virtio_request_completion_count
        ));
        output.push_str(&format!(
            "Handled ICC read count: {}\n",
            self.handled_icc_read_count
        ));
        output.push_str(&format!(
            "Handled ICC write count: {}\n",
            self.handled_icc_write_count
        ));
        output.push_str(&format!(
            "Handled ICC_IAR1 read count: {}\n",
            self.handled_icc_iar1_read_count
        ));
        output.push_str(&format!(
            "Handled ICC_EOIR1 write count: {}\n",
            self.handled_icc_eoir1_write_count
        ));
        output.push_str(&format!(
            "Handled ICC_DIR write count: {}\n",
            self.handled_icc_dir_write_count
        ));
        output.push_str(&format!(
            "Last ICC_IAR1 INTID: {}\n",
            render_optional_intid(self.last_icc_iar1_intid)
        ));
        output.push_str(&format!(
            "Last ICC_EOIR1 INTID: {}\n",
            render_optional_intid(self.last_icc_eoir1_intid)
        ));
        output.push_str(&format!(
            "Last ICC_DIR INTID: {}\n",
            render_optional_intid(self.last_icc_dir_intid)
        ));
        output.push_str(&format!(
            "Firmware source bytes: {}\n",
            render_optional_u64(self.firmware_source_bytes)
        ));
        output.push_str(&format!(
            "Vars source bytes: {}\n",
            render_optional_u64(self.vars_source_bytes)
        ));
        output.push_str(&format!(
            "Installer ISO path: {}\n",
            self.installer_iso_path.as_ref().map_or_else(
                || "not provided".to_string(),
                |path| path.display().to_string()
            )
        ));
        output.push_str(&format!(
            "Writable target disk path: {}\n",
            self.writable_target_disk_path.as_ref().map_or_else(
                || "not provided".to_string(),
                |path| path.display().to_string()
            )
        ));
        output.push_str("Firmware block devices:\n");
    }
}
