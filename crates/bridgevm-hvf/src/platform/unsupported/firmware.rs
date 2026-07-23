//! Split out of unsupported.rs by responsibility.

use super::super::*;
use super::*;
use crate::*;

pub fn probe_windows_11_arm_uefi_firmware_run_loop(
    options: WindowsArmUefiFirmwareRunLoopExecutionOptions,
    pflash_map: WindowsArmUefiPflashMapProbe,
    host: HvfHostCapabilities,
) -> WindowsArmUefiFirmwareRunLoopProbe {
    let WindowsArmUefiFirmwareRunLoopExecutionOptions {
        allow_loop,
        requested_exits,
        guest_ram_mib,
        watchdog_timeout_ms,
        map_low_pflash_alias,
        seed_diagnostic_vector,
        seed_guest_ram_diagnostic_vector,
        seed_executable_diagnostic_vector,
        try_recommended_vector_base_vbar,
        continue_after_recommended_vector_base_vbar,
        repair_low_vector_diagnostic_page,
        remap_low_vector_to_recommended_vector: _,
        continue_after_low_vector_repair,
        restore_low_vector_slot_before_eret: _,
        wire_interrupt_timer,
        stop_at_first_post_repair_device_boundary,
        installer_iso_path,
        writable_target_disk_path,
    } = options.clone();
    let mut blockers = pflash_map.blockers.clone();
    if !allow_loop {
        blockers.push(
            "set BRIDGEVM_HVF_ALLOW_UEFI_FIRMWARE_RUN_LOOP=1 or pass --allow-loop to map Windows UEFI pflash plus guest RAM, create one vCPU, and classify bounded firmware exits under a watchdog".to_string(),
        );
    }
    blockers.push(
        "Apple Hypervisor.framework Windows UEFI firmware run-loop probe is only available on Apple Silicon macOS".to_string(),
    );
    let guest_ram_bytes = u64::from(guest_ram_mib.clamp(1, 4096)) * 1024 * 1024;
    let block_devices = windows_arm_firmware_block_devices(
        installer_iso_path.clone(),
        writable_target_disk_path.clone(),
    );
    let (platform_dtb_bytes, platform_dtb_magic, platform_dtb_magic_verified) =
        windows_arm_firmware_run_loop_dtb_metadata(guest_ram_bytes);
    let diagnostic_vector = windows_arm_diagnostic_vector_selection(
        seed_diagnostic_vector,
        seed_guest_ram_diagnostic_vector,
        seed_executable_diagnostic_vector,
    );
    let diagnostic_vector_seed_requested = diagnostic_vector.requested;
    let recommended_vector_base_vbar_reason = recommended_vector_base_vbar_initial_reason(
        try_recommended_vector_base_vbar,
        diagnostic_vector_seed_requested,
        repair_low_vector_diagnostic_page,
    );
    let low_vector_post_repair = LowVectorPostRepairTelemetry::default();
    WindowsArmUefiFirmwareRunLoopProbe {
        allowed: allow_loop,
        attempted: false,
        vm_created: false,
        firmware_memory_allocated: false,
        vars_memory_allocated: false,
        guest_ram_memory_allocated: false,
        firmware_memory_populated: false,
        vars_memory_populated: false,
        firmware_memory_mapped: false,
        vars_memory_mapped: false,
        low_firmware_alias_mapped: false,
        low_vars_alias_mapped: false,
        guest_ram_memory_mapped: false,
        platform_dtb_populated: false,
        diagnostic_vector_seed_requested,
        diagnostic_vector_populated: false,
        low_vector_diagnostic_page_repair_requested: repair_low_vector_diagnostic_page,
        low_vector_diagnostic_page_repaired: false,
        low_vector_diagnostic_page_slot_restored: false,
        low_vector_diagnostic_page_restore_before_eret_requested: options
            .restore_low_vector_slot_before_eret,
        low_vector_diagnostic_page_restore_before_eret_attempted: false,
        low_vector_diagnostic_page_entry_ipa: None,
        low_vector_diagnostic_page_previous_descriptor: None,
        low_vector_diagnostic_page_descriptor: None,
        low_vector_diagnostic_page_repeated_fault_observed: false,
        low_vector_recommended_vector_remap_requested: options
            .remap_low_vector_to_recommended_vector,
        low_vector_recommended_vector_remap_attempted: false,
        low_vector_recommended_vector_remap_succeeded: false,
        low_vector_recommended_vector_remap_target_physical_address: None,
        low_vector_recommended_vector_remap_descriptor: None,
        low_vector_post_repair_continue_requested: continue_after_low_vector_repair,
        low_vector_post_repair_continue_attempted: low_vector_post_repair.continue_attempted,
        stop_at_first_post_repair_device_boundary_requested:
            stop_at_first_post_repair_device_boundary,
        low_vector_post_repair_unsupported_exit_observed: low_vector_post_repair
            .unsupported_exit_observed,
        low_vector_post_repair_unsupported_exit_reason: low_vector_post_repair
            .unsupported_exit_reason,
        low_vector_post_repair_unsupported_exit_diagnosis: low_vector_post_repair
            .unsupported_exit_diagnosis,
        low_vector_post_repair_first_exit_observed: low_vector_post_repair.first_exit.observed,
        low_vector_post_repair_first_exit_index: low_vector_post_repair.first_exit.index,
        low_vector_post_repair_first_exit_reason: low_vector_post_repair.first_exit.reason,
        low_vector_post_repair_first_exit_diagnosis: low_vector_post_repair.first_exit.diagnosis,
        low_vector_post_repair_first_exit_pc: low_vector_post_repair.first_exit.pc,
        low_vector_post_repair_first_interaction_kind: low_vector_post_repair
            .first_exit
            .interaction_kind,
        low_vector_post_repair_first_exit_access_kind: low_vector_post_repair
            .first_exit
            .access
            .kind,
        low_vector_post_repair_first_exit_access_direction: low_vector_post_repair
            .first_exit
            .access
            .direction,
        low_vector_post_repair_first_exit_access_address: low_vector_post_repair
            .first_exit
            .access
            .address,
        low_vector_post_repair_first_exit_access_sysreg: low_vector_post_repair
            .first_exit
            .access
            .sysreg,
        low_vector_post_repair_first_exit_access_syndrome: low_vector_post_repair
            .first_exit
            .access
            .syndrome,
        low_vector_post_repair_first_device_interaction_observed: low_vector_post_repair
            .first_device_interaction
            .observed,
        low_vector_post_repair_first_device_interaction_index: low_vector_post_repair
            .first_device_interaction
            .index,
        low_vector_post_repair_first_device_interaction_reason: low_vector_post_repair
            .first_device_interaction
            .reason,
        low_vector_post_repair_first_device_interaction_diagnosis: low_vector_post_repair
            .first_device_interaction
            .diagnosis,
        low_vector_post_repair_first_device_interaction_pc: low_vector_post_repair
            .first_device_interaction
            .pc,
        low_vector_post_repair_first_device_interaction_kind: low_vector_post_repair
            .first_device_interaction
            .interaction_kind,
        low_vector_post_repair_first_device_interaction_access_kind: low_vector_post_repair
            .first_device_interaction
            .access
            .kind,
        low_vector_post_repair_first_device_interaction_access_direction: low_vector_post_repair
            .first_device_interaction
            .access
            .direction,
        low_vector_post_repair_first_device_interaction_access_address: low_vector_post_repair
            .first_device_interaction
            .access
            .address,
        low_vector_post_repair_first_device_interaction_access_sysreg: low_vector_post_repair
            .first_device_interaction
            .access
            .sysreg,
        low_vector_post_repair_first_device_interaction_access_syndrome: low_vector_post_repair
            .first_device_interaction
            .access
            .syndrome,
        low_vector_post_repair_first_unhandled_access_observed: low_vector_post_repair
            .first_unhandled_access
            .observed,
        low_vector_post_repair_first_unhandled_access_index: low_vector_post_repair
            .first_unhandled_access
            .index,
        low_vector_post_repair_first_unhandled_access_reason: low_vector_post_repair
            .first_unhandled_access
            .reason,
        low_vector_post_repair_first_unhandled_access_diagnosis: low_vector_post_repair
            .first_unhandled_access
            .diagnosis,
        low_vector_post_repair_first_unhandled_access_pc: low_vector_post_repair
            .first_unhandled_access
            .pc,
        low_vector_post_repair_first_unhandled_access_syndrome: low_vector_post_repair
            .first_unhandled_access
            .syndrome,
        low_vector_post_repair_first_unhandled_access_kind: low_vector_post_repair
            .first_unhandled_access
            .kind,
        low_vector_post_repair_first_unhandled_access_direction: low_vector_post_repair
            .first_unhandled_access
            .access,
        low_vector_post_repair_first_unhandled_access_register: low_vector_post_repair
            .first_unhandled_access
            .register,
        low_vector_post_repair_first_unhandled_access_value: low_vector_post_repair
            .first_unhandled_access
            .value,
        low_vector_post_repair_first_unhandled_access_handler_result: low_vector_post_repair
            .first_unhandled_access
            .handler_result,
        low_vector_post_repair_first_unhandled_access_mmio_ipa: low_vector_post_repair
            .first_unhandled_access
            .mmio_ipa,
        low_vector_post_repair_first_unhandled_access_mmio_width: low_vector_post_repair
            .first_unhandled_access
            .mmio_width,
        low_vector_post_repair_first_unhandled_access_mmio_device_kind: low_vector_post_repair
            .first_unhandled_access
            .mmio_device_kind,
        low_vector_post_repair_first_unhandled_access_sysreg: low_vector_post_repair
            .first_unhandled_access
            .sysreg,
        low_vector_post_repair_first_unhandled_access_sysreg_name: low_vector_post_repair
            .first_unhandled_access
            .sysreg_name,
        low_vector_post_repair_first_unhandled_access_sysreg_op0: low_vector_post_repair
            .first_unhandled_access
            .sysreg_op0,
        low_vector_post_repair_first_unhandled_access_sysreg_op1: low_vector_post_repair
            .first_unhandled_access
            .sysreg_op1,
        low_vector_post_repair_first_unhandled_access_sysreg_crn: low_vector_post_repair
            .first_unhandled_access
            .sysreg_crn,
        low_vector_post_repair_first_unhandled_access_sysreg_crm: low_vector_post_repair
            .first_unhandled_access
            .sysreg_crm,
        low_vector_post_repair_first_unhandled_access_sysreg_op2: low_vector_post_repair
            .first_unhandled_access
            .sysreg_op2,
        low_vector_diagnostic_page_resume_attempted: false,
        low_vector_diagnostic_page_resume_armed: false,
        low_vector_diagnostic_page_resume_original_pc: None,
        low_vector_diagnostic_page_resume_original_elr_el1: None,
        low_vector_diagnostic_page_resume_original_esr_el1: None,
        low_vector_diagnostic_page_resume_original_far_el1: None,
        low_vector_diagnostic_page_resume_original_spsr_el1: None,
        low_vector_diagnostic_page_original_slot_bytes: None,
        low_vector_diagnostic_page_resume_target_instruction_before_eret: None,
        low_vector_diagnostic_page_resume_target_stage1_leaf_descriptor_before_eret: None,
        low_vector_diagnostic_page_resume_target_stage1_leaf_kind_before_eret: "not observed",
        low_vector_diagnostic_page_resume_target_is_installed_diagnostic_hvc_before_eret: false,
        low_vector_diagnostic_page_resume_elr_el1_set_status: None,
        low_vector_diagnostic_page_resume_spsr_el1_set_status: None,
        low_vector_diagnostic_page_resume_cpsr_set_status: None,
        low_vector_diagnostic_page_resume_pc_set_status: None,
        vcpu_created: false,
        pc_set: false,
        x0_dtb_ipa_set: false,
        cpsr_set: false,
        sp_el1_set: false,
        diagnostic_vector_vbar_el1_set: false,
        recommended_vector_base_vbar_requested: try_recommended_vector_base_vbar,
        recommended_vector_base_vbar_attempted: false,
        recommended_vector_base_vbar_set: false,
        recommended_vector_base_vbar_diagnostic_vector_populated: false,
        recommended_vector_base_vbar_resume_requested: continue_after_recommended_vector_base_vbar,
        recommended_vector_base_vbar_resume_attempted: false,
        recommended_vector_base_vbar_resume_armed: false,
        interrupt_timer_wiring_requested: wire_interrupt_timer,
        interrupt_timer_initialized: false,
        run_loop_attempted: false,
        firmware_progress_observed: false,
        unsupported_exit_observed: false,
        watchdog_cancel_fired: false,
        vcpu_destroyed: false,
        firmware_memory_unmapped: false,
        vars_memory_unmapped: false,
        guest_ram_memory_unmapped: false,
        firmware_memory_deallocated: false,
        vars_memory_deallocated: false,
        guest_ram_memory_deallocated: false,
        vm_destroyed: false,
        host,
        pflash_map_verified: pflash_map.pflash_map_verified,
        reset_vector_ipa: WINDOWS_ARM_UEFI_CODE_IPA,
        firmware_slot_ipa: WINDOWS_ARM_UEFI_CODE_IPA,
        vars_slot_ipa: WINDOWS_ARM_UEFI_VARS_IPA,
        low_firmware_alias_ipa: WINDOWS_ARM_UEFI_LOW_CODE_ALIAS_IPA,
        low_vars_alias_ipa: WINDOWS_ARM_UEFI_LOW_VARS_ALIAS_IPA,
        guest_ram_ipa: WINDOWS_ARM_GUEST_RAM_IPA,
        platform_dtb_ipa: WINDOWS_ARM_PLATFORM_DTB_IPA,
        platform_dtb_guest_ram_offset: WINDOWS_ARM_PLATFORM_DTB_GUEST_RAM_OFFSET,
        sp_el1_seed_ipa: crate::windows_arm_initial_sp_el1_ipa(guest_ram_bytes),
        diagnostic_vector_location: diagnostic_vector.location,
        diagnostic_vector_ipa: diagnostic_vector.ipa,
        diagnostic_vector_bytes: WINDOWS_ARM_DIAGNOSTIC_VECTOR_BYTES,
        recommended_vector_base_vbar_source_exit_index: None,
        recommended_vector_base_vbar_target: None,
        recommended_vector_base_vbar_target_physical_address: None,
        recommended_vector_base_vbar_reason,
        recommended_vector_base_vbar_current_el_spx_sync_instruction_word: None,
        recommended_vector_base_vbar_current_el_spx_sync_instruction_hint: "not observed",
        recommended_vector_base_vbar_followup_exit_observed: false,
        recommended_vector_base_vbar_followup_exit_index: None,
        recommended_vector_base_vbar_followup_exit_reason: None,
        recommended_vector_base_vbar_followup_exit_diagnosis: "not observed",
        recommended_vector_base_vbar_followup_pc: None,
        recommended_vector_base_vbar_followup_vbar_el1: None,
        recommended_vector_base_vbar_followup_target_still_set: false,
        recommended_vector_base_vbar_resume_original_pc: None,
        recommended_vector_base_vbar_resume_original_elr_el1: None,
        recommended_vector_base_vbar_resume_original_esr_el1: None,
        recommended_vector_base_vbar_resume_original_far_el1: None,
        recommended_vector_base_vbar_resume_original_spsr_el1: None,
        slot_bytes: WINDOWS_ARM_UEFI_SLOT_BYTES,
        guest_ram_bytes,
        platform_dtb_bytes,
        platform_dtb_magic,
        platform_dtb_magic_verified,
        requested_exits: requested_exits.clamp(1, 64),
        observed_exits: 0,
        watchdog_timeout_ms: watchdog_timeout_ms.clamp(1, 60_000),
        vtimer_offset_value: wire_interrupt_timer.then_some(0x1000),
        cntv_cval_value: wire_interrupt_timer.then_some(0),
        cntv_ctl_value: wire_interrupt_timer.then_some(1),
        vtimer_exit_count: 0,
        pending_irq_injected_count: 0,
        device_irq_injected_count: 0,
        device_irq_cleared_count: 0,
        handled_mmio_read_count: 0,
        handled_mmio_write_count: 0,
        handled_pl011_mmio_count: 0,
        handled_pl031_mmio_count: 0,
        handled_gicd_mmio_count: 0,
        handled_gicr_mmio_count: 0,
        handled_virtio_installer_iso_mmio_count: 0,
        handled_virtio_target_disk_mmio_count: 0,
        virtio_queue_notify_count: 0,
        virtio_request_completion_count: 0,
        handled_icc_read_count: 0,
        handled_icc_write_count: 0,
        handled_icc_iar1_read_count: 0,
        handled_icc_eoir1_write_count: 0,
        handled_icc_dir_write_count: 0,
        last_icc_iar1_intid: None,
        last_icc_eoir1_intid: None,
        last_icc_dir_intid: None,
        firmware_source_bytes: pflash_map
            .firmware_slot
            .as_ref()
            .map(|slot| slot.source_bytes),
        vars_source_bytes: pflash_map.vars_slot.as_ref().map(|slot| slot.source_bytes),
        installer_iso_path,
        writable_target_disk_path,
        block_devices,
        firmware_map_flags: "read|exec",
        vars_map_flags: "read|write",
        low_firmware_alias_map_flags: "read|exec",
        low_vars_alias_map_flags: "read|write",
        guest_ram_map_flags: "read|write|exec",
        low_pflash_alias_requested: map_low_pflash_alias,
        vm_create_status: None,
        firmware_allocate_status: None,
        vars_allocate_status: None,
        guest_ram_allocate_status: None,
        firmware_map_status: None,
        vars_map_status: None,
        low_firmware_alias_map_status: None,
        low_vars_alias_map_status: None,
        guest_ram_map_status: None,
        vcpu_create_status: None,
        pc_set_status: None,
        x0_dtb_ipa_set_status: None,
        cpsr_set_status: None,
        sp_el1_set_status: None,
        diagnostic_vector_vbar_el1_set_status: None,
        recommended_vector_base_vbar_set_status: None,
        recommended_vector_base_vbar_resume_vbar_el1_set_status: None,
        recommended_vector_base_vbar_resume_elr_el1_set_status: None,
        recommended_vector_base_vbar_resume_spsr_el1_set_status: None,
        recommended_vector_base_vbar_resume_pc_set_status: None,
        vtimer_offset_set_status: None,
        cntv_cval_set_status: None,
        cntv_ctl_set_status: None,
        vtimer_initial_unmask_status: None,
        last_pending_irq_set_status: None,
        last_device_irq_set_status: None,
        last_device_irq_clear_status: None,
        last_vtimer_unmask_status: None,
        final_pc_status: None,
        final_pc: None,
        vcpu_destroy_status: None,
        firmware_unmap_status: None,
        vars_unmap_status: None,
        low_firmware_alias_unmap_status: None,
        low_vars_alias_unmap_status: None,
        guest_ram_unmap_status: None,
        firmware_deallocate_status: None,
        vars_deallocate_status: None,
        guest_ram_deallocate_status: None,
        vm_destroy_status: None,
        exits: Vec::new(),
        blockers,
    }
}
