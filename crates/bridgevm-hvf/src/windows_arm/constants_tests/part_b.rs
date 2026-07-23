//! Tests for the Windows-on-Arm constants and vector vocabulary.

use super::helpers::*;
use crate::windows_arm::*;
use crate::*;

#[test]
fn windows_11_arm_uefi_pflash_map_probe_loads_verified_slots() {
    let stem = format!(
        "bridgevm-windows-arm-uefi-pflash-map-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let firmware_path = std::env::temp_dir().join(format!("{stem}-code.fd"));
    let template_path = std::env::temp_dir().join(format!("{stem}-vars-template.fd"));
    let vars_path = std::env::temp_dir().join(format!("{stem}-vars.fd"));
    std::fs::write(&firmware_path, test_uefi_fv_bytes(128 * 1024)).unwrap();
    std::fs::write(&template_path, test_uefi_fv_bytes(64 * 1024)).unwrap();
    let _ = std::fs::remove_file(&vars_path);

    let probe = probe_windows_11_arm_uefi_pflash_map(WindowsArmUefiPflashMapOptions {
        firmware_path: firmware_path.clone(),
        vars_template_path: Some(template_path.clone()),
        vars_path: Some(vars_path.clone()),
        create_vars: true,
    });
    let output = probe.render_text();
    let vars_bytes = std::fs::read(&vars_path).unwrap();
    let template_bytes = std::fs::read(&template_path).unwrap();
    let _ = std::fs::remove_file(&firmware_path);
    let _ = std::fs::remove_file(&template_path);
    let _ = std::fs::remove_file(&vars_path);

    assert_eq!(vars_bytes, template_bytes);
    assert_eq!(probe.firmware_path, firmware_path);
    assert_eq!(probe.vars_path, Some(vars_path));
    assert!(probe.vars_created);
    assert!(probe.firmware_verified);
    assert!(probe.vars_verified);
    assert!(probe.pflash_slots_non_overlapping);
    assert!(probe.guest_ram_overlap_verified);
    assert!(probe.device_mmio_overlap_verified);
    assert!(probe.pflash_map_verified);
    assert_eq!(
        probe.planned_reset_vector_ipa,
        Some(WINDOWS_ARM_UEFI_CODE_IPA)
    );
    let firmware_slot = probe.firmware_slot.as_ref().unwrap();
    assert_eq!(firmware_slot.name, "code");
    assert_eq!(firmware_slot.ipa_start, WINDOWS_ARM_UEFI_CODE_IPA);
    assert_eq!(firmware_slot.ipa_end_exclusive(), WINDOWS_ARM_UEFI_VARS_IPA);
    assert_eq!(firmware_slot.source_bytes, 128 * 1024);
    assert_eq!(
        firmware_slot.zero_padding_bytes,
        WINDOWS_ARM_UEFI_SLOT_BYTES - 128 * 1024
    );
    assert!(!firmware_slot.writable);
    assert!(firmware_slot.prefix_verified);
    assert!(firmware_slot.padding_zeroed);
    let vars_slot = probe.vars_slot.as_ref().unwrap();
    assert_eq!(vars_slot.name, "vars");
    assert_eq!(vars_slot.ipa_start, WINDOWS_ARM_UEFI_VARS_IPA);
    assert_eq!(vars_slot.ipa_end_exclusive(), WINDOWS_ARM_DEVICE_MMIO_IPA);
    assert_eq!(vars_slot.source_bytes, 64 * 1024);
    assert!(vars_slot.writable);
    assert!(vars_slot.prefix_verified);
    assert!(vars_slot.padding_zeroed);
    assert!(probe.blockers.is_empty());
    assert!(output.contains("Windows 11 Arm HVF UEFI pflash map probe"));
    assert!(output.contains("QEMU: not used"));
    assert!(output.contains("Apple VZ: not used"));
    assert!(output.contains("HVF: not entered"));
    assert!(output.contains("AArch64 UEFI pflash slots loaded into memory images"));
    assert!(output.contains("Firmware pflash loaded: true"));
    assert!(output.contains("Firmware pflash IPA range: 0x8000000..0xc000000"));
    assert!(output.contains("Firmware pflash source bytes: 0x20000"));
    assert!(output.contains("Firmware pflash prefix verified: true"));
    assert!(output.contains("Firmware pflash padding zeroed: true"));
    assert!(output.contains("Vars pflash loaded: true"));
    assert!(output.contains("Vars pflash IPA range: 0xc000000..0x10000000"));
    assert!(output.contains("Vars pflash source bytes: 0x10000"));
    assert!(output.contains("Vars pflash writable: true"));
    assert!(output.contains("Pflash slots non-overlapping: true"));
    assert!(output.contains("Guest RAM overlap verified: true"));
    assert!(output.contains("Device MMIO overlap verified: true"));
    assert!(output.contains("Pflash map verified: true"));
    assert!(output.contains("Planned reset vector IPA: 0x8000000"));
    assert!(output.contains("Blockers: none"));
    assert!(!output.contains("qemu-system"));
    assert!(!output.contains('%'));
}

#[test]
fn pflash_slot_load_rejects_oversized_file_before_allocation() {
    let path = std::env::temp_dir().join(format!(
        "bridgevm-oversized-pflash-{}-{}.fd",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let file = std::fs::File::create(&path).unwrap();
    file.set_len(512 * 1024 * 1024).unwrap();

    let error =
        load_uefi_pflash_slot("code", &path, WINDOWS_ARM_UEFI_CODE_IPA, 4096, false).unwrap_err();
    let _ = std::fs::remove_file(&path);

    assert!(error.contains("536870912 bytes"), "{error}");
    assert!(error.contains("4096 byte region"), "{error}");
}

#[test]
fn windows_11_arm_uefi_pflash_hvf_map_probe_defaults_to_no_live_map() {
    let stem = format!(
        "bridgevm-windows-arm-uefi-pflash-hvf-map-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let firmware_path = std::env::temp_dir().join(format!("{stem}-code.fd"));
    let template_path = std::env::temp_dir().join(format!("{stem}-vars-template.fd"));
    let vars_path = std::env::temp_dir().join(format!("{stem}-vars.fd"));
    std::fs::write(&firmware_path, test_uefi_fv_bytes(128 * 1024)).unwrap();
    std::fs::write(&template_path, test_uefi_fv_bytes(64 * 1024)).unwrap();
    let _ = std::fs::remove_file(&vars_path);

    let probe = probe_windows_11_arm_uefi_pflash_hvf_map(
        WindowsArmUefiPflashMapOptions {
            firmware_path: firmware_path.clone(),
            vars_template_path: Some(template_path.clone()),
            vars_path: Some(vars_path.clone()),
            create_vars: true,
        },
        false,
    );
    let output = probe.render_text();
    let _ = std::fs::remove_file(&firmware_path);
    let _ = std::fs::remove_file(&template_path);
    let _ = std::fs::remove_file(&vars_path);

    assert!(!probe.allowed);
    assert!(!probe.attempted);
    assert!(!probe.vm_created);
    assert!(!probe.firmware_memory_allocated);
    assert!(!probe.vars_memory_allocated);
    assert!(!probe.firmware_memory_mapped);
    assert!(!probe.vars_memory_mapped);
    assert!(!probe.vm_destroyed);
    assert!(probe.pflash_map_verified);
    assert_eq!(probe.firmware_slot_ipa, WINDOWS_ARM_UEFI_CODE_IPA);
    assert_eq!(probe.vars_slot_ipa, WINDOWS_ARM_UEFI_VARS_IPA);
    assert_eq!(probe.slot_bytes, WINDOWS_ARM_UEFI_SLOT_BYTES);
    assert_eq!(probe.firmware_source_bytes, Some(128 * 1024));
    assert_eq!(probe.vars_source_bytes, Some(64 * 1024));
    assert_eq!(probe.firmware_map_flags, "read|exec");
    assert_eq!(probe.vars_map_flags, "read|write");
    assert!(probe
        .blockers
        .iter()
        .any(|blocker| blocker.contains("BRIDGEVM_HVF_ALLOW_UEFI_PFLASH_MAP")));
    assert!(output.contains("Windows 11 Arm HVF UEFI pflash HVF map/unmap probe"));
    assert!(output.contains("QEMU: not used"));
    assert!(output.contains("Apple VZ: not used"));
    assert!(output.contains("Guest execution: not entered"));
    assert!(output.contains("Allowed: false"));
    assert!(output.contains("Attempted: false"));
    assert!(output.contains("Pflash map verified: true"));
    assert!(output.contains("Firmware slot IPA: 0x8000000"));
    assert!(output.contains("Vars slot IPA: 0xc000000"));
    assert!(output.contains("Slot bytes: 0x4000000"));
    assert!(output.contains("Firmware source bytes: 0x20000"));
    assert!(output.contains("Vars source bytes: 0x10000"));
    assert!(output.contains("Firmware map flags: read|exec"));
    assert!(output.contains("Vars map flags: read|write"));
    assert!(output.contains("BRIDGEVM_HVF_ALLOW_UEFI_PFLASH_MAP"));
    assert!(!output.contains("qemu-system"));
    assert!(!output.contains('%'));
}

#[test]
fn windows_11_arm_uefi_reset_vector_entry_probe_defaults_to_no_live_entry() {
    let stem = format!(
        "bridgevm-windows-arm-uefi-reset-vector-entry-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let firmware_path = std::env::temp_dir().join(format!("{stem}-code.fd"));
    let template_path = std::env::temp_dir().join(format!("{stem}-vars-template.fd"));
    let vars_path = std::env::temp_dir().join(format!("{stem}-vars.fd"));
    std::fs::write(&firmware_path, test_uefi_fv_bytes(128 * 1024)).unwrap();
    std::fs::write(&template_path, test_uefi_fv_bytes(64 * 1024)).unwrap();
    let _ = std::fs::remove_file(&vars_path);

    let probe = probe_windows_11_arm_uefi_reset_vector_entry(
        WindowsArmUefiPflashMapOptions {
            firmware_path: firmware_path.clone(),
            vars_template_path: Some(template_path.clone()),
            vars_path: Some(vars_path.clone()),
            create_vars: true,
        },
        false,
    );
    let output = probe.render_text();
    let _ = std::fs::remove_file(&firmware_path);
    let _ = std::fs::remove_file(&template_path);
    let _ = std::fs::remove_file(&vars_path);

    assert!(!probe.allowed);
    assert!(!probe.attempted);
    assert!(!probe.vm_created);
    assert!(!probe.firmware_memory_allocated);
    assert!(!probe.vars_memory_allocated);
    assert!(!probe.firmware_memory_mapped);
    assert!(!probe.vars_memory_mapped);
    assert!(!probe.vcpu_created);
    assert!(!probe.pc_set);
    assert!(!probe.cpsr_set);
    assert!(!probe.run_attempted);
    assert!(!probe.reset_vector_entry_observed);
    assert!(!probe.firmware_progress_observed);
    assert!(!probe.vm_destroyed);
    assert!(probe.pflash_map_verified);
    assert_eq!(probe.reset_vector_ipa, WINDOWS_ARM_UEFI_CODE_IPA);
    assert_eq!(probe.firmware_slot_ipa, WINDOWS_ARM_UEFI_CODE_IPA);
    assert_eq!(probe.vars_slot_ipa, WINDOWS_ARM_UEFI_VARS_IPA);
    assert_eq!(probe.slot_bytes, WINDOWS_ARM_UEFI_SLOT_BYTES);
    assert_eq!(probe.firmware_source_bytes, Some(128 * 1024));
    assert_eq!(probe.vars_source_bytes, Some(64 * 1024));
    assert_eq!(probe.firmware_map_flags, "read|exec");
    assert_eq!(probe.vars_map_flags, "read|write");
    assert!(probe
        .blockers
        .iter()
        .any(|blocker| blocker.contains("BRIDGEVM_HVF_ALLOW_UEFI_RESET_VECTOR_ENTRY")));
    assert!(output.contains("Windows 11 Arm HVF UEFI reset-vector entry probe"));
    assert!(output.contains("QEMU: not used"));
    assert!(output.contains("Apple VZ: not used"));
    assert!(output.contains("Guest execution: UEFI reset vector entered under watchdog"));
    assert!(output.contains("Windows boot: not claimed"));
    assert!(output.contains("Allowed: false"));
    assert!(output.contains("Attempted: false"));
    assert!(output.contains("Pflash map verified: true"));
    assert!(output.contains("Reset vector IPA: 0x8000000"));
    assert!(output.contains("Firmware source bytes: 0x20000"));
    assert!(output.contains("Vars source bytes: 0x10000"));
    assert!(output.contains("VM create status name: not attempted"));
    assert!(output.contains("Run status name: not attempted"));
    assert!(output.contains("Firmware progress observed: false"));
    assert!(output.contains("Exit exception class name: not observed"));
    assert!(output.contains("BRIDGEVM_HVF_ALLOW_UEFI_RESET_VECTOR_ENTRY"));
    assert!(!output.contains("qemu-system"));
    assert!(!output.contains('%'));
}

#[test]
fn windows_11_arm_uefi_firmware_run_loop_probe_defaults_to_no_live_loop() {
    let stem = format!(
        "bridgevm-windows-arm-uefi-firmware-run-loop-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let firmware_path = std::env::temp_dir().join(format!("{stem}-code.fd"));
    let template_path = std::env::temp_dir().join(format!("{stem}-vars-template.fd"));
    let vars_path = std::env::temp_dir().join(format!("{stem}-vars.fd"));
    let installer_iso_path = std::env::temp_dir().join(format!("{stem}-win11-arm.iso"));
    let writable_target_disk_path = std::env::temp_dir().join(format!("{stem}-windows.raw"));
    std::fs::write(&firmware_path, test_uefi_fv_bytes(128 * 1024)).unwrap();
    std::fs::write(&template_path, test_uefi_fv_bytes(64 * 1024)).unwrap();
    let _ = std::fs::remove_file(&vars_path);

    let probe = probe_windows_11_arm_uefi_firmware_run_loop(WindowsArmUefiFirmwareRunLoopOptions {
        pflash: WindowsArmUefiPflashMapOptions {
            firmware_path: firmware_path.clone(),
            vars_template_path: Some(template_path.clone()),
            vars_path: Some(vars_path.clone()),
            create_vars: true,
        },
        execution: WindowsArmUefiFirmwareRunLoopExecutionOptions {
            allow_loop: false,
            requested_exits: 8,
            guest_ram_mib: 64,
            watchdog_timeout_ms: 100,
            map_low_pflash_alias: false,
            seed_diagnostic_vector: false,
            seed_guest_ram_diagnostic_vector: false,
            seed_executable_diagnostic_vector: false,
            try_recommended_vector_base_vbar: false,
            continue_after_recommended_vector_base_vbar: false,
            repair_low_vector_diagnostic_page: false,
            remap_low_vector_to_recommended_vector: false,
            continue_after_low_vector_repair: false,
            restore_low_vector_slot_before_eret: false,
            wire_interrupt_timer: false,
            stop_at_first_post_repair_device_boundary: false,
            installer_iso_path: Some(installer_iso_path.clone()),
            writable_target_disk_path: Some(writable_target_disk_path.clone()),
        },
    });
    let output = probe.render_text();
    let _ = std::fs::remove_file(&firmware_path);
    let _ = std::fs::remove_file(&template_path);
    let _ = std::fs::remove_file(&vars_path);

    assert!(!probe.allowed);
    assert!(!probe.attempted);
    assert!(!probe.vm_created);
    assert!(!probe.guest_ram_memory_allocated);
    assert!(!probe.low_pflash_alias_requested);
    assert!(!probe.low_firmware_alias_mapped);
    assert!(!probe.low_vars_alias_mapped);
    assert!(!probe.guest_ram_memory_mapped);
    assert!(!probe.platform_dtb_populated);
    assert!(!probe.diagnostic_vector_seed_requested);
    assert!(!probe.diagnostic_vector_populated);
    assert!(!probe.low_vector_diagnostic_page_repair_requested);
    assert!(!probe.low_vector_diagnostic_page_repaired);
    assert!(!probe.low_vector_diagnostic_page_slot_restored);
    assert!(!probe.low_vector_diagnostic_page_restore_before_eret_requested);
    assert!(!probe.low_vector_diagnostic_page_restore_before_eret_attempted);
    assert_eq!(probe.low_vector_diagnostic_page_previous_descriptor, None);
    assert!(!probe.low_vector_diagnostic_page_repeated_fault_observed);
    assert!(!probe.low_vector_post_repair_continue_requested);
    assert!(!probe.low_vector_post_repair_continue_attempted);
    assert!(!probe.low_vector_post_repair_unsupported_exit_observed);
    assert_eq!(probe.low_vector_post_repair_unsupported_exit_reason, None);
    assert_eq!(
        probe.low_vector_post_repair_unsupported_exit_diagnosis,
        "not observed"
    );
    assert!(!probe.low_vector_post_repair_first_exit_observed);
    assert_eq!(probe.low_vector_post_repair_first_exit_index, None);
    assert_eq!(probe.low_vector_post_repair_first_exit_reason, None);
    assert_eq!(
        probe.low_vector_post_repair_first_exit_diagnosis,
        "not observed"
    );
    assert_eq!(probe.low_vector_post_repair_first_exit_pc, None);
    assert_eq!(
        probe.low_vector_post_repair_first_interaction_kind,
        "not observed"
    );
    assert_eq!(
        probe.low_vector_post_repair_first_exit_access_kind,
        "not observed"
    );
    assert_eq!(
        probe.low_vector_post_repair_first_exit_access_direction,
        "not observed"
    );
    assert_eq!(probe.low_vector_post_repair_first_exit_access_address, None);
    assert_eq!(probe.low_vector_post_repair_first_exit_access_sysreg, None);
    assert_eq!(
        probe.low_vector_post_repair_first_exit_access_syndrome,
        None
    );
    assert!(!probe.low_vector_post_repair_first_device_interaction_observed);
    assert_eq!(
        probe.low_vector_post_repair_first_device_interaction_index,
        None
    );
    assert_eq!(
        probe.low_vector_post_repair_first_device_interaction_reason,
        None
    );
    assert_eq!(
        probe.low_vector_post_repair_first_device_interaction_diagnosis,
        "not observed"
    );
    assert_eq!(
        probe.low_vector_post_repair_first_device_interaction_pc,
        None
    );
    assert_eq!(
        probe.low_vector_post_repair_first_device_interaction_kind,
        "not observed"
    );
    assert_eq!(
        probe.low_vector_post_repair_first_device_interaction_access_kind,
        "not observed"
    );
    assert_eq!(
        probe.low_vector_post_repair_first_device_interaction_access_direction,
        "not observed"
    );
    assert_eq!(
        probe.low_vector_post_repair_first_device_interaction_access_address,
        None
    );
    assert_eq!(
        probe.low_vector_post_repair_first_device_interaction_access_sysreg,
        None
    );
    assert_eq!(
        probe.low_vector_post_repair_first_device_interaction_access_syndrome,
        None
    );
    assert!(!probe.low_vector_post_repair_first_unhandled_access_observed);
    assert_eq!(
        probe.low_vector_post_repair_first_unhandled_access_index,
        None
    );
    assert_eq!(
        probe.low_vector_post_repair_first_unhandled_access_reason,
        None
    );
    assert_eq!(
        probe.low_vector_post_repair_first_unhandled_access_diagnosis,
        "not observed"
    );
    assert_eq!(probe.low_vector_post_repair_first_unhandled_access_pc, None);
    assert_eq!(
        probe.low_vector_post_repair_first_unhandled_access_syndrome,
        None
    );
    assert_eq!(
        probe.low_vector_post_repair_first_unhandled_access_kind,
        "not observed"
    );
    assert_eq!(
        probe.low_vector_post_repair_first_unhandled_access_direction,
        "not observed"
    );
    assert_eq!(
        probe.low_vector_post_repair_first_unhandled_access_register,
        None
    );
    assert_eq!(
        probe.low_vector_post_repair_first_unhandled_access_value,
        None
    );
    assert_eq!(
        probe.low_vector_post_repair_first_unhandled_access_handler_result,
        "not observed"
    );
    assert_eq!(
        probe.low_vector_post_repair_first_unhandled_access_mmio_ipa,
        None
    );
    assert_eq!(
        probe.low_vector_post_repair_first_unhandled_access_mmio_width,
        None
    );
    assert_eq!(
        probe.low_vector_post_repair_first_unhandled_access_mmio_device_kind,
        "not observed"
    );
    assert_eq!(
        probe.low_vector_post_repair_first_unhandled_access_sysreg,
        None
    );
    assert_eq!(
        probe.low_vector_post_repair_first_unhandled_access_sysreg_name,
        "not observed"
    );
    assert_eq!(
        probe.low_vector_post_repair_first_unhandled_access_sysreg_op0,
        None
    );
    assert_eq!(
        probe.low_vector_post_repair_first_unhandled_access_sysreg_op1,
        None
    );
    assert_eq!(
        probe.low_vector_post_repair_first_unhandled_access_sysreg_crn,
        None
    );
    assert_eq!(
        probe.low_vector_post_repair_first_unhandled_access_sysreg_crm,
        None
    );
    assert_eq!(
        probe.low_vector_post_repair_first_unhandled_access_sysreg_op2,
        None
    );
    assert!(!probe.low_vector_diagnostic_page_resume_attempted);
    assert!(!probe.low_vector_diagnostic_page_resume_armed);
    assert_eq!(probe.low_vector_diagnostic_page_resume_original_pc, None);
    assert_eq!(
        probe.low_vector_diagnostic_page_resume_original_elr_el1,
        None
    );
    assert_eq!(
        probe.low_vector_diagnostic_page_resume_original_esr_el1,
        None
    );
    assert_eq!(
        probe.low_vector_diagnostic_page_resume_original_far_el1,
        None
    );
    assert_eq!(
        probe.low_vector_diagnostic_page_resume_original_spsr_el1,
        None
    );
    assert_eq!(probe.low_vector_diagnostic_page_original_slot_bytes, None);
    assert_eq!(
        probe.low_vector_diagnostic_page_resume_target_instruction_before_eret,
        None
    );
    assert_eq!(
        probe.low_vector_diagnostic_page_resume_target_stage1_leaf_descriptor_before_eret,
        None
    );
    assert_eq!(
        probe.low_vector_diagnostic_page_resume_target_stage1_leaf_kind_before_eret,
        "not observed"
    );
    assert!(
        !probe.low_vector_diagnostic_page_resume_target_is_installed_diagnostic_hvc_before_eret
    );
    assert_eq!(
        probe.low_vector_diagnostic_page_resume_elr_el1_set_status,
        None
    );
    assert_eq!(
        probe.low_vector_diagnostic_page_resume_spsr_el1_set_status,
        None
    );
    assert_eq!(
        probe.low_vector_diagnostic_page_resume_cpsr_set_status,
        None
    );
    assert_eq!(probe.low_vector_diagnostic_page_resume_pc_set_status, None);
    assert!(!probe.interrupt_timer_wiring_requested);
    assert!(!probe.interrupt_timer_initialized);
    assert!(!probe.vcpu_created);
    assert!(!probe.pc_set);
    assert!(!probe.x0_dtb_ipa_set);
    assert!(!probe.cpsr_set);
    assert!(!probe.sp_el1_set);
    assert!(!probe.diagnostic_vector_vbar_el1_set);
    assert!(!probe.recommended_vector_base_vbar_requested);
    assert!(!probe.recommended_vector_base_vbar_attempted);
    assert!(!probe.recommended_vector_base_vbar_set);
    assert!(!probe.recommended_vector_base_vbar_diagnostic_vector_populated);
    assert_eq!(probe.recommended_vector_base_vbar_source_exit_index, None);
    assert_eq!(probe.recommended_vector_base_vbar_target, None);
    assert_eq!(
        probe.recommended_vector_base_vbar_target_physical_address,
        None
    );
    assert_eq!(probe.recommended_vector_base_vbar_reason, "not requested");
    assert_eq!(
        probe.recommended_vector_base_vbar_current_el_spx_sync_instruction_word,
        None
    );
    assert_eq!(
        probe.recommended_vector_base_vbar_current_el_spx_sync_instruction_hint,
        "not observed"
    );
    assert!(!probe.recommended_vector_base_vbar_followup_exit_observed);
    assert_eq!(probe.recommended_vector_base_vbar_followup_exit_index, None);
    assert_eq!(
        probe.recommended_vector_base_vbar_followup_exit_reason,
        None
    );
    assert_eq!(
        probe.recommended_vector_base_vbar_followup_exit_diagnosis,
        "not observed"
    );
    assert_eq!(probe.recommended_vector_base_vbar_followup_pc, None);
    assert_eq!(probe.recommended_vector_base_vbar_followup_vbar_el1, None);
    assert!(!probe.recommended_vector_base_vbar_followup_target_still_set);
    assert_eq!(probe.recommended_vector_base_vbar_set_status, None);
    assert!(!probe.run_loop_attempted);
    assert!(!probe.firmware_progress_observed);
    assert!(!probe.unsupported_exit_observed);
    assert_eq!(probe.requested_exits, 8);
    assert_eq!(probe.observed_exits, 0);
    assert_eq!(probe.watchdog_timeout_ms, 100);
    assert_eq!(probe.vtimer_offset_value, None);
    assert_eq!(probe.cntv_cval_value, None);
    assert_eq!(probe.cntv_ctl_value, None);
    assert_eq!(probe.vtimer_exit_count, 0);
    assert_eq!(probe.pending_irq_injected_count, 0);
    assert_eq!(probe.device_irq_injected_count, 0);
    assert_eq!(probe.device_irq_cleared_count, 0);
    assert_eq!(probe.handled_mmio_read_count, 0);
    assert_eq!(probe.handled_mmio_write_count, 0);
    assert_eq!(probe.handled_pl011_mmio_count, 0);
    assert_eq!(probe.handled_pl031_mmio_count, 0);
    assert_eq!(probe.handled_gicd_mmio_count, 0);
    assert_eq!(probe.handled_gicr_mmio_count, 0);
    assert_eq!(probe.handled_virtio_installer_iso_mmio_count, 0);
    assert_eq!(probe.handled_virtio_target_disk_mmio_count, 0);
    assert_eq!(probe.virtio_queue_notify_count, 0);
    assert_eq!(probe.virtio_request_completion_count, 0);
    assert_eq!(probe.guest_ram_ipa, WINDOWS_ARM_GUEST_RAM_IPA);
    assert_eq!(probe.platform_dtb_ipa, WINDOWS_ARM_PLATFORM_DTB_IPA);
    assert_eq!(
        probe.platform_dtb_guest_ram_offset,
        WINDOWS_ARM_PLATFORM_DTB_GUEST_RAM_OFFSET
    );
    assert_eq!(
        probe.low_firmware_alias_ipa,
        WINDOWS_ARM_UEFI_LOW_CODE_ALIAS_IPA
    );
    assert_eq!(
        probe.low_vars_alias_ipa,
        WINDOWS_ARM_UEFI_LOW_VARS_ALIAS_IPA
    );
    assert_eq!(probe.guest_ram_bytes, 64 * 1024 * 1024);
    assert!(probe.platform_dtb_bytes >= 40);
    assert_eq!(probe.platform_dtb_magic, FDT_MAGIC);
    assert!(probe.platform_dtb_magic_verified);
    assert_eq!(
        probe.sp_el1_seed_ipa,
        WINDOWS_ARM_GUEST_RAM_IPA + 64 * 1024 * 1024 - 16
    );
    assert_eq!(
        probe.diagnostic_vector_ipa,
        WINDOWS_ARM_DIAGNOSTIC_VECTOR_IPA
    );
    assert_eq!(probe.diagnostic_vector_location, "pflash");
    assert_eq!(
        probe.diagnostic_vector_bytes,
        WINDOWS_ARM_DIAGNOSTIC_VECTOR_BYTES
    );
    assert!(probe.pflash_map_verified);
    assert_eq!(probe.firmware_source_bytes, Some(128 * 1024));
    assert_eq!(probe.vars_source_bytes, Some(64 * 1024));
    assert_eq!(probe.installer_iso_path, Some(installer_iso_path.clone()));
    assert_eq!(
        probe.writable_target_disk_path,
        Some(writable_target_disk_path.clone())
    );
    assert_eq!(probe.block_devices.len(), 2);
    let installer_block = probe
        .block_devices
        .iter()
        .find(|device| device.role == "installer-iso")
        .expect("installer ISO block metadata is present");
    assert_eq!(installer_block.label, "VirtIO-MMIO installer ISO");
    assert_eq!(installer_block.node_name, "virtio_mmio@10002000");
    assert_eq!(
        installer_block.base_ipa,
        WINDOWS_ARM_VIRTIO_INSTALLER_ISO_MMIO_IPA
    );
    assert_eq!(installer_block.bytes, VIRTIO_MMIO_REGISTER_WINDOW_BYTES);
    assert!(installer_block.read_only);
    assert_eq!(installer_block.backing_kind, "host-iso-readonly");
    assert_eq!(
        installer_block.backing_path,
        Some(installer_iso_path.clone())
    );
    assert_eq!(installer_block.device_features, VIRTIO_BLK_F_RO);
    let target_block = probe
        .block_devices
        .iter()
        .find(|device| device.role == "target-disk")
        .expect("target disk block metadata is present");
    assert_eq!(target_block.label, "VirtIO-MMIO target disk");
    assert_eq!(target_block.node_name, "virtio_mmio@10003000");
    assert_eq!(
        target_block.base_ipa,
        WINDOWS_ARM_VIRTIO_TARGET_DISK_MMIO_IPA
    );
    assert_eq!(target_block.bytes, VIRTIO_MMIO_REGISTER_WINDOW_BYTES);
    assert!(!target_block.read_only);
    assert_eq!(target_block.backing_kind, "host-file-writable");
    assert_eq!(
        target_block.backing_path,
        Some(writable_target_disk_path.clone())
    );
    assert_eq!(
        target_block.device_features,
        VIRTIO_MMIO_BLOCK_DEVICE_FEATURES_VALUE
    );
    assert!(probe.exits.is_empty());
    assert!(probe
        .blockers
        .iter()
        .any(|blocker| blocker.contains("BRIDGEVM_HVF_ALLOW_UEFI_FIRMWARE_RUN_LOOP")));
    assert!(output.contains("Windows 11 Arm HVF UEFI firmware run-loop probe"));
    assert!(output.contains("QEMU: not used"));
    assert!(output.contains("Apple VZ: not used"));
    assert!(output.contains("Guest execution: bounded UEFI firmware exit classification loop"));
    assert!(output.contains("Windows boot: not claimed"));
    assert!(output.contains("Allowed: false"));
    assert!(output.contains("Attempted: false"));
    assert!(output.contains("Low firmware alias mapped: false"));
    assert!(output.contains("Low vars alias mapped: false"));
    assert!(output.contains("Low firmware alias IPA: 0x0"));
    assert!(output.contains("Low vars alias IPA: 0x4000000"));
    assert!(output.contains("Guest RAM IPA: 0x40000000"));
    assert!(output.contains("Platform DTB populated: false"));
    assert!(output.contains("X0 DTB IPA set: false"));
    assert!(output.contains("Platform DTB IPA: 0x40010000"));
    assert!(output.contains("Platform DTB guest RAM offset: 0x10000"));
    assert!(output.contains("Platform DTB bytes: 0x"));
    assert!(output.contains("Platform DTB magic: 0xd00dfeed"));
    assert!(output.contains("Platform DTB magic verified: true"));
    assert!(output.contains("SP_EL1 seed IPA: 0x43fffff0"));
    assert!(output.contains("Diagnostic vector seed requested: false"));
    assert!(output.contains("Diagnostic vector populated: false"));
    assert!(output.contains("Recommended vector-base VBAR requested: false"));
    assert!(output.contains("Recommended vector-base VBAR attempted: false"));
    assert!(output.contains("Recommended vector-base VBAR set: false"));
    assert!(output.contains("Low vector diagnostic page repair requested: false"));
    assert!(output.contains("Low vector diagnostic page repaired: false"));
    assert!(output.contains("Low vector diagnostic page slot restored: false"));
    assert!(output.contains("Low vector diagnostic page restore before ERET requested: false"));
    assert!(output.contains("Low vector diagnostic page restore before ERET attempted: false"));
    assert!(output.contains("Low vector diagnostic page previous descriptor: not observed"));
    assert!(output.contains("Low vector diagnostic page repeated fault observed: false"));
    assert!(output.contains("Continue after low-vector repair requested: false"));
    assert!(output.contains("Continue after low-vector repair attempted: false"));
    assert!(output.contains("Post-repair unsupported exit observed: false"));
    assert!(output.contains("Post-repair unsupported exit reason name: not observed"));
    assert!(output.contains("Post-repair unsupported exit classification: not observed"));
    assert!(output.contains("Post-repair first exit observed: false"));
    assert!(output.contains("Post-repair first exit: not observed"));
    assert!(output.contains("Post-repair first exit reason name: not observed"));
    assert!(output.contains("Post-repair first exit classification: not observed"));
    assert!(output.contains("Post-repair first exit PC: not observed"));
    assert!(output.contains("Post-repair first exit instruction: not observed"));
    assert!(output.contains("Post-repair first exit instruction hint: not observed"));
    assert!(output.contains("Post-repair first exit VBAR_EL1: not observed"));
    assert!(output.contains("Post-repair first exit ELR_EL1: not observed"));
    assert!(output.contains("Post-repair first exit ESR_EL1: not observed"));
    assert!(output.contains("Post-repair first exit FAR_EL1: not observed"));
    assert!(output.contains("Post-repair first exit SPSR_EL1: not observed"));
    assert!(output.contains("Post-repair first exit access kind: not observed"));
    assert!(output.contains("Post-repair first exit access direction: not observed"));
    assert!(output.contains("Post-repair first exit access address: not observed"));
    assert!(output.contains("Post-repair first exit access sysreg: not observed"));
    assert!(output.contains("Post-repair first exit access syndrome: not observed"));
    assert!(output.contains("Post-repair first interaction kind: not observed"));
    assert!(output.contains("Post-repair first device interaction observed: false"));
    assert!(output.contains("Post-repair first device interaction: not observed"));
    assert!(output.contains("Post-repair first device interaction reason name: not observed"));
    assert!(output.contains("Post-repair first device interaction classification: not observed"));
    assert!(output.contains("Post-repair first device interaction PC: not observed"));
    assert!(output.contains("Post-repair first device interaction instruction: not observed"));
    assert!(output.contains("Post-repair first device interaction instruction hint: not observed"));
    assert!(output.contains("Post-repair first device interaction VBAR_EL1: not observed"));
    assert!(output.contains("Post-repair first device interaction ELR_EL1: not observed"));
    assert!(output.contains("Post-repair first device interaction ESR_EL1: not observed"));
    assert!(output.contains("Post-repair first device interaction FAR_EL1: not observed"));
    assert!(output.contains("Post-repair first device interaction SPSR_EL1: not observed"));
    assert!(output.contains("Post-repair first device interaction access kind: not observed"));
    assert!(output.contains("Post-repair first device interaction access direction: not observed"));
    assert!(output.contains("Post-repair first device interaction access address: not observed"));
    assert!(output.contains("Post-repair first device interaction access sysreg: not observed"));
    assert!(output.contains("Post-repair first device interaction access syndrome: not observed"));
    assert!(output.contains("Post-repair first device interaction kind: not observed"));
    assert!(output.contains("Post-repair first unhandled access observed: false"));
    assert!(output.contains("Post-repair first unhandled access: not observed"));
    assert!(output.contains("Post-repair first unhandled access reason name: not observed"));
    assert!(output.contains("Post-repair first unhandled access classification: not observed"));
    assert!(output.contains("Post-repair first unhandled access PC: not observed"));
    assert!(output.contains("Post-repair first unhandled access syndrome: not observed"));
    assert!(output.contains("Post-repair first unhandled access kind: not observed"));
    assert!(output.contains("Post-repair first unhandled access direction: not observed"));
    assert!(output.contains("Post-repair first unhandled access register: not observed"));
    assert!(output.contains("Post-repair first unhandled access value: not observed"));
    assert!(output.contains("Post-repair first unhandled access handler result: not observed"));
    assert!(output.contains("Post-repair first unhandled access MMIO IPA: not observed"));
    assert!(output.contains("Post-repair first unhandled access MMIO width: not observed"));
    assert!(output.contains("Post-repair first unhandled access MMIO device kind: not observed"));
    assert!(output.contains("Post-repair first unhandled access sysreg: not observed"));
    assert!(output.contains("Post-repair first unhandled access sysreg name: not observed"));
    assert!(output.contains("Post-repair first unhandled access sysreg op0: not observed"));
    assert!(output.contains("Post-repair first unhandled access sysreg op1: not observed"));
    assert!(output.contains("Post-repair first unhandled access sysreg crn: not observed"));
    assert!(output.contains("Post-repair first unhandled access sysreg crm: not observed"));
    assert!(output.contains("Post-repair first unhandled access sysreg op2: not observed"));
    assert!(output.contains("Low vector diagnostic page resume attempted: false"));
    assert!(output.contains("Low vector diagnostic page resume armed: false"));
    assert!(output.contains("Low vector diagnostic page resume original PC: not observed"));
    assert!(output.contains("Low vector diagnostic page resume original ELR_EL1: not observed"));
    assert!(output.contains("Low vector diagnostic page resume original ESR_EL1: not observed"));
    assert!(output.contains("Low vector diagnostic page resume original FAR_EL1: not observed"));
    assert!(output.contains("Low vector diagnostic page resume original SPSR_EL1: not observed"));
    assert!(output.contains("Diagnostic vector VBAR_EL1 set: false"));
    assert!(output.contains("Interrupt/timer wiring requested: false"));
    assert!(output.contains("Interrupt/timer initialized: false"));
    assert!(output.contains("Diagnostic vector location: pflash"));
    assert!(output.contains("Diagnostic vector IPA: 0x8000000"));
    assert!(output.contains("Diagnostic vector bytes: 0x800"));
    assert!(output.contains("Recommended vector-base VBAR source exit: not observed"));
    assert!(output.contains("Recommended vector-base VBAR target: not observed"));
    assert!(output.contains("Recommended vector-base VBAR target PA: not observed"));
    assert!(output.contains("Recommended vector-base VBAR reason: not requested"));
    assert!(output
        .contains("Recommended vector-base VBAR current EL/SPx sync instruction: not observed"));
    assert!(output.contains("Recommended vector-base VBAR current EL/SPx sync hint: not observed"));
    assert!(output.contains("Recommended vector-base VBAR follow-up exit observed: false"));
    assert!(output.contains("Recommended vector-base VBAR follow-up exit: not observed"));
    assert!(
        output.contains("Recommended vector-base VBAR follow-up exit reason name: not observed")
    );
    assert!(output.contains("Recommended vector-base VBAR follow-up classification: not observed"));
    assert!(output.contains("Recommended vector-base VBAR follow-up PC: not observed"));
    assert!(output.contains("Recommended vector-base VBAR follow-up VBAR_EL1: not observed"));
    assert!(output.contains("Recommended vector-base VBAR follow-up target still set: false"));
    assert!(output.contains("Low firmware alias map flags: read|exec"));
    assert!(output.contains("Low vars alias map flags: read|write"));
    assert!(output.contains("Low pflash alias requested: false"));
    assert!(output.contains("Low firmware alias map status name: not attempted"));
    assert!(output.contains("Low vars alias map status name: not attempted"));
    assert!(output.contains("Guest RAM bytes: 0x4000000"));
    assert!(output.contains("Requested exits: 8"));
    assert!(output.contains("Observed exits: 0"));
    assert!(output.contains("Watchdog timeout ms: 100"));
    assert!(output.contains("VTimer offset value: not observed"));
    assert!(output.contains("CNTV_CVAL_EL0 value: not observed"));
    assert!(output.contains("CNTV_CTL_EL0 value: not observed"));
    assert!(output.contains("VTimer exit count: 0"));
    assert!(output.contains("Pending IRQ injected count: 0"));
    assert!(output.contains("Device IRQ line asserted count: 0"));
    assert!(output.contains("Device IRQ line deasserted count: 0"));
    assert!(output.contains("Handled MMIO read count: 0"));
    assert!(output.contains("Handled MMIO write count: 0"));
    assert!(output.contains("Handled PL011 MMIO count: 0"));
    assert!(output.contains("Handled PL031 MMIO count: 0"));
    assert!(output.contains("Handled GICD MMIO count: 0"));
    assert!(output.contains("Handled GICR MMIO count: 0"));
    assert!(output.contains("Handled VirtIO installer ISO MMIO count: 0"));
    assert!(output.contains("Handled VirtIO target disk MMIO count: 0"));
    assert!(output.contains("VirtIO queue_notify count: 0"));
    assert!(output.contains("VirtIO request completion count: 0"));
    assert!(output.contains("Handled ICC read count: 0"));
    assert!(output.contains("Handled ICC write count: 0"));
    assert!(output.contains("Handled ICC_IAR1 read count: 0"));
    assert!(output.contains("Handled ICC_EOIR1 write count: 0"));
    assert!(output.contains("Handled ICC_DIR write count: 0"));
    assert!(output.contains("Last ICC_IAR1 INTID: not observed"));
    assert!(output.contains("Last ICC_EOIR1 INTID: not observed"));
    assert!(output.contains("Last ICC_DIR INTID: not observed"));
    assert!(output.contains(&format!(
        "Installer ISO path: {}",
        installer_iso_path.display()
    )));
    assert!(output.contains(&format!(
        "Writable target disk path: {}",
        writable_target_disk_path.display()
    )));
    assert!(output.contains("Firmware block devices:"));
    assert!(output.contains(&format!(
        "- role=installer-iso, label=VirtIO-MMIO installer ISO, node=virtio_mmio@10002000, base=0x10002000, bytes=0x1000, read_only=true, backing_kind=host-iso-readonly, backing_path={}, device_features=0x20",
        installer_iso_path.display()
    )));
    assert!(output.contains(&format!(
        "- role=target-disk, label=VirtIO-MMIO target disk, node=virtio_mmio@10003000, base=0x10003000, bytes=0x1000, read_only=false, backing_kind=host-file-writable, backing_path={}, device_features=0x0",
        writable_target_disk_path.display()
    )));
    assert!(output.contains("VTimer offset set status name: not attempted"));
    assert!(output.contains("Recommended vector-base VBAR set status name: not attempted"));
    assert!(output.contains("Recommended vector-base VBAR resume requested: false"));
    assert!(output.contains("Recommended vector-base VBAR resume attempted: false"));
    assert!(output.contains("Recommended vector-base VBAR resume armed: false"));
    assert!(output.contains("Recommended vector-base VBAR resume original PC: not observed"));
    assert!(output.contains("Recommended vector-base VBAR resume original ELR_EL1: not observed"));
    assert!(output.contains("Recommended vector-base VBAR resume original ESR_EL1: not observed"));
    assert!(output.contains("Recommended vector-base VBAR resume original FAR_EL1: not observed"));
    assert!(output.contains("Recommended vector-base VBAR resume original SPSR_EL1: not observed"));
    assert!(output
        .contains("Recommended vector-base VBAR resume ELR_EL1 set status name: not attempted"));
    assert!(output
        .contains("Recommended vector-base VBAR resume VBAR_EL1 set status name: not attempted"));
    assert!(output
        .contains("Recommended vector-base VBAR resume SPSR_EL1 set status name: not attempted"));
    assert!(
        output.contains("Recommended vector-base VBAR resume PC set status name: not attempted")
    );
    assert!(output.contains("X0 DTB IPA set status name: not attempted"));
    assert!(output.contains("CNTV_CVAL_EL0 set status name: not attempted"));
    assert!(output.contains("CNTV_CTL_EL0 set status name: not attempted"));
    assert!(
        output.contains("Low vector diagnostic page resume ELR_EL1 set status name: not attempted")
    );
    assert!(output
        .contains("Low vector diagnostic page resume SPSR_EL1 set status name: not attempted"));
    assert!(
        output.contains("Low vector diagnostic page resume CPSR set status name: not attempted")
    );
    assert!(output.contains("Low vector diagnostic page resume PC set status name: not attempted"));
    assert!(output.contains("Low vector diagnostic page original slot bytes: not observed"));
    assert!(output.contains("Low vector diagnostic page original sync instruction: not observed"));
    assert!(output.contains("Low vector diagnostic page original sync hint: not observed"));
    assert!(output.contains(
        "Low vector diagnostic page resume target instruction before ERET: not observed"
    ));
    assert!(
        output.contains("Low vector diagnostic page resume target hint before ERET: not observed")
    );
    assert!(output.contains(
        "Low vector diagnostic page resume target stage-1 descriptor before ERET: not observed"
    ));
    assert!(output.contains(
        "Low vector diagnostic page resume target stage-1 kind before ERET: not observed"
    ));
    assert!(output.contains(
        "Low vector diagnostic page resume target is installed diagnostic HVC before ERET: false"
    ));
    assert!(output.contains("VTimer initial unmask status name: not attempted"));
    assert!(output.contains("Last pending IRQ set status name: not attempted"));
    assert!(output.contains("Last device IRQ line assert status name: not attempted"));
    assert!(output.contains("Last device IRQ line deassert status name: not attempted"));
    assert!(output.contains("Last VTimer unmask status name: not attempted"));
    assert!(output.contains("Run-loop exits:"));
    assert!(output.contains("- none"));
    assert!(output.contains("BRIDGEVM_HVF_ALLOW_UEFI_FIRMWARE_RUN_LOOP"));
    assert!(!output.contains("qemu-system"));
    assert!(!output.contains('%'));
}
