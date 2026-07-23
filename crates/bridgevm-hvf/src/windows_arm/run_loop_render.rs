//! `WindowsArmUefiFirmwareRunLoopProbe::render_text`.
//!
//! NOTE: render_text is a single ~1000-line formatter. Splitting it further
//! requires decomposing the function itself, which is a behaviour-sensitive
//! change rather than a relocation, so it is deliberately left intact here.

use super::*;
use crate::*;

impl WindowsArmUefiFirmwareRunLoopProbe {
    pub(crate) fn low_vector_post_repair_first_exit_telemetry(
        &self,
    ) -> LowVectorPostRepairExitTelemetry {
        LowVectorPostRepairExitTelemetry {
            observed: self.low_vector_post_repair_first_exit_observed,
            index: self.low_vector_post_repair_first_exit_index,
            reason: self.low_vector_post_repair_first_exit_reason,
            diagnosis: self.low_vector_post_repair_first_exit_diagnosis,
            pc: self.low_vector_post_repair_first_exit_pc,
            interaction_kind: self.low_vector_post_repair_first_interaction_kind,
            access: LowVectorPostRepairAccessTelemetry {
                kind: self.low_vector_post_repair_first_exit_access_kind,
                direction: self.low_vector_post_repair_first_exit_access_direction,
                address: self.low_vector_post_repair_first_exit_access_address,
                sysreg: self.low_vector_post_repair_first_exit_access_sysreg,
                syndrome: self.low_vector_post_repair_first_exit_access_syndrome,
            },
        }
    }

    pub(crate) fn low_vector_post_repair_first_device_interaction_telemetry(
        &self,
    ) -> LowVectorPostRepairExitTelemetry {
        LowVectorPostRepairExitTelemetry {
            observed: self.low_vector_post_repair_first_device_interaction_observed,
            index: self.low_vector_post_repair_first_device_interaction_index,
            reason: self.low_vector_post_repair_first_device_interaction_reason,
            diagnosis: self.low_vector_post_repair_first_device_interaction_diagnosis,
            pc: self.low_vector_post_repair_first_device_interaction_pc,
            interaction_kind: self.low_vector_post_repair_first_device_interaction_kind,
            access: LowVectorPostRepairAccessTelemetry {
                kind: self.low_vector_post_repair_first_device_interaction_access_kind,
                direction: self.low_vector_post_repair_first_device_interaction_access_direction,
                address: self.low_vector_post_repair_first_device_interaction_access_address,
                sysreg: self.low_vector_post_repair_first_device_interaction_access_sysreg,
                syndrome: self.low_vector_post_repair_first_device_interaction_access_syndrome,
            },
        }
    }

    pub(crate) fn low_vector_post_repair_first_unhandled_access_telemetry(
        &self,
    ) -> LowVectorPostRepairUnhandledAccessTelemetry {
        LowVectorPostRepairUnhandledAccessTelemetry {
            observed: self.low_vector_post_repair_first_unhandled_access_observed,
            index: self.low_vector_post_repair_first_unhandled_access_index,
            reason: self.low_vector_post_repair_first_unhandled_access_reason,
            diagnosis: self.low_vector_post_repair_first_unhandled_access_diagnosis,
            pc: self.low_vector_post_repair_first_unhandled_access_pc,
            syndrome: self.low_vector_post_repair_first_unhandled_access_syndrome,
            kind: self.low_vector_post_repair_first_unhandled_access_kind,
            access: self.low_vector_post_repair_first_unhandled_access_direction,
            register: self.low_vector_post_repair_first_unhandled_access_register,
            value: self.low_vector_post_repair_first_unhandled_access_value,
            handler_result: self.low_vector_post_repair_first_unhandled_access_handler_result,
            mmio_ipa: self.low_vector_post_repair_first_unhandled_access_mmio_ipa,
            mmio_width: self.low_vector_post_repair_first_unhandled_access_mmio_width,
            mmio_device_kind: self.low_vector_post_repair_first_unhandled_access_mmio_device_kind,
            sysreg: self.low_vector_post_repair_first_unhandled_access_sysreg,
            sysreg_name: self.low_vector_post_repair_first_unhandled_access_sysreg_name,
            sysreg_op0: self.low_vector_post_repair_first_unhandled_access_sysreg_op0,
            sysreg_op1: self.low_vector_post_repair_first_unhandled_access_sysreg_op1,
            sysreg_crn: self.low_vector_post_repair_first_unhandled_access_sysreg_crn,
            sysreg_crm: self.low_vector_post_repair_first_unhandled_access_sysreg_crm,
            sysreg_op2: self.low_vector_post_repair_first_unhandled_access_sysreg_op2,
        }
    }

    pub fn render_text(&self) -> String {
        let mut output = String::new();
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
            &mut output,
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
            &mut output,
            "Post-repair first device interaction",
            &post_repair_first_device_interaction,
            "Post-repair first device interaction kind",
            post_repair_first_device_interaction_context,
        );
        let post_repair_first_unhandled_access =
            self.low_vector_post_repair_first_unhandled_access_telemetry();
        append_low_vector_post_repair_unhandled_access_telemetry(
            &mut output,
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
        if self.exits.is_empty() {
            output.push_str("- none\n");
        } else {
            for exit in &self.exits {
                output.push_str(&format!(
                    "- Exit {}: run={}, reason={}, exception_class={}, exception_class_name={}, syndrome={}, abort_iss={}, abort_fault_status={}, abort_fault_status_name={}, va={}, va_region={}, pa={}, pa_region={}, pc={}, instruction={}, instruction_hint={}, pc_stage1_leaf_level={}, pc_stage1_leaf_descriptor={}, pc_stage1_leaf_kind={}, pc_stage1_leaf_pxn={}, pc_stage1_leaf_uxn={}, x0={}, x1={}, x2={}, x3={}, x4={}, cpsr={}, vbar_el1={}, elr_el1={}, esr_el1={}, esr_el1_class_name={}, esr_el1_fault_status_name={}, far_el1={}, spsr_el1={}, sctlr_el1={}, sctlr_el1_mmu_enabled={}, tcr_el1={}, ttbr0_el1={}, ttbr1_el1={}, mair_el1={}, sp_el1={}, diagnosis={}, watchdog={}, vtimer_auto_mask={}, vtimer_auto_mask_get={}, vtimer_rearm_cval={}, vtimer_rearm_cval_set={}, vtimer_ppi_pending_recorded={}, vtimer_irq_line_assertable={}, vtimer_gic_group1_enabled={}, vtimer_gic_priority_mask={}, vtimer_gic_running_priority={}, vtimer_gic_priority_threshold={}, vtimer_gic_pending_intid={}, vtimer_pending_irq={}, vtimer_unmask={}, handled={}\n",
                    exit.index,
                    render_optional_status_name(exit.run_status),
                    render_optional_exit_reason_name(exit.exit_reason),
                    render_optional_u64(exit.exit_exception_class),
                    render_optional_exception_class_name(exit.exit_exception_class),
                    render_optional_u64(exit.exit_syndrome),
                    render_optional_abort_iss(exit.exit_syndrome),
                    render_optional_abort_fault_status(exit.exit_syndrome),
                    render_optional_abort_fault_status_name(exit.exit_syndrome),
                    render_optional_u64(exit.exit_virtual_address),
                    windows_arm_guest_region_name(exit.exit_virtual_address, self.guest_ram_bytes),
                    render_optional_u64(exit.exit_physical_address),
                    windows_arm_guest_region_name(exit.exit_physical_address, self.guest_ram_bytes),
                    render_optional_u64(exit.pc_after_exit),
                    render_optional_instruction_word(exit.instruction_word_after_exit),
                    exit.instruction_hint_after_exit,
                    render_optional_u8(exit.pc_stage1_leaf_level_after_exit),
                    render_optional_u64(exit.pc_stage1_leaf_descriptor_after_exit),
                    exit.pc_stage1_leaf_descriptor_kind_after_exit,
                    render_optional_bool(exit.pc_stage1_leaf_pxn_after_exit),
                    render_optional_bool(exit.pc_stage1_leaf_uxn_after_exit),
                    render_optional_u64(exit.x0_after_exit),
                    render_optional_u64(exit.x1_after_exit),
                    render_optional_u64(exit.x2_after_exit),
                    render_optional_u64(exit.x3_after_exit),
                    render_optional_u64(exit.x4_after_exit),
                    render_optional_u64(exit.cpsr_after_exit),
                    render_optional_u64(exit.vbar_el1_after_exit),
                    render_optional_u64(exit.elr_el1_after_exit),
                    render_optional_u64(exit.esr_el1_after_exit),
                    render_optional_esr_exception_class_name(exit.esr_el1_after_exit),
                    render_optional_abort_fault_status_name(exit.esr_el1_after_exit),
                    render_optional_u64(exit.far_el1_after_exit),
                    render_optional_u64(exit.spsr_el1_after_exit),
                    render_optional_u64(exit.sctlr_el1_after_exit),
                    render_optional_sctlr_mmu_enabled(exit.sctlr_el1_after_exit),
                    render_optional_u64(exit.tcr_el1_after_exit),
                    render_optional_u64(exit.ttbr0_el1_after_exit),
                    render_optional_u64(exit.ttbr1_el1_after_exit),
                    render_optional_u64(exit.mair_el1_after_exit),
                    render_optional_u64(exit.sp_el1_after_exit),
                    windows_arm_firmware_run_loop_exit_diagnosis(exit),
                    render_optional_status_name(exit.watchdog_cancel_status),
                    render_optional_bool(exit.vtimer_auto_mask_after_exit),
                    render_optional_status_name(exit.vtimer_auto_mask_get_status),
                    render_optional_u64(exit.vtimer_rearm_cval_value),
                    render_optional_status_name(exit.vtimer_rearm_cval_set_status),
                    render_optional_bool(exit.vtimer_ppi_pending_recorded),
                    render_optional_bool(exit.vtimer_irq_line_assertable),
                    render_optional_bool(exit.vtimer_gic_group1_enabled),
                    render_optional_u8(exit.vtimer_gic_priority_mask),
                    render_optional_u8(exit.vtimer_gic_running_priority),
                    render_optional_u8(exit.vtimer_gic_priority_threshold),
                    render_optional_gic_intid(exit.vtimer_gic_pending_intid),
                    render_optional_status_name(exit.vtimer_pending_irq_set_status),
                    render_optional_status_name(exit.vtimer_unmask_status),
                    exit.handled
                ));
                if exit.stage1_descriptor_samples_after_exit.is_empty() {
                    output.push_str("  Stage-1 descriptor samples: none\n");
                } else {
                    output.push_str("  Stage-1 descriptor samples:\n");
                    for sample in &exit.stage1_descriptor_samples_after_exit {
                        output.push_str(&format!(
                            "  - label={}, va={:#x}, region={}, level={}, descriptor={}, kind={}, output={}, attr_index={}, ap={}, sh={}, af={}, pxn={}, uxn={}\n",
                            sample.label,
                            sample.virtual_address,
                            sample.region,
                            render_optional_u8(sample.level),
                            render_optional_u64(sample.descriptor),
                            sample.descriptor_kind,
                            render_optional_u64(sample.output_address),
                            render_optional_u8(sample.attr_index),
                            render_optional_u8(sample.access_permissions),
                            render_optional_u8(sample.shareability),
                            render_optional_bool(sample.access_flag),
                            render_optional_bool(sample.pxn),
                            render_optional_bool(sample.uxn),
                        ));
                    }
                }
                if exit.stage1_walk_entries_after_exit.is_empty() {
                    output.push_str("  Stage-1 walk entries: none\n");
                } else {
                    output.push_str("  Stage-1 walk entries:\n");
                    for entry in &exit.stage1_walk_entries_after_exit {
                        output.push_str(&format!(
                            "  - label={}, va={:#x}, region={}, level={}, table_ipa={:#x}, index={:#x}, entry_ipa={:#x}, descriptor={}, kind={}, next_table={}, output={}, attr_index={}, ap={}, sh={}, af={}, pxn={}, uxn={}\n",
                            entry.label,
                            entry.virtual_address,
                            entry.region,
                            entry.level,
                            entry.table_ipa,
                            entry.index,
                            entry.entry_ipa,
                            render_optional_u64(entry.descriptor),
                            entry.descriptor_kind,
                            render_optional_u64(entry.next_table_ipa),
                            render_optional_u64(entry.output_address),
                            render_optional_u8(entry.attr_index),
                            render_optional_u8(entry.access_permissions),
                            render_optional_u8(entry.shareability),
                            render_optional_bool(entry.access_flag),
                            render_optional_bool(entry.pxn),
                            render_optional_bool(entry.uxn),
                        ));
                    }
                }
                if exit.stage1_executable_candidates_after_exit.is_empty() {
                    output.push_str("  Stage-1 EL1-executable leaf candidates: none\n");
                } else {
                    output.push_str("  Stage-1 EL1-executable leaf candidates:\n");
                    for candidate in &exit.stage1_executable_candidates_after_exit {
                        output.push_str(&format!(
                            "  - va={:#x}, region={}, level={}, descriptor={:#x}, kind={}, output={}, span={}, vector_sync_va={}, vector_sync_pa={}, vector_sync_instruction={}, vector_sync_hint={}, vector_base_scan_scanned={}, vector_base_scan_suppressed={}, vector_base_scan_limit_reached={}, attr_index={}, ap={}, sh={}, af={}, pxn={}, uxn={}\n",
                            candidate.virtual_address,
                            candidate.region,
                            candidate.level,
                            candidate.descriptor,
                            candidate.descriptor_kind,
                            render_optional_u64(candidate.output_address),
                            render_optional_u64(candidate.span_bytes),
                            render_optional_u64(candidate.vector_sync_virtual_address),
                            render_optional_u64(candidate.vector_sync_physical_address),
                            render_optional_instruction_word(candidate.vector_sync_instruction_word),
                            candidate.vector_sync_instruction_hint,
                            candidate.vector_base_scan_scanned_count,
                            candidate.vector_base_scan_suppressed_count,
                            candidate.vector_base_scan_limit_reached,
                            candidate.attr_index,
                            candidate.access_permissions,
                            candidate.shareability,
                            candidate.access_flag,
                            candidate.pxn,
                            candidate.uxn,
                        ));
                        if let Some(recommendation) = &candidate.recommended_vector_base_candidate {
                            output.push_str(&format!(
                                "    Recommended vector base: base_va={:#x}, base_pa={}, current_el_spx_sync={}, current_el_spx_hint={}, reason={}\n",
                                recommendation.base_virtual_address,
                                render_optional_u64(recommendation.base_physical_address),
                                render_optional_instruction_word(
                                    recommendation.current_el_spx_sync_instruction_word,
                                ),
                                recommendation.current_el_spx_sync_instruction_hint,
                                recommendation.reason,
                            ));
                        } else {
                            output.push_str("    Recommended vector base: none\n");
                        }
                        if candidate.vector_base_candidates.is_empty() {
                            output.push_str("    Vector base candidates: none\n");
                        } else {
                            output.push_str("    Vector base candidates:\n");
                            for vector_candidate in &candidate.vector_base_candidates {
                                output.push_str(&format!(
                                    "    - base_va={:#x}, base_pa={}, current_el_sp0_sync={}, current_el_spx_sync={}, current_el_spx_hint={}, lower_aarch64_sync={}, lower_aarch32_sync={}, populated_slots={}\n",
                                    vector_candidate.base_virtual_address,
                                    render_optional_u64(vector_candidate.base_physical_address),
                                    render_optional_instruction_word(
                                        vector_candidate.current_el_sp0_sync_instruction_word,
                                    ),
                                    render_optional_instruction_word(
                                        vector_candidate.current_el_spx_sync_instruction_word,
                                    ),
                                    vector_candidate.current_el_spx_sync_instruction_hint,
                                    render_optional_instruction_word(
                                        vector_candidate.lower_aarch64_sync_instruction_word,
                                    ),
                                    render_optional_instruction_word(
                                        vector_candidate.lower_aarch32_sync_instruction_word,
                                    ),
                                    vector_candidate.populated_slot_count,
                                ));
                            }
                        }
                    }
                }
            }
        }
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
            "Low firmware alias unmap status name: {}\n",
            render_optional_status_name(self.low_firmware_alias_unmap_status)
        ));
        output.push_str(&format!(
            "Low vars alias unmap status name: {}\n",
            render_optional_status_name(self.low_vars_alias_unmap_status)
        ));
        output.push_str(&format!(
            "Guest RAM unmap status name: {}\n",
            render_optional_status_name(self.guest_ram_unmap_status)
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
            "Guest RAM deallocate status name: {}\n",
            render_optional_status_name(self.guest_ram_deallocate_status)
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
