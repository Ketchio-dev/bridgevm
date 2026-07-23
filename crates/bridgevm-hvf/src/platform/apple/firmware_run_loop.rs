//! The live UEFI firmware run loop and its result rendering.
//!
//! Split out of the single 12,111-line apple.rs backend.

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
        remap_low_vector_to_recommended_vector,
        continue_after_low_vector_repair,
        restore_low_vector_slot_before_eret,
        wire_interrupt_timer,
        stop_at_first_post_repair_device_boundary,
        installer_iso_path,
        writable_target_disk_path,
    } = options.clone();
    let mut blockers = pflash_map.blockers.clone();
    let diagnostic_vector = windows_arm_diagnostic_vector_selection(
        seed_diagnostic_vector,
        seed_guest_ram_diagnostic_vector,
        seed_executable_diagnostic_vector,
    );
    let diagnostic_vector_seed_requested = diagnostic_vector.requested;
    let diagnostic_vector_location = diagnostic_vector.location;
    let diagnostic_vector_ipa = diagnostic_vector.ipa;
    let diagnostic_vector_request_count = usize::from(seed_diagnostic_vector)
        + usize::from(seed_guest_ram_diagnostic_vector)
        + usize::from(seed_executable_diagnostic_vector);
    if diagnostic_vector_request_count > 1 {
        blockers.push(
            "multiple diagnostic vectors were requested; using the executable candidate when present, otherwise guest RAM".to_string(),
        );
    }
    if seed_executable_diagnostic_vector && !map_low_pflash_alias {
        blockers.push(
            "executable diagnostic vector requires --map-low-pflash-alias so VBAR_EL1 can target the low pflash alias".to_string(),
        );
    }
    if try_recommended_vector_base_vbar && diagnostic_vector_seed_requested {
        blockers.push(
            "recommended vector-base VBAR redirect is ignored while a diagnostic vector seed is requested".to_string(),
        );
    }
    if try_recommended_vector_base_vbar
        && repair_low_vector_diagnostic_page
        && !continue_after_recommended_vector_base_vbar
    {
        blockers.push(
            "recommended vector-base VBAR redirect is ignored while low-vector diagnostic page repair is requested without --continue-after-recommended-vector-base-vbar".to_string(),
        );
    }
    if continue_after_recommended_vector_base_vbar && !try_recommended_vector_base_vbar {
        blockers.push(
            "continue-after-recommended-vector-base-vbar requires --try-recommended-vector-base-vbar; recording the request as a no-op".to_string(),
        );
    }
    if repair_low_vector_diagnostic_page && !map_low_pflash_alias {
        blockers.push(
            "low-vector diagnostic page repair requires --map-low-pflash-alias so the patched low-vector page has a stage-2 pflash backing".to_string(),
        );
    }
    if continue_after_low_vector_repair && !repair_low_vector_diagnostic_page {
        blockers.push(
            "continue-after-low-vector-repair requires --repair-low-vector-diagnostic-page; recording the request as a no-op".to_string(),
        );
    }
    if restore_low_vector_slot_before_eret
        && (!repair_low_vector_diagnostic_page || !continue_after_low_vector_repair)
    {
        blockers.push(
            "restore-low-vector-slot-before-eret requires --repair-low-vector-diagnostic-page and --continue-after-low-vector-repair; recording the request as a no-op".to_string(),
        );
    }
    if remap_low_vector_to_recommended_vector
        && (!repair_low_vector_diagnostic_page || !continue_after_low_vector_repair)
    {
        blockers.push(
            "remap-low-vector-to-recommended-vector requires --repair-low-vector-diagnostic-page and --continue-after-low-vector-repair; recording the request as a no-op".to_string(),
        );
    }
    let firmware_source_bytes = pflash_map
        .firmware_slot
        .as_ref()
        .map(|slot| slot.source_bytes);
    let vars_source_bytes = pflash_map.vars_slot.as_ref().map(|slot| slot.source_bytes);
    let bounded_requested_exits = requested_exits.clamp(1, 64);
    if requested_exits == 0 || requested_exits > 64 {
        blockers.push(
            "--max-exits must be between 1 and 64; using the bounded firmware loop range"
                .to_string(),
        );
    }
    let bounded_watchdog_timeout_ms = watchdog_timeout_ms.clamp(1, 60_000);
    if watchdog_timeout_ms == 0 || watchdog_timeout_ms > 60_000 {
        blockers.push(
            "--watchdog-ms must be between 1 and 60000; using the bounded firmware watchdog range"
                .to_string(),
        );
    }
    let bounded_guest_ram_mib = guest_ram_mib.clamp(1, 4096);
    if guest_ram_mib == 0 || guest_ram_mib > 4096 {
        blockers.push(
            "--guest-ram-mib must be between 1 and 4096; using the bounded guest RAM range"
                .to_string(),
        );
    }
    let guest_ram_bytes = u64::from(bounded_guest_ram_mib) * 1024 * 1024;
    let platform_dtb_blob = build_windows_arm_firmware_run_loop_fdt_blob(guest_ram_bytes);
    let platform_dtb_bytes = platform_dtb_blob.len();
    let platform_dtb_magic = read_be_u32(&platform_dtb_blob, 0).unwrap_or(0);
    let platform_dtb_magic_verified = platform_dtb_magic == FDT_MAGIC;
    if !platform_dtb_magic_verified {
        blockers.push("platform DTB magic did not verify before firmware handoff".to_string());
    }
    let cntv_cval_value = firmware_vtimer_deadline(WINDOWS_ARM_VTIMER_OFFSET_VALUE);
    let cntv_ctl_value = 1;
    let guest_ram_bytes_usize: usize = match guest_ram_bytes.try_into() {
        Ok(bytes) => bytes,
        Err(_) => {
            blockers.push("guest RAM size does not fit in host usize".to_string());
            return firmware_run_loop_probe_result(FirmwareRunLoopProbeResultInput {
                allowed: allow_loop,
                attempted: false,
                host,
                pflash_map_verified: pflash_map.pflash_map_verified,
                guest_ram_bytes,
                requested_exits: bounded_requested_exits,
                watchdog_timeout_ms: bounded_watchdog_timeout_ms,
                options: &options,
                firmware_source_bytes,
                vars_source_bytes,
                blockers,
            });
        }
    };
    let block_devices = windows_arm_firmware_block_devices(
        installer_iso_path.clone(),
        writable_target_disk_path.clone(),
    );

    if !allow_loop {
        blockers.push(
            "set BRIDGEVM_HVF_ALLOW_UEFI_FIRMWARE_RUN_LOOP=1 or pass --allow-loop to map Windows UEFI pflash plus guest RAM, create one vCPU, and classify bounded firmware exits under a watchdog".to_string(),
        );
        return firmware_run_loop_probe_result(FirmwareRunLoopProbeResultInput {
            allowed: false,
            attempted: false,
            host,
            pflash_map_verified: pflash_map.pflash_map_verified,
            guest_ram_bytes,
            requested_exits: bounded_requested_exits,
            watchdog_timeout_ms: bounded_watchdog_timeout_ms,
            options: &options,
            firmware_source_bytes,
            vars_source_bytes,
            blockers,
        });
    }

    if !pflash_map.pflash_map_verified {
        blockers.push(
            "pflash memory-image mapper did not verify code/vars slots; refusing firmware run-loop entry"
                .to_string(),
        );
        return firmware_run_loop_probe_result(FirmwareRunLoopProbeResultInput {
            allowed: true,
            attempted: false,
            host,
            pflash_map_verified: false,
            guest_ram_bytes,
            requested_exits: bounded_requested_exits,
            watchdog_timeout_ms: bounded_watchdog_timeout_ms,
            options: &options,
            firmware_source_bytes,
            vars_source_bytes,
            blockers,
        });
    }

    if !host.available {
        blockers.push("Hypervisor.framework host capabilities are not available".to_string());
        return firmware_run_loop_probe_result(FirmwareRunLoopProbeResultInput {
            allowed: true,
            attempted: false,
            host,
            pflash_map_verified: true,
            guest_ram_bytes,
            requested_exits: bounded_requested_exits,
            watchdog_timeout_ms: bounded_watchdog_timeout_ms,
            options: &options,
            firmware_source_bytes,
            vars_source_bytes,
            blockers,
        });
    }

    let slot_bytes_usize: usize = WINDOWS_ARM_UEFI_SLOT_BYTES
        .try_into()
        .expect("Windows UEFI pflash slot fits in usize");
    let mut firmware_memory = ptr::null_mut();
    let mut vars_memory = ptr::null_mut();
    let sp_el1_seed_ipa = windows_arm_initial_sp_el1_ipa(guest_ram_bytes);
    let mut guest_ram_memory = ptr::null_mut();
    let mut firmware_memory_populated = false;
    let mut vars_memory_populated = false;
    let mut firmware_memory_mapped = false;
    let mut vars_memory_mapped = false;
    let mut low_firmware_alias_mapped = false;
    let mut low_vars_alias_mapped = false;
    let mut guest_ram_memory_mapped = false;
    let mut platform_dtb_populated = false;
    let mut diagnostic_vector_populated = false;
    let mut low_vector_diagnostic_page_repaired = false;
    let mut low_vector_diagnostic_page_slot_restored = false;
    let mut low_vector_diagnostic_page_restore_before_eret_attempted = false;
    let mut low_vector_diagnostic_page_slot_snapshot = None;
    let mut low_vector_diagnostic_page_entry_ipa = None;
    let mut low_vector_diagnostic_page_previous_descriptor = None;
    let mut low_vector_diagnostic_page_descriptor = None;
    let mut low_vector_diagnostic_page_repeated_fault_observed = false;
    let mut low_vector_recommended_vector_remap_attempted = false;
    let mut low_vector_recommended_vector_remap_succeeded = false;
    let mut low_vector_recommended_vector_remap_target_physical_address = None;
    let mut low_vector_recommended_vector_remap_descriptor = None;
    let mut low_vector_post_repair = LowVectorPostRepairTelemetry::default();
    let mut low_vector_resume = LowVectorDiagnosticPageResumeTelemetry::new();
    let mut vcpu_created = false;
    let mut pc_set = false;
    let mut x0_dtb_ipa_set = false;
    let mut cpsr_set = false;
    let mut sp_el1_set = false;
    let mut diagnostic_vector_vbar_el1_set = false;
    let mut recommended_vector_base_vbar_attempted = false;
    let mut recommended_vector_base_vbar_set = false;
    let mut recommended_vector_base_vbar_diagnostic_vector_populated = false;
    let mut recommended_vector_base_vbar_source_exit_index = None;
    let mut recommended_vector_base_vbar_target = None;
    let mut recommended_vector_base_vbar_target_physical_address = None;
    let mut recommended_vector_base_vbar_reason = recommended_vector_base_vbar_initial_reason(
        try_recommended_vector_base_vbar,
        diagnostic_vector_seed_requested,
        repair_low_vector_diagnostic_page,
    );
    let mut recommended_vector_base_vbar_current_el_spx_sync_instruction_word = None;
    let mut recommended_vector_base_vbar_current_el_spx_sync_instruction_hint = "not observed";
    let mut recommended_vector_base_vbar_followup_exit_observed = false;
    let mut recommended_vector_base_vbar_followup_exit_index = None;
    let mut recommended_vector_base_vbar_followup_exit_reason = None;
    let mut recommended_vector_base_vbar_followup_exit_diagnosis = "not observed";
    let mut recommended_vector_base_vbar_followup_pc = None;
    let mut recommended_vector_base_vbar_followup_vbar_el1 = None;
    let mut recommended_vector_base_vbar_followup_target_still_set = false;
    let mut recommended_vector_base_vbar_resume_attempted = false;
    let mut recommended_vector_base_vbar_resume_armed = false;
    let mut recommended_vector_base_vbar_resume_original_pc = None;
    let mut recommended_vector_base_vbar_resume_original_elr_el1 = None;
    let mut recommended_vector_base_vbar_resume_original_esr_el1 = None;
    let mut recommended_vector_base_vbar_resume_original_far_el1 = None;
    let mut recommended_vector_base_vbar_resume_original_spsr_el1 = None;
    let mut interrupt_timer_initialized = false;
    let mut run_loop_attempted = false;
    let mut firmware_progress_observed = false;
    let mut unsupported_exit_observed = false;
    let mut watchdog_cancel_fired = false;
    let mut vcpu_destroyed = false;
    let mut firmware_memory_unmapped = false;
    let mut vars_memory_unmapped = false;
    let mut guest_ram_memory_unmapped = false;
    let mut firmware_memory_deallocated = false;
    let mut vars_memory_deallocated = false;
    let mut guest_ram_memory_deallocated = false;

    let mut firmware_allocate_status = None;
    let mut vars_allocate_status = None;
    let mut guest_ram_allocate_status = None;
    let mut firmware_map_status = None;
    let mut vars_map_status = None;
    let mut low_firmware_alias_map_status = None;
    let mut low_vars_alias_map_status = None;
    let mut guest_ram_map_status = None;
    let mut vcpu_create_status = None;
    let mut pc_set_status = None;
    let mut x0_dtb_ipa_set_status = None;
    let mut cpsr_set_status = None;
    let mut sp_el1_set_status = None;
    let mut diagnostic_vector_vbar_el1_set_status = None;
    let mut recommended_vector_base_vbar_set_status = None;
    let mut recommended_vector_base_vbar_resume_vbar_el1_set_status = None;
    let mut recommended_vector_base_vbar_resume_elr_el1_set_status = None;
    let mut recommended_vector_base_vbar_resume_spsr_el1_set_status = None;
    let mut recommended_vector_base_vbar_resume_pc_set_status = None;
    let mut vtimer_offset_set_status = None;
    let mut cntv_cval_set_status = None;
    let mut cntv_ctl_set_status = None;
    let mut vtimer_initial_unmask_status = None;
    let mut last_pending_irq_set_status = None;
    let mut last_device_irq_set_status = None;
    let mut last_device_irq_clear_status = None;
    let mut last_vtimer_unmask_status = None;
    let mut final_pc_status = None;
    let mut final_pc = None;
    let mut vcpu_destroy_status = None;
    let mut firmware_unmap_status = None;
    let mut vars_unmap_status = None;
    let mut low_firmware_alias_unmap_status = None;
    let mut low_vars_alias_unmap_status = None;
    let mut guest_ram_unmap_status = None;
    let mut firmware_deallocate_status = None;
    let mut vars_deallocate_status = None;
    let mut guest_ram_deallocate_status = None;
    let mut exits = Vec::new();
    let mut vtimer_exit_count = 0;
    let mut pending_irq_injected_count = 0;
    let mut device_irq_injected_count = 0;
    let mut device_irq_cleared_count = 0;
    let mut handled_mmio_read_count = 0;
    let mut handled_mmio_write_count = 0;
    let mut handled_pl011_mmio_count = 0;
    let mut handled_pl031_mmio_count = 0;
    let mut handled_gicd_mmio_count = 0;
    let mut handled_gicr_mmio_count = 0;
    let mut handled_virtio_installer_iso_mmio_count = 0;
    let mut handled_virtio_target_disk_mmio_count = 0;
    let mut virtio_queue_notify_count = 0;
    let mut virtio_request_completion_count = 0;
    let mut handled_icc_read_count = 0;
    let mut handled_icc_write_count = 0;
    let mut handled_icc_iar1_read_count = 0;
    let mut handled_icc_eoir1_write_count = 0;
    let mut handled_icc_dir_write_count = 0;
    let mut last_icc_iar1_intid = None;
    let mut last_icc_eoir1_intid = None;
    let mut last_icc_dir_intid = None;
    let mut device_irq_line_asserted = false;
    let mut firmware_mmio_bus = windows_arm_firmware_mmio_bus_with_block_devices(&block_devices);
    let mut gic_cpu_interface = GicV3CpuInterfaceState::new();

    let mut vcpu = 0;
    let mut exit = ptr::null_mut();

    let vm_create_status = unsafe { hv_vm_create(ptr::null_mut()) };
    let vm_created = vm_create_status == HV_SUCCESS;
    if !vm_created {
        blockers.push(format!("hv_vm_create failed: {vm_create_status:#x}"));
    }

    let mut firmware_memory_allocated = false;
    let mut vars_memory_allocated = false;
    let mut guest_ram_memory_allocated = false;

    if vm_created {
        let status =
            unsafe { hv_vm_allocate(&mut firmware_memory, slot_bytes_usize, HV_ALLOCATE_DEFAULT) };
        firmware_allocate_status = Some(status);
        firmware_memory_allocated = status == HV_SUCCESS && !firmware_memory.is_null();
        if !firmware_memory_allocated {
            blockers.push(format!(
                "hv_vm_allocate firmware pflash failed: {status:#x}"
            ));
        }

        let status =
            unsafe { hv_vm_allocate(&mut vars_memory, slot_bytes_usize, HV_ALLOCATE_DEFAULT) };
        vars_allocate_status = Some(status);
        vars_memory_allocated = status == HV_SUCCESS && !vars_memory.is_null();
        if !vars_memory_allocated {
            blockers.push(format!("hv_vm_allocate vars pflash failed: {status:#x}"));
        }

        let status = unsafe {
            hv_vm_allocate(
                &mut guest_ram_memory,
                guest_ram_bytes_usize,
                HV_ALLOCATE_DEFAULT,
            )
        };
        guest_ram_allocate_status = Some(status);
        guest_ram_memory_allocated = status == HV_SUCCESS && !guest_ram_memory.is_null();
        if !guest_ram_memory_allocated {
            blockers.push(format!("hv_vm_allocate guest RAM failed: {status:#x}"));
        }
    }

    if firmware_memory_allocated {
        firmware_memory_populated = populate_pflash_hvf_memory(
            firmware_memory,
            pflash_map.firmware_slot.as_ref(),
            "firmware",
            &mut blockers,
        );
    }
    if vars_memory_allocated {
        vars_memory_populated = populate_pflash_hvf_memory(
            vars_memory,
            pflash_map.vars_slot.as_ref(),
            "vars",
            &mut blockers,
        );
    }

    if firmware_memory_populated {
        let status = unsafe {
            hv_vm_map(
                firmware_memory,
                WINDOWS_ARM_UEFI_CODE_IPA,
                slot_bytes_usize,
                HV_MEMORY_READ | HV_MEMORY_EXEC,
            )
        };
        firmware_map_status = Some(status);
        firmware_memory_mapped = status == HV_SUCCESS;
        if !firmware_memory_mapped {
            blockers.push(format!("hv_vm_map firmware pflash failed: {status:#x}"));
        }
    }

    if vars_memory_populated {
        let status = unsafe {
            hv_vm_map(
                vars_memory,
                WINDOWS_ARM_UEFI_VARS_IPA,
                slot_bytes_usize,
                HV_MEMORY_READ | HV_MEMORY_WRITE,
            )
        };
        vars_map_status = Some(status);
        vars_memory_mapped = status == HV_SUCCESS;
        if !vars_memory_mapped {
            blockers.push(format!("hv_vm_map vars pflash failed: {status:#x}"));
        }
    }

    if diagnostic_vector_seed_requested {
        if seed_executable_diagnostic_vector {
            if firmware_memory_populated {
                diagnostic_vector_populated = populate_diagnostic_exception_vector_slot(
                    firmware_memory,
                    slot_bytes_usize,
                    WINDOWS_ARM_EXECUTABLE_DIAGNOSTIC_VECTOR_IPA as usize,
                    "low pflash executable diagnostic candidate",
                    &mut blockers,
                );
            } else {
                blockers.push(
                    "executable diagnostic exception vector requested before firmware pflash population succeeded".to_string(),
                );
            }
        } else if seed_guest_ram_diagnostic_vector {
            if guest_ram_memory_allocated {
                diagnostic_vector_populated = populate_diagnostic_exception_vector_slot(
                    guest_ram_memory,
                    guest_ram_bytes_usize,
                    0,
                    "guest RAM",
                    &mut blockers,
                );
            } else {
                blockers.push(
                    "guest RAM diagnostic exception vector requested before guest RAM allocation succeeded".to_string(),
                );
            }
        } else if firmware_memory_populated {
            diagnostic_vector_populated = populate_diagnostic_exception_vector_slot(
                firmware_memory,
                slot_bytes_usize,
                0,
                "firmware pflash",
                &mut blockers,
            );
        } else {
            blockers.push(
                "pflash diagnostic exception vector requested before firmware pflash population succeeded".to_string(),
            );
        }
    }

    if guest_ram_memory_allocated && platform_dtb_magic_verified {
        platform_dtb_populated = populate_platform_dtb_guest_ram(
            guest_ram_memory,
            guest_ram_bytes_usize,
            &platform_dtb_blob,
            &mut blockers,
        );
    }

    if map_low_pflash_alias && firmware_memory_populated {
        let status = unsafe {
            hv_vm_map(
                firmware_memory,
                WINDOWS_ARM_UEFI_LOW_CODE_ALIAS_IPA,
                slot_bytes_usize,
                HV_MEMORY_READ | HV_MEMORY_EXEC,
            )
        };
        low_firmware_alias_map_status = Some(status);
        low_firmware_alias_mapped = status == HV_SUCCESS;
        if !low_firmware_alias_mapped {
            blockers.push(format!(
                "hv_vm_map low firmware pflash alias failed: {status:#x}"
            ));
        }
    }

    if map_low_pflash_alias && vars_memory_populated {
        let status = unsafe {
            hv_vm_map(
                vars_memory,
                WINDOWS_ARM_UEFI_LOW_VARS_ALIAS_IPA,
                slot_bytes_usize,
                HV_MEMORY_READ | HV_MEMORY_WRITE,
            )
        };
        low_vars_alias_map_status = Some(status);
        low_vars_alias_mapped = status == HV_SUCCESS;
        if !low_vars_alias_mapped {
            blockers.push(format!(
                "hv_vm_map low vars pflash alias failed: {status:#x}"
            ));
        }
    }

    if guest_ram_memory_allocated {
        let status = unsafe {
            hv_vm_map(
                guest_ram_memory,
                WINDOWS_ARM_GUEST_RAM_IPA,
                guest_ram_bytes_usize,
                HV_MEMORY_READ | HV_MEMORY_WRITE | HV_MEMORY_EXEC,
            )
        };
        guest_ram_map_status = Some(status);
        guest_ram_memory_mapped = status == HV_SUCCESS;
        if !guest_ram_memory_mapped {
            blockers.push(format!("hv_vm_map guest RAM failed: {status:#x}"));
        }
    }

    let requested_aliases_ready =
        !map_low_pflash_alias || (low_firmware_alias_mapped && low_vars_alias_mapped);
    if firmware_memory_mapped
        && vars_memory_mapped
        && guest_ram_memory_mapped
        && requested_aliases_ready
    {
        let status = unsafe { hv_vcpu_create(&mut vcpu, &mut exit, ptr::null_mut()) };
        vcpu_create_status = Some(status);
        vcpu_created = status == HV_SUCCESS;
        if !vcpu_created {
            blockers.push(format!("hv_vcpu_create failed: {status:#x}"));
        }
    }

    if vcpu_created {
        let status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_PC, WINDOWS_ARM_UEFI_CODE_IPA) };
        pc_set_status = Some(status);
        pc_set = status == HV_SUCCESS;
        if !pc_set {
            blockers.push(format!("hv_vcpu_set_reg(PC) failed: {status:#x}"));
        }
    }

    if vcpu_created && platform_dtb_populated {
        let status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_X0, WINDOWS_ARM_PLATFORM_DTB_IPA) };
        x0_dtb_ipa_set_status = Some(status);
        x0_dtb_ipa_set = status == HV_SUCCESS;
        if !x0_dtb_ipa_set {
            blockers.push(format!(
                "hv_vcpu_set_reg(X0=platform DTB IPA) failed: {status:#x}"
            ));
        }
    }

    if vcpu_created {
        let status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_CPSR, AARCH64_PSTATE_EL1H_DAIF_MASKED) };
        cpsr_set_status = Some(status);
        cpsr_set = status == HV_SUCCESS;
        if !cpsr_set {
            blockers.push(format!("hv_vcpu_set_reg(CPSR) failed: {status:#x}"));
        }
    }

    if vcpu_created {
        let status = unsafe { hv_vcpu_set_sys_reg(vcpu, HV_SYS_REG_SP_EL1, sp_el1_seed_ipa) };
        sp_el1_set_status = Some(status);
        sp_el1_set = status == HV_SUCCESS;
        if !sp_el1_set {
            blockers.push(format!("hv_vcpu_set_sys_reg(SP_EL1) failed: {status:#x}"));
        }
    }

    if vcpu_created && diagnostic_vector_seed_requested && diagnostic_vector_populated {
        let status =
            unsafe { hv_vcpu_set_sys_reg(vcpu, HV_SYS_REG_VBAR_EL1, diagnostic_vector_ipa) };
        diagnostic_vector_vbar_el1_set_status = Some(status);
        diagnostic_vector_vbar_el1_set = status == HV_SUCCESS;
        if !diagnostic_vector_vbar_el1_set {
            blockers.push(format!(
                "hv_vcpu_set_sys_reg(VBAR_EL1 diagnostic vector) failed: {status:#x}"
            ));
        }
    }

    if vcpu_created && wire_interrupt_timer {
        let offset_status =
            unsafe { hv_vcpu_set_vtimer_offset(vcpu, WINDOWS_ARM_VTIMER_OFFSET_VALUE) };
        vtimer_offset_set_status = Some(offset_status);
        if offset_status != HV_SUCCESS {
            blockers.push(format!(
                "hv_vcpu_set_vtimer_offset for firmware run-loop failed: {offset_status:#x}"
            ));
        }

        let cval_status =
            unsafe { hv_vcpu_set_sys_reg(vcpu, HV_SYS_REG_CNTV_CVAL_EL0, cntv_cval_value) };
        cntv_cval_set_status = Some(cval_status);
        if cval_status != HV_SUCCESS {
            blockers.push(format!(
                "hv_vcpu_set_sys_reg(CNTV_CVAL_EL0) for firmware run-loop failed: {cval_status:#x}"
            ));
        }

        let ctl_status =
            unsafe { hv_vcpu_set_sys_reg(vcpu, HV_SYS_REG_CNTV_CTL_EL0, cntv_ctl_value) };
        cntv_ctl_set_status = Some(ctl_status);
        if ctl_status != HV_SUCCESS {
            blockers.push(format!(
                "hv_vcpu_set_sys_reg(CNTV_CTL_EL0) for firmware run-loop failed: {ctl_status:#x}"
            ));
        }

        let unmask_status = unsafe { hv_vcpu_set_vtimer_mask(vcpu, false) };
        vtimer_initial_unmask_status = Some(unmask_status);
        if unmask_status != HV_SUCCESS {
            blockers.push(format!(
                "hv_vcpu_set_vtimer_mask(false) for firmware run-loop failed: {unmask_status:#x}"
            ));
        }

        interrupt_timer_initialized = offset_status == HV_SUCCESS
            && cval_status == HV_SUCCESS
            && ctl_status == HV_SUCCESS
            && unmask_status == HV_SUCCESS;
    }

    let diagnostic_vector_ready =
        !diagnostic_vector_seed_requested || diagnostic_vector_vbar_el1_set;
    let interrupt_timer_ready = !wire_interrupt_timer || interrupt_timer_initialized;
    if vcpu_created
        && pc_set
        && x0_dtb_ipa_set
        && cpsr_set
        && sp_el1_set
        && diagnostic_vector_ready
        && interrupt_timer_ready
    {
        run_loop_attempted = true;
        for index in 1..=bounded_requested_exits {
            let observation =
                run_vcpu_once_with_watchdog_timeout(vcpu, exit, bounded_watchdog_timeout_ms);
            let exit_exception_class = observation.exit_syndrome.map(arm_exception_class);
            let mut pc_after_exit = None;
            let mut pc = 0;
            let status = unsafe { hv_vcpu_get_reg(vcpu, HV_REG_PC, &mut pc) };
            let pc_after_exit_status = Some(status);
            final_pc_status = Some(status);
            if status == HV_SUCCESS {
                pc_after_exit = Some(pc);
                final_pc = Some(pc);
                firmware_progress_observed = pc != WINDOWS_ARM_UEFI_CODE_IPA;
            } else {
                blockers.push(format!(
                    "hv_vcpu_get_reg(PC) after firmware exit {index} failed: {status:#x}"
                ));
            }

            let watchdog_blocker = observation
                .watchdog_cancel_status
                .is_some()
                .then(|| format!("firmware run-loop watchdog fired before exit {index} completed"));
            if let Some(blocker) = &watchdog_blocker {
                watchdog_cancel_fired = true;
                blockers.push(blocker.clone());
            }

            let instruction_word_after_exit = read_guest_instruction_word(
                pc_after_exit,
                firmware_memory.cast_const(),
                vars_memory.cast_const(),
                guest_ram_memory.cast_const(),
                guest_ram_bytes_usize,
            );
            let instruction_hint_after_exit = instruction_word_after_exit
                .map(aarch64_instruction_hint)
                .unwrap_or("not observed");
            let x0_after_exit = read_vcpu_reg(vcpu, HV_REG_X0);
            let x1_after_exit = read_vcpu_reg(vcpu, HV_REG_X1);
            let x2_after_exit = read_vcpu_reg(vcpu, HV_REG_X2);
            let x3_after_exit = read_vcpu_reg(vcpu, HV_REG_X3);
            let x4_after_exit = read_vcpu_reg(vcpu, HV_REG_X4);
            let cpsr_after_exit = read_vcpu_reg(vcpu, HV_REG_CPSR);
            let vbar_el1_after_exit = read_vcpu_sys_reg(vcpu, HV_SYS_REG_VBAR_EL1);
            let elr_el1_after_exit = read_vcpu_sys_reg(vcpu, HV_SYS_REG_ELR_EL1);
            let esr_el1_after_exit = read_vcpu_sys_reg(vcpu, HV_SYS_REG_ESR_EL1);
            let far_el1_after_exit = read_vcpu_sys_reg(vcpu, HV_SYS_REG_FAR_EL1);
            let spsr_el1_after_exit = read_vcpu_sys_reg(vcpu, HV_SYS_REG_SPSR_EL1);
            let sctlr_el1_after_exit = read_vcpu_sys_reg(vcpu, HV_SYS_REG_SCTLR_EL1);
            let tcr_el1_after_exit = read_vcpu_sys_reg(vcpu, HV_SYS_REG_TCR_EL1);
            let ttbr0_el1_after_exit = read_vcpu_sys_reg(vcpu, HV_SYS_REG_TTBR0_EL1);
            let ttbr1_el1_after_exit = read_vcpu_sys_reg(vcpu, HV_SYS_REG_TTBR1_EL1);
            let mair_el1_after_exit = read_vcpu_sys_reg(vcpu, HV_SYS_REG_MAIR_EL1);
            let sp_el1_after_exit = read_vcpu_sys_reg(vcpu, HV_SYS_REG_SP_EL1);
            let stage1_memory = WindowsArmKnownGuestMemory {
                firmware_memory: firmware_memory.cast_const(),
                vars_memory: vars_memory.cast_const(),
                guest_ram_memory: guest_ram_memory.cast_const(),
                guest_ram_bytes: guest_ram_bytes_usize,
            };
            let stage1_translation = Stage1TranslationContext {
                tcr_el1: tcr_el1_after_exit,
                ttbr0_el1: ttbr0_el1_after_exit,
                memory: stage1_memory,
            };
            let stage1_addresses = Stage1ExitAddresses {
                pc: pc_after_exit,
                vbar_el1: vbar_el1_after_exit,
                elr_el1: elr_el1_after_exit,
                far_el1: far_el1_after_exit,
                sp_el1: sp_el1_after_exit,
            };
            let pc_stage1_leaf_after_exit = read_stage1_leaf_descriptor(
                pc_after_exit,
                tcr_el1_after_exit,
                ttbr0_el1_after_exit,
                firmware_memory.cast_const(),
                vars_memory.cast_const(),
                guest_ram_memory.cast_const(),
                guest_ram_bytes_usize,
            );
            let stage1_descriptor_samples_after_exit =
                collect_stage1_descriptor_samples(stage1_addresses, stage1_translation);
            let stage1_walk_entries_after_exit =
                collect_stage1_walk_entries(stage1_addresses, stage1_translation);
            let stage1_executable_candidates_after_exit = collect_stage1_executable_candidates(
                tcr_el1_after_exit,
                ttbr0_el1_after_exit,
                firmware_memory.cast_const(),
                vars_memory.cast_const(),
                guest_ram_memory.cast_const(),
                guest_ram_bytes_usize,
            );

            let mut run_loop_exit = WindowsArmUefiFirmwareRunLoopExit {
                index,
                run_status: Some(observation.run_status),
                exit_reason: observation.exit_reason,
                exit_syndrome: observation.exit_syndrome,
                exit_exception_class,
                exit_virtual_address: observation.exit_virtual_address,
                exit_physical_address: observation.exit_physical_address,
                pc_after_exit_status,
                pc_after_exit,
                instruction_word_after_exit,
                instruction_hint_after_exit,
                pc_stage1_leaf_level_after_exit: pc_stage1_leaf_after_exit.map(|leaf| leaf.level),
                pc_stage1_leaf_descriptor_after_exit: pc_stage1_leaf_after_exit
                    .map(|leaf| leaf.descriptor),
                pc_stage1_leaf_descriptor_kind_after_exit: pc_stage1_leaf_after_exit
                    .map(|leaf| leaf.kind)
                    .unwrap_or("not observed"),
                pc_stage1_leaf_pxn_after_exit: pc_stage1_leaf_after_exit.map(|leaf| leaf.pxn),
                pc_stage1_leaf_uxn_after_exit: pc_stage1_leaf_after_exit.map(|leaf| leaf.uxn),
                stage1_descriptor_samples_after_exit,
                stage1_walk_entries_after_exit,
                stage1_executable_candidates_after_exit,
                x0_after_exit,
                x1_after_exit,
                x2_after_exit,
                x3_after_exit,
                x4_after_exit,
                cpsr_after_exit,
                vbar_el1_after_exit,
                elr_el1_after_exit,
                esr_el1_after_exit,
                far_el1_after_exit,
                spsr_el1_after_exit,
                sctlr_el1_after_exit,
                tcr_el1_after_exit,
                ttbr0_el1_after_exit,
                ttbr1_el1_after_exit,
                mair_el1_after_exit,
                sp_el1_after_exit,
                watchdog_cancel_status: observation.watchdog_cancel_status,
                vtimer_auto_mask_get_status: None,
                vtimer_auto_mask_after_exit: None,
                vtimer_rearm_cval_value: None,
                vtimer_rearm_cval_set_status: None,
                vtimer_ppi_pending_recorded: None,
                vtimer_irq_line_assertable: None,
                vtimer_gic_group1_enabled: None,
                vtimer_gic_priority_mask: None,
                vtimer_gic_running_priority: None,
                vtimer_gic_priority_threshold: None,
                vtimer_gic_pending_intid: None,
                vtimer_pending_irq_set_status: None,
                vtimer_unmask_status: None,
                handled: false,
            };

            if low_vector_post_repair.continue_attempted
                && low_vector_resume.armed
                && !low_vector_post_repair.first_exit.observed
            {
                low_vector_post_repair.observe_first_exit(&block_devices, &run_loop_exit);
            }
            if low_vector_post_repair.continue_attempted && low_vector_resume.armed {
                low_vector_post_repair.observe_device_interaction(&block_devices, &run_loop_exit);
            }

            if observation.run_status != HV_SUCCESS {
                unsupported_exit_observed = true;
                blockers.push(format!(
                    "firmware run-loop stopped at hv_vcpu_run failure on exit {index}: {:#x}",
                    observation.run_status
                ));
                exits.push(run_loop_exit);
                break;
            }

            if observation.exit_reason.is_none() {
                unsupported_exit_observed = true;
                blockers.push(format!(
                    "firmware run-loop stopped because exit {index} returned no exit info"
                ));
                exits.push(run_loop_exit);
                break;
            }

            if recommended_vector_base_vbar_set
                && !recommended_vector_base_vbar_followup_exit_observed
                && recommended_vector_base_vbar_source_exit_index
                    .is_some_and(|source_index| index > source_index)
            {
                recommended_vector_base_vbar_followup_exit_observed = true;
                recommended_vector_base_vbar_followup_exit_index = Some(index);
                recommended_vector_base_vbar_followup_exit_reason = run_loop_exit.exit_reason;
                recommended_vector_base_vbar_followup_exit_diagnosis =
                    windows_arm_firmware_run_loop_exit_diagnosis(&run_loop_exit);
                recommended_vector_base_vbar_followup_pc = run_loop_exit.pc_after_exit;
                recommended_vector_base_vbar_followup_vbar_el1 = run_loop_exit.vbar_el1_after_exit;
                recommended_vector_base_vbar_followup_target_still_set =
                    recommended_vector_base_vbar_target
                        .zip(run_loop_exit.vbar_el1_after_exit)
                        .is_some_and(|(target, observed)| target == observed);
            }

            if observation.exit_reason == Some(HV_EXIT_REASON_EXCEPTION) {
                let mmio_ipa = observation
                    .exit_physical_address
                    .or(observation.exit_virtual_address);
                if let (Some(syndrome), Some(mmio_ipa), Some(pc)) =
                    (observation.exit_syndrome, mmio_ipa, pc_after_exit)
                {
                    if windows_arm_device_mmio_contains(mmio_ipa) {
                        let Some(mmio_access) = decode_mmio_data_abort(syndrome) else {
                            unsupported_exit_observed = true;
                            blockers.push(format!(
                                "firmware run-loop stopped at undecodable data-abort MMIO exit {index}: syndrome {syndrome:#x}, ipa {mmio_ipa:#x}"
                            ));
                            exits.push(run_loop_exit);
                            break;
                        };

                        let pc_next = pc.saturating_add(4);
                        let pc_status = if mmio_access.is_write {
                            match read_vcpu_reg(vcpu, u32::from(mmio_access.register)) {
                                Some(value) => {
                                    let value = mask_mmio_value(value, mmio_access.width);
                                    let block_queue_notify =
                                        windows_arm_firmware_block_queue_notify_ipa(
                                            &block_devices,
                                            mmio_ipa,
                                        );
                                    let block_irq_source_may_change =
                                        windows_arm_firmware_block_irq_source_may_change(
                                            &block_devices,
                                            mmio_ipa,
                                            value,
                                        );
                                    let gicd_pending_clear_may_need_source_refresh =
                                        windows_arm_firmware_gicd_pending_clear_may_need_source_refresh(
                                            mmio_ipa,
                                            value,
                                            mmio_access.width,
                                        );
                                    match firmware_mmio_bus.dispatch(MmioAccess::write(
                                        mmio_ipa,
                                        value,
                                        mmio_access.width,
                                    )) {
                                        MmioAction::WriteAccepted { .. } => {
                                            if block_queue_notify {
                                                virtio_queue_notify_count += 1;
                                                let completion_result = unsafe {
                                                    let bytes = std::slice::from_raw_parts_mut(
                                                        guest_ram_memory.cast::<u8>(),
                                                        guest_ram_bytes_usize,
                                                    );
                                                    let mut guest_memory = VirtioGuestMemory::new(
                                                        WINDOWS_ARM_GUEST_RAM_IPA,
                                                        bytes,
                                                    );
                                                    complete_windows_arm_firmware_block_queue_notify(
                                                        &mut firmware_mmio_bus,
                                                        &mut guest_memory,
                                                        &block_devices,
                                                        mmio_ipa,
                                                        value,
                                                    )
                                                };
                                                if let Err(error) = completion_result {
                                                    unsupported_exit_observed = true;
                                                    blockers.push(format!(
                                                        "firmware run-loop VirtIO block queue_notify completion failed on exit {index}: {}",
                                                        error.render_blocker()
                                                    ));
                                                    exits.push(run_loop_exit);
                                                    break;
                                                }
                                                virtio_request_completion_count += 1;
                                            }
                                            let irq_delivery =
                                                service_windows_arm_firmware_gic_irq_line_delivery(
                                                    vcpu,
                                                    &mut firmware_mmio_bus,
                                                    &block_devices,
                                                    &gic_cpu_interface,
                                                    device_irq_line_asserted,
                                                    block_irq_source_may_change
                                                        || gicd_pending_clear_may_need_source_refresh,
                                                );
                                            record_windows_arm_firmware_irq_line_delivery(
                                                irq_delivery,
                                                &mut device_irq_line_asserted,
                                                &mut last_device_irq_set_status,
                                                &mut last_device_irq_clear_status,
                                                &mut device_irq_injected_count,
                                                &mut device_irq_cleared_count,
                                            );
                                            if !irq_delivery.succeeded() {
                                                unsupported_exit_observed = true;
                                                blockers.push(irq_delivery.failure_blocker(index));
                                                exits.push(run_loop_exit);
                                                break;
                                            }
                                            unsafe { hv_vcpu_set_reg(vcpu, HV_REG_PC, pc_next) }
                                        }
                                        unexpected_action @ (MmioAction::ReadValue(_)
                                        | MmioAction::Unhandled) => {
                                            let handler_result = match unexpected_action {
                                                MmioAction::ReadValue(_) => {
                                                    "device-bus-returned-read-for-write"
                                                }
                                                MmioAction::Unhandled => {
                                                    "device-bus-unhandled-write"
                                                }
                                                MmioAction::WriteAccepted { .. } => {
                                                    "device-bus-write-accepted"
                                                }
                                            };
                                            if low_vector_post_repair.continue_attempted
                                                && low_vector_resume.armed
                                            {
                                                low_vector_post_repair
                                                    .observe_unhandled_mmio_access(
                                                        &block_devices,
                                                        &run_loop_exit,
                                                        mmio_access,
                                                        mmio_ipa,
                                                        Some(value),
                                                        handler_result,
                                                    );
                                            }
                                            unsupported_exit_observed = true;
                                            blockers.push(format!(
                                                "firmware run-loop MMIO write exit {index} was not handled by the device bus: register X{}, width {}, ipa {mmio_ipa:#x}, value {value:#x}",
                                                mmio_access.register, mmio_access.width
                                            ));
                                            exits.push(run_loop_exit);
                                            break;
                                        }
                                    }
                                }
                                None => {
                                    if low_vector_post_repair.continue_attempted
                                        && low_vector_resume.armed
                                    {
                                        low_vector_post_repair.observe_unhandled_mmio_access(
                                            &block_devices,
                                            &run_loop_exit,
                                            mmio_access,
                                            mmio_ipa,
                                            None,
                                            "write-register-read-failed",
                                        );
                                    }
                                    unsupported_exit_observed = true;
                                    blockers.push(format!(
                                        "firmware run-loop could not read X{} for MMIO write exit {index} at ipa {mmio_ipa:#x}",
                                        mmio_access.register
                                    ));
                                    exits.push(run_loop_exit);
                                    break;
                                }
                            }
                        } else {
                            match firmware_mmio_bus
                                .dispatch(MmioAccess::read(mmio_ipa, mmio_access.width))
                            {
                                MmioAction::ReadValue(value) => {
                                    let value = mask_mmio_value(value, mmio_access.width);
                                    let value_status = unsafe {
                                        hv_vcpu_set_reg(
                                            vcpu,
                                            u32::from(mmio_access.register),
                                            value,
                                        )
                                    };
                                    if value_status == HV_SUCCESS {
                                        unsafe { hv_vcpu_set_reg(vcpu, HV_REG_PC, pc_next) }
                                    } else {
                                        value_status
                                    }
                                }
                                unexpected_action @ (MmioAction::WriteAccepted { .. }
                                | MmioAction::Unhandled) => {
                                    let handler_result = match unexpected_action {
                                        MmioAction::WriteAccepted { .. } => {
                                            "device-bus-returned-write-for-read"
                                        }
                                        MmioAction::Unhandled => "device-bus-unhandled-read",
                                        MmioAction::ReadValue(_) => "device-bus-read-value",
                                    };
                                    if low_vector_post_repair.continue_attempted
                                        && low_vector_resume.armed
                                    {
                                        low_vector_post_repair.observe_unhandled_mmio_access(
                                            &block_devices,
                                            &run_loop_exit,
                                            mmio_access,
                                            mmio_ipa,
                                            None,
                                            handler_result,
                                        );
                                    }
                                    unsupported_exit_observed = true;
                                    blockers.push(format!(
                                        "firmware run-loop MMIO read exit {index} was not handled by the device bus: register X{}, width {}, ipa {mmio_ipa:#x}",
                                        mmio_access.register, mmio_access.width
                                    ));
                                    exits.push(run_loop_exit);
                                    break;
                                }
                            }
                        };

                        if pc_status == HV_SUCCESS {
                            if mmio_access.is_write {
                                handled_mmio_write_count += 1;
                            } else {
                                handled_mmio_read_count += 1;
                            }
                            match windows_arm_firmware_mmio_device_kind(&block_devices, mmio_ipa) {
                                Some(WindowsArmFirmwareMmioDeviceKind::Pl011) => {
                                    handled_pl011_mmio_count += 1;
                                }
                                Some(WindowsArmFirmwareMmioDeviceKind::Pl031) => {
                                    handled_pl031_mmio_count += 1;
                                }
                                Some(WindowsArmFirmwareMmioDeviceKind::GicDistributor) => {
                                    handled_gicd_mmio_count += 1;
                                }
                                Some(WindowsArmFirmwareMmioDeviceKind::GicRedistributor) => {
                                    handled_gicr_mmio_count += 1;
                                }
                                Some(WindowsArmFirmwareMmioDeviceKind::VirtioInstallerIso) => {
                                    handled_virtio_installer_iso_mmio_count += 1;
                                }
                                Some(WindowsArmFirmwareMmioDeviceKind::VirtioTargetDisk) => {
                                    handled_virtio_target_disk_mmio_count += 1;
                                }
                                None => {}
                            }
                            run_loop_exit.handled = true;
                            exits.push(run_loop_exit);
                            if stop_at_first_post_repair_device_boundary
                                && low_vector_post_repair.first_device_interaction_is(index)
                            {
                                break;
                            }
                            continue;
                        }

                        unsupported_exit_observed = true;
                        blockers.push(format!(
                            "firmware run-loop failed to resume after MMIO {} exit {index}: register X{}, width {}, ipa {mmio_ipa:#x}, hv_vcpu_set_reg(PC={pc_next:#x})={pc_status:#x}",
                            mmio_access.access_name(),
                            mmio_access.register,
                            mmio_access.width
                        ));
                        exits.push(run_loop_exit);
                        break;
                    }
                }

                if let (Some(syndrome), Some(pc)) = (observation.exit_syndrome, pc_after_exit) {
                    if let Some(sysreg_access) = decode_system_register_trap(syndrome) {
                        let write_value = if sysreg_access.is_read {
                            None
                        } else if sysreg_access.register == 31 {
                            Some(0)
                        } else {
                            match read_vcpu_reg(vcpu, u32::from(sysreg_access.register)) {
                                Some(value) => Some(value),
                                None => {
                                    if low_vector_post_repair.continue_attempted
                                        && low_vector_resume.armed
                                    {
                                        low_vector_post_repair.observe_unhandled_sysreg_access(
                                            &run_loop_exit,
                                            sysreg_access,
                                            None,
                                            "sysreg-write-register-read-failed",
                                        );
                                    }
                                    unsupported_exit_observed = true;
                                    blockers.push(format!(
                                        "firmware run-loop could not read X{} for GIC CPU-interface sysreg write exit {index}: sys_reg={:#x}",
                                        sysreg_access.register, sysreg_access.sys_reg
                                    ));
                                    exits.push(run_loop_exit);
                                    break;
                                }
                            }
                        };

                        let Some(gic_action) = gic_cpu_interface.handle_system_register_access(
                            &mut firmware_mmio_bus,
                            sysreg_access,
                            write_value,
                        ) else {
                            if low_vector_post_repair.continue_attempted && low_vector_resume.armed
                            {
                                low_vector_post_repair.observe_unhandled_sysreg_access(
                                    &run_loop_exit,
                                    sysreg_access,
                                    write_value,
                                    "sysreg-unhandled",
                                );
                            }
                            unsupported_exit_observed = true;
                            blockers.push(format!(
                                "firmware run-loop GIC CPU-interface sysreg {} exit {index} was not handled: sys_reg={:#x}, op0={}, op1={}, crn={}, crm={}, op2={}, rt=X{}",
                                sysreg_access.access_name(),
                                sysreg_access.sys_reg,
                                sysreg_access.op0,
                                sysreg_access.op1,
                                sysreg_access.crn,
                                sysreg_access.crm,
                                sysreg_access.op2,
                                sysreg_access.register,
                            ));
                            exits.push(run_loop_exit);
                            break;
                        };

                        let pc_next = pc.saturating_add(4);
                        let value_status = match gic_action {
                            GicV3CpuInterfaceAction::Read(value) => {
                                if sysreg_access.register == 31 {
                                    HV_SUCCESS
                                } else {
                                    unsafe {
                                        hv_vcpu_set_reg(
                                            vcpu,
                                            u32::from(sysreg_access.register),
                                            value,
                                        )
                                    }
                                }
                            }
                            GicV3CpuInterfaceAction::Write { .. } => HV_SUCCESS,
                        };
                        if value_status != HV_SUCCESS {
                            unsupported_exit_observed = true;
                            blockers.push(format!(
                                "firmware run-loop failed to inject GIC CPU-interface sysreg read value on exit {index}: sys_reg={:#x}, rt=X{}, hv_vcpu_set_reg={value_status:#x}",
                                sysreg_access.sys_reg, sysreg_access.register
                            ));
                            exits.push(run_loop_exit);
                            break;
                        }

                        if sysreg_access.is_read {
                            handled_icc_read_count += 1;
                            if sysreg_access.sys_reg == ICC_IAR1_EL1_SYSREG {
                                handled_icc_iar1_read_count += 1;
                                if let GicV3CpuInterfaceAction::Read(value) = gic_action {
                                    last_icc_iar1_intid = Some((value & 0x00ff_ffff) as u32);
                                }
                            }
                        } else {
                            handled_icc_write_count += 1;
                            match sysreg_access.sys_reg {
                                ICC_EOIR1_EL1_SYSREG => {
                                    handled_icc_eoir1_write_count += 1;
                                    last_icc_eoir1_intid =
                                        write_value.map(|value| (value & 0x00ff_ffff) as u32);
                                }
                                ICC_DIR_EL1_SYSREG => {
                                    handled_icc_dir_write_count += 1;
                                    last_icc_dir_intid =
                                        write_value.map(|value| (value & 0x00ff_ffff) as u32);
                                }
                                _ => {}
                            }
                        }

                        let GicV3CpuInterfaceAction::Write {
                            refresh_level_sources,
                        } = gic_action
                        else {
                            let irq_delivery = service_windows_arm_firmware_gic_irq_line_delivery(
                                vcpu,
                                &mut firmware_mmio_bus,
                                &block_devices,
                                &gic_cpu_interface,
                                device_irq_line_asserted,
                                false,
                            );
                            record_windows_arm_firmware_irq_line_delivery(
                                irq_delivery,
                                &mut device_irq_line_asserted,
                                &mut last_device_irq_set_status,
                                &mut last_device_irq_clear_status,
                                &mut device_irq_injected_count,
                                &mut device_irq_cleared_count,
                            );
                            if !irq_delivery.succeeded() {
                                unsupported_exit_observed = true;
                                blockers.push(irq_delivery.failure_blocker(index));
                                exits.push(run_loop_exit);
                                break;
                            }
                            let pc_status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_PC, pc_next) };
                            if pc_status == HV_SUCCESS {
                                run_loop_exit.handled = true;
                                exits.push(run_loop_exit);
                                if stop_at_first_post_repair_device_boundary
                                    && low_vector_post_repair.first_device_interaction_is(index)
                                {
                                    break;
                                }
                                continue;
                            }
                            unsupported_exit_observed = true;
                            blockers.push(format!(
                                "firmware run-loop failed to advance PC after GIC CPU-interface sysreg read exit {index}: sys_reg={:#x}, hv_vcpu_set_reg(PC={pc_next:#x})={pc_status:#x}",
                                sysreg_access.sys_reg
                            ));
                            exits.push(run_loop_exit);
                            break;
                        };

                        let irq_delivery = service_windows_arm_firmware_gic_irq_line_delivery(
                            vcpu,
                            &mut firmware_mmio_bus,
                            &block_devices,
                            &gic_cpu_interface,
                            device_irq_line_asserted,
                            refresh_level_sources,
                        );
                        record_windows_arm_firmware_irq_line_delivery(
                            irq_delivery,
                            &mut device_irq_line_asserted,
                            &mut last_device_irq_set_status,
                            &mut last_device_irq_clear_status,
                            &mut device_irq_injected_count,
                            &mut device_irq_cleared_count,
                        );
                        if !irq_delivery.succeeded() {
                            unsupported_exit_observed = true;
                            blockers.push(irq_delivery.failure_blocker(index));
                            exits.push(run_loop_exit);
                            break;
                        }
                        let pc_status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_PC, pc_next) };
                        if pc_status == HV_SUCCESS {
                            run_loop_exit.handled = true;
                            exits.push(run_loop_exit);
                            if stop_at_first_post_repair_device_boundary
                                && low_vector_post_repair.first_device_interaction_is(index)
                            {
                                break;
                            }
                            continue;
                        }

                        unsupported_exit_observed = true;
                        blockers.push(format!(
                            "firmware run-loop failed to advance PC after GIC CPU-interface sysreg write exit {index}: sys_reg={:#x}, hv_vcpu_set_reg(PC={pc_next:#x})={pc_status:#x}",
                            sysreg_access.sys_reg
                        ));
                        exits.push(run_loop_exit);
                        break;
                    }
                }
            }

            if wire_interrupt_timer
                && observation.exit_reason == Some(HV_EXIT_REASON_VTIMER_ACTIVATED)
            {
                vtimer_exit_count += 1;
                let mut auto_masked = false;
                let mask_status = unsafe { hv_vcpu_get_vtimer_mask(vcpu, &mut auto_masked) };
                run_loop_exit.vtimer_auto_mask_get_status = Some(mask_status);
                if mask_status == HV_SUCCESS {
                    run_loop_exit.vtimer_auto_mask_after_exit = Some(auto_masked);
                } else {
                    unsupported_exit_observed = true;
                    blockers.push(format!(
                        "firmware run-loop failed to inspect VTimer auto-mask after exit {index}: hv_vcpu_get_vtimer_mask={mask_status:#x}"
                    ));
                    exits.push(run_loop_exit);
                    break;
                }

                if !auto_masked {
                    unsupported_exit_observed = true;
                    blockers.push(format!(
                        "firmware run-loop VTimer exit {index} was not automatically masked before IRQ injection"
                    ));
                    exits.push(run_loop_exit);
                    break;
                }

                let low_vector_fault =
                    windows_arm_firmware_run_loop_exit_diagnosis_kind(&run_loop_exit)
                        == WindowsArmFirmwareRunLoopDiagnosis::El1LowVectorMmuTranslationFault;
                let defer_to_low_vector_repair = repair_low_vector_diagnostic_page
                    && low_vector_fault
                    && !low_vector_diagnostic_page_repaired;
                let defer_to_low_vector_repeat_guard = repair_low_vector_diagnostic_page
                    && low_vector_fault
                    && low_vector_diagnostic_page_repaired;
                let delivery = service_windows_arm_firmware_vtimer_delivery(
                    vcpu,
                    &mut firmware_mmio_bus,
                    &gic_cpu_interface,
                    device_irq_line_asserted,
                    defer_to_low_vector_repair,
                );
                run_loop_exit.vtimer_rearm_cval_value = Some(delivery.rearm_cval_value);
                run_loop_exit.vtimer_rearm_cval_set_status = Some(delivery.rearm_cval_status);
                run_loop_exit.vtimer_ppi_pending_recorded = Some(delivery.ppi_pending_recorded);
                run_loop_exit.vtimer_irq_line_assertable = Some(delivery.irq_line_should_assert);
                run_loop_exit.vtimer_gic_group1_enabled =
                    Some(delivery.irq_line_snapshot.group1_enabled);
                run_loop_exit.vtimer_gic_priority_mask =
                    Some(delivery.irq_line_snapshot.priority_mask);
                run_loop_exit.vtimer_gic_running_priority =
                    Some(delivery.irq_line_snapshot.running_priority);
                run_loop_exit.vtimer_gic_priority_threshold =
                    Some(delivery.irq_line_snapshot.priority_threshold);
                run_loop_exit.vtimer_gic_pending_intid =
                    Some(delivery.irq_line_snapshot.pending_intid);
                run_loop_exit.vtimer_pending_irq_set_status = delivery.pending_irq_status;
                run_loop_exit.vtimer_unmask_status = delivery.unmask_status;

                if let Some(irq_status) = delivery.pending_irq_status {
                    last_pending_irq_set_status = Some(irq_status);
                    if delivery.irq_line_should_assert {
                        last_device_irq_set_status = Some(irq_status);
                    } else {
                        last_device_irq_clear_status = Some(irq_status);
                    }
                }
                if delivery.device_irq_injected {
                    device_irq_injected_count += 1;
                }
                if delivery.device_irq_cleared {
                    device_irq_cleared_count += 1;
                }
                device_irq_line_asserted = delivery.next_device_irq_line_asserted;
                if let Some(unmask_status) = delivery.unmask_status {
                    last_vtimer_unmask_status = Some(unmask_status);
                }

                if delivery.succeeded() {
                    if delivery.pending_irq_injected() {
                        pending_irq_injected_count += 1;
                    }
                    if defer_to_low_vector_repair || defer_to_low_vector_repeat_guard {
                        // The timer boundary is serviced, but the same snapshot also
                        // exposes the low-vector fault. Let the repair/repeat-fault
                        // handlers below decide whether to patch or stop with telemetry.
                    } else {
                        run_loop_exit.handled = true;
                        exits.push(run_loop_exit);
                        if delivery.unmask_status.is_some() {
                            continue;
                        }
                        break;
                    }
                } else {
                    unsupported_exit_observed = true;
                    blockers.push(delivery.failure_blocker(index));
                    exits.push(run_loop_exit);
                    break;
                }
            }

            if try_recommended_vector_base_vbar
                && !remap_low_vector_to_recommended_vector
                && !diagnostic_vector_seed_requested
                && (!repair_low_vector_diagnostic_page
                    || continue_after_recommended_vector_base_vbar)
                && !recommended_vector_base_vbar_attempted
                && windows_arm_firmware_run_loop_exit_diagnosis_kind(&run_loop_exit)
                    == WindowsArmFirmwareRunLoopDiagnosis::El1LowVectorMmuTranslationFault
            {
                if let Some(recommendation) =
                    recommended_vector_base_vbar_redirect_target(&run_loop_exit)
                {
                    recommended_vector_base_vbar_attempted = true;
                    recommended_vector_base_vbar_source_exit_index = Some(index);
                    recommended_vector_base_vbar_target = Some(recommendation.base_virtual_address);
                    recommended_vector_base_vbar_target_physical_address =
                        recommendation.base_physical_address;
                    recommended_vector_base_vbar_reason = recommendation.reason;
                    recommended_vector_base_vbar_current_el_spx_sync_instruction_word =
                        recommendation.current_el_spx_sync_instruction_word;
                    recommended_vector_base_vbar_current_el_spx_sync_instruction_hint =
                        recommendation.current_el_spx_sync_instruction_hint;
                    if continue_after_recommended_vector_base_vbar {
                        recommended_vector_base_vbar_resume_original_pc = pc_after_exit;
                        recommended_vector_base_vbar_resume_original_elr_el1 = elr_el1_after_exit;
                        recommended_vector_base_vbar_resume_original_esr_el1 = esr_el1_after_exit;
                        recommended_vector_base_vbar_resume_original_far_el1 = far_el1_after_exit;
                        recommended_vector_base_vbar_resume_original_spsr_el1 = spsr_el1_after_exit;
                    }
                    recommended_vector_base_vbar_diagnostic_vector_populated =
                        populate_recommended_vector_base_diagnostic_vector_slot(
                            recommendation,
                            firmware_memory,
                            vars_memory,
                            guest_ram_memory,
                            slot_bytes_usize,
                            guest_ram_bytes_usize,
                            &mut blockers,
                        );
                    if !recommended_vector_base_vbar_diagnostic_vector_populated {
                        unsupported_exit_observed = true;
                        blockers.push(format!(
                            "firmware run-loop could not seed diagnostic vector at recommended vector base on exit {index}: target={:#x}, target_pa={}",
                            recommendation.base_virtual_address,
                            crate::render_optional_u64(recommendation.base_physical_address)
                        ));
                        exits.push(run_loop_exit);
                        break;
                    }
                    diagnostic_vector_populated = true;
                    if repair_low_vector_diagnostic_page {
                        low_vector_resume.capture_original_context(&run_loop_exit);
                        let low_vector_repair = prepare_low_vector_diagnostic_page_repair(
                            LowVectorDiagnosticPageRepairRequest {
                                firmware_memory,
                                vars_memory,
                                guest_ram_memory,
                                slot_bytes: slot_bytes_usize,
                                guest_ram_bytes: guest_ram_bytes_usize,
                                tcr_el1: tcr_el1_after_exit,
                                ttbr0_el1: ttbr0_el1_after_exit,
                                location:
                                    "recommended-vector VBAR low-vector diagnostic page repair",
                                blockers: &mut blockers,
                            },
                        );
                        low_vector_diagnostic_page_slot_snapshot =
                            low_vector_repair.diagnostic_slot_snapshot;
                        low_vector_resume.capture_diagnostic_slot_bytes(
                            low_vector_diagnostic_page_slot_snapshot
                                .map(|snapshot| snapshot.original),
                        );
                        let low_vector_populated = low_vector_repair.vector_populated();
                        if let Some((entry_ipa, previous_descriptor)) =
                            low_vector_repair.patched_descriptor
                        {
                            low_vector_diagnostic_page_entry_ipa = Some(entry_ipa);
                            low_vector_diagnostic_page_previous_descriptor =
                                Some(previous_descriptor);
                            low_vector_diagnostic_page_descriptor =
                                Some(WINDOWS_ARM_LOW_VECTOR_DIAGNOSTIC_PAGE_DESCRIPTOR);
                            low_vector_diagnostic_page_repaired = low_vector_populated;
                        }
                        if !low_vector_diagnostic_page_repaired {
                            unsupported_exit_observed = true;
                            blockers.push(
                                "firmware run-loop could not prepare low-vector diagnostic page repair before recommended-vector original-context resume"
                                    .to_string(),
                            );
                            exits.push(run_loop_exit);
                            break;
                        }
                    }
                    let vbar_status = unsafe {
                        hv_vcpu_set_sys_reg(
                            vcpu,
                            HV_SYS_REG_VBAR_EL1,
                            recommendation.base_virtual_address,
                        )
                    };
                    recommended_vector_base_vbar_set_status = Some(vbar_status);
                    recommended_vector_base_vbar_set = vbar_status == HV_SUCCESS;
                    if recommended_vector_base_vbar_set {
                        run_loop_exit.handled = true;
                        if let Some(blocker) = &watchdog_blocker {
                            blockers.retain(|candidate| candidate != blocker);
                        }
                        exits.push(run_loop_exit);
                        continue;
                    }

                    unsupported_exit_observed = true;
                    blockers.push(format!(
                        "firmware run-loop failed to set VBAR_EL1 to recommended vector base on exit {index}: target={:#x}, hv_vcpu_set_sys_reg={vbar_status:#x}",
                        recommendation.base_virtual_address
                    ));
                    exits.push(run_loop_exit);
                    break;
                }

                recommended_vector_base_vbar_reason = "no recommended vector base candidate";
            }

            if let Some(target) = recommended_vector_base_vbar_target {
                let route = recommended_vector_base_diagnostic_route(target);
                if let Some((eret_pc, landing_pc)) =
                    diagnostic_vector_hvc_eret_recovery_target(&run_loop_exit, route)
                {
                    if continue_after_recommended_vector_base_vbar {
                        if recommended_vector_base_vbar_resume_attempted {
                            unsupported_exit_observed = true;
                            blockers.push(format!(
                                "firmware run-loop repeated recommended-vector diagnostic HVC after original-context resume on exit {index}: original ELR_EL1={}, original SPSR_EL1={}",
                                crate::render_optional_u64(
                                    recommended_vector_base_vbar_resume_original_elr_el1,
                                ),
                                crate::render_optional_u64(
                                    recommended_vector_base_vbar_resume_original_spsr_el1,
                                )
                            ));
                            exits.push(run_loop_exit);
                            break;
                        }
                        recommended_vector_base_vbar_resume_attempted = true;
                        let Some(original_elr_el1) =
                            recommended_vector_base_vbar_resume_original_elr_el1
                        else {
                            unsupported_exit_observed = true;
                            blockers.push(
                                "firmware run-loop reached recommended-vector diagnostic HVC, but original ELR_EL1 was not captured before VBAR redirect"
                                    .to_string(),
                            );
                            exits.push(run_loop_exit);
                            break;
                        };
                        let Some(original_spsr_el1) =
                            recommended_vector_base_vbar_resume_original_spsr_el1
                        else {
                            unsupported_exit_observed = true;
                            blockers.push(
                                "firmware run-loop reached recommended-vector diagnostic HVC, but original SPSR_EL1 was not captured before VBAR redirect"
                                    .to_string(),
                            );
                            exits.push(run_loop_exit);
                            break;
                        };

                        let resume_status = resume_diagnostic_eret_to_original_context(
                            vcpu,
                            original_elr_el1,
                            original_spsr_el1,
                            eret_pc,
                            repair_low_vector_diagnostic_page
                                && low_vector_diagnostic_page_repaired,
                        );
                        let elr_status = resume_status.elr_status;
                        let vbar_status = resume_status.vbar_effective_status();
                        let spsr_status = resume_status.spsr_status;
                        let pc_status = resume_status.pc_status;
                        recommended_vector_base_vbar_resume_elr_el1_set_status = Some(elr_status);
                        recommended_vector_base_vbar_resume_vbar_el1_set_status =
                            resume_status.vbar_status;
                        recommended_vector_base_vbar_resume_spsr_el1_set_status = Some(spsr_status);
                        recommended_vector_base_vbar_resume_pc_set_status = Some(pc_status);

                        if resume_status.succeeded() {
                            recommended_vector_base_vbar_resume_armed = true;
                            run_loop_exit.handled = true;
                            if let Some(blocker) = &watchdog_blocker {
                                blockers.retain(|candidate| candidate != blocker);
                            }
                            exits.push(run_loop_exit);
                            continue;
                        }

                        unsupported_exit_observed = true;
                        blockers.push(format!(
                            "firmware run-loop failed to arm recommended-vector diagnostic ERET resume to original context: hv_vcpu_set_sys_reg(ELR_EL1={original_elr_el1:#x})={elr_status:#x}, hv_vcpu_set_sys_reg(VBAR_EL1=0x0)={vbar_status:#x}, hv_vcpu_set_sys_reg(SPSR_EL1={original_spsr_el1:#x})={spsr_status:#x}, hv_vcpu_set_reg(PC={eret_pc:#x})={pc_status:#x}"
                        ));
                        exits.push(run_loop_exit);
                        break;
                    }

                    let route_status =
                        route_diagnostic_hvc_exit_through_eret_landing(vcpu, eret_pc, landing_pc);
                    let elr_status = route_status.elr_status;
                    let pc_status = route_status.pc_status;
                    if route_status.succeeded() {
                        run_loop_exit.handled = true;
                        exits.push(run_loop_exit);
                        continue;
                    }

                    unsupported_exit_observed = true;
                    blockers.push(format!(
                        "firmware run-loop failed to route recommended-vector diagnostic HVC exit {index} through ERET landing: hv_vcpu_set_sys_reg(ELR_EL1={landing_pc:#x})={elr_status:#x}, hv_vcpu_set_reg(PC={eret_pc:#x})={pc_status:#x}"
                    ));
                    exits.push(run_loop_exit);
                    break;
                }

                if diagnostic_vector_eret_landing_stop(&run_loop_exit, route) {
                    exits.push(run_loop_exit);
                    break;
                }
            }

            if let Some((eret_pc, landing_pc)) =
                executable_diagnostic_vector_hvc_eret_recovery_target(&run_loop_exit)
            {
                let route_status =
                    route_diagnostic_hvc_exit_through_eret_landing(vcpu, eret_pc, landing_pc);
                let elr_status = route_status.elr_status;
                let pc_status = route_status.pc_status;
                if route_status.succeeded() {
                    run_loop_exit.handled = true;
                    exits.push(run_loop_exit);
                    continue;
                }

                unsupported_exit_observed = true;
                blockers.push(format!(
                    "firmware run-loop failed to route executable diagnostic HVC exit {index} through ERET landing: hv_vcpu_set_sys_reg(ELR_EL1={landing_pc:#x})={elr_status:#x}, hv_vcpu_set_reg(PC={eret_pc:#x})={pc_status:#x}"
                ));
                exits.push(run_loop_exit);
                break;
            }

            if executable_diagnostic_vector_eret_landing_stop(&run_loop_exit) {
                exits.push(run_loop_exit);
                break;
            }

            if repair_low_vector_diagnostic_page
                && low_vector_diagnostic_page_repaired
                && windows_arm_firmware_run_loop_exit_diagnosis_kind(&run_loop_exit)
                    == WindowsArmFirmwareRunLoopDiagnosis::El1LowVectorMmuTranslationFault
            {
                low_vector_diagnostic_page_repeated_fault_observed = true;
                if continue_after_low_vector_repair {
                    low_vector_post_repair.observe_unsupported_exit(&run_loop_exit);
                }
                unsupported_exit_observed = true;
                blockers.push(format!(
                    "firmware run-loop saw a repeated low-vector stage-1 translation fault after diagnostic page repair: entry_ipa={}, previous_descriptor={}, patched_descriptor={}",
                    crate::render_optional_u64(low_vector_diagnostic_page_entry_ipa),
                    crate::render_optional_u64(low_vector_diagnostic_page_previous_descriptor),
                    crate::render_optional_u64(low_vector_diagnostic_page_descriptor)
                ));
                exits.push(run_loop_exit);
                break;
            }

            if repair_low_vector_diagnostic_page
                && !low_vector_diagnostic_page_repaired
                && windows_arm_firmware_run_loop_exit_diagnosis_kind(&run_loop_exit)
                    == WindowsArmFirmwareRunLoopDiagnosis::El1LowVectorMmuTranslationFault
            {
                low_vector_resume.capture_original_context(&run_loop_exit);
                if remap_low_vector_to_recommended_vector && continue_after_low_vector_repair {
                    low_vector_recommended_vector_remap_attempted = true;
                    if let Some(recommendation) =
                        low_vector_recommended_vector_remap_target(&run_loop_exit)
                    {
                        low_vector_recommended_vector_remap_target_physical_address =
                            recommendation.base_physical_address;
                        if let (Some(original_elr_el1), Some(original_spsr_el1)) = (
                            low_vector_resume.original_elr_el1,
                            low_vector_resume.original_spsr_el1,
                        ) {
                            if let Some((entry_ipa, previous_descriptor, descriptor)) =
                                patch_low_vector_recommended_vector_descriptor(
                                    recommendation,
                                    tcr_el1_after_exit,
                                    ttbr0_el1_after_exit,
                                    firmware_memory,
                                    vars_memory,
                                    guest_ram_memory,
                                    guest_ram_bytes_usize,
                                )
                            {
                                low_vector_diagnostic_page_entry_ipa = Some(entry_ipa);
                                low_vector_diagnostic_page_previous_descriptor =
                                    Some(previous_descriptor);
                                low_vector_diagnostic_page_descriptor = Some(descriptor);
                                low_vector_recommended_vector_remap_descriptor = Some(descriptor);
                                let cpsr_status = unsafe {
                                    hv_vcpu_set_reg(vcpu, HV_REG_CPSR, original_spsr_el1)
                                };
                                let pc_status = if cpsr_status == HV_SUCCESS {
                                    unsafe { hv_vcpu_set_reg(vcpu, HV_REG_PC, original_elr_el1) }
                                } else {
                                    cpsr_status
                                };
                                low_vector_resume
                                    .record_direct_resume_status(cpsr_status, pc_status);

                                if cpsr_status == HV_SUCCESS && pc_status == HV_SUCCESS {
                                    low_vector_recommended_vector_remap_succeeded = true;
                                    low_vector_diagnostic_page_repaired = true;
                                    low_vector_resume.mark_attempted();
                                    low_vector_resume.mark_armed();
                                    low_vector_post_repair.mark_continue_attempted();
                                    run_loop_exit.handled = true;
                                    if let Some(blocker) = &watchdog_blocker {
                                        blockers.retain(|candidate| candidate != blocker);
                                    }
                                    exits.push(run_loop_exit);
                                    continue;
                                }

                                unsupported_exit_observed = true;
                                blockers.push(format!(
                                    "firmware run-loop patched low-vector descriptor at {entry_ipa:#x} from {previous_descriptor:#x} to recommended-vector descriptor {descriptor:#x}, but failed to resume original context directly: hv_vcpu_set_reg(CPSR={original_spsr_el1:#x})={cpsr_status:#x}, hv_vcpu_set_reg(PC={original_elr_el1:#x})={pc_status:#x}"
                                ));
                                exits.push(run_loop_exit);
                                break;
                            }
                        }
                    }
                }
                let low_vector_repair = prepare_low_vector_diagnostic_page_repair(
                    LowVectorDiagnosticPageRepairRequest {
                        firmware_memory,
                        vars_memory,
                        guest_ram_memory,
                        slot_bytes: slot_bytes_usize,
                        guest_ram_bytes: guest_ram_bytes_usize,
                        tcr_el1: tcr_el1_after_exit,
                        ttbr0_el1: ttbr0_el1_after_exit,
                        location: "low-vector diagnostic page repair",
                        blockers: &mut blockers,
                    },
                );
                low_vector_diagnostic_page_slot_snapshot =
                    low_vector_repair.diagnostic_slot_snapshot;
                low_vector_resume.capture_diagnostic_slot_bytes(
                    low_vector_diagnostic_page_slot_snapshot.map(|snapshot| snapshot.original),
                );
                let vector_populated = low_vector_repair.vector_populated();
                if vector_populated {
                    diagnostic_vector_populated = true;
                }
                if let Some((entry_ipa, previous_descriptor)) = low_vector_repair.patched_descriptor
                {
                    low_vector_diagnostic_page_entry_ipa = Some(entry_ipa);
                    low_vector_diagnostic_page_previous_descriptor = Some(previous_descriptor);
                    low_vector_diagnostic_page_descriptor =
                        Some(WINDOWS_ARM_LOW_VECTOR_DIAGNOSTIC_PAGE_DESCRIPTOR);
                    let pc_status = unsafe {
                        hv_vcpu_set_reg(
                            vcpu,
                            HV_REG_PC,
                            WINDOWS_ARM_DIAGNOSTIC_VECTOR_CURRENT_EL_SPX_SYNC_OFFSET as u64,
                        )
                    };
                    if vector_populated && pc_status == HV_SUCCESS {
                        low_vector_diagnostic_page_repaired = true;
                        run_loop_exit.handled = true;
                        if let Some(blocker) = &watchdog_blocker {
                            blockers.retain(|candidate| candidate != blocker);
                        }
                        exits.push(run_loop_exit);
                        continue;
                    }
                    unsupported_exit_observed = true;
                    blockers.push(format!(
                        "firmware run-loop patched low-vector page descriptor at {entry_ipa:#x} from {previous_descriptor:#x} to {:#x}, but failed to resume at the low vector: vector_populated={vector_populated}, hv_vcpu_set_reg(PC=0x200)={pc_status:#x}",
                        WINDOWS_ARM_LOW_VECTOR_DIAGNOSTIC_PAGE_DESCRIPTOR
                    ));
                    exits.push(run_loop_exit);
                    break;
                }

                unsupported_exit_observed = true;
                blockers.push(
                    "firmware run-loop could not find or patch the low-vector stage-1 L3 descriptor for diagnostic page repair"
                        .to_string(),
                );
                exits.push(run_loop_exit);
                break;
            }

            if let Some((eret_pc, landing_pc)) =
                low_vector_diagnostic_page_hvc_eret_recovery_target(&run_loop_exit)
            {
                let route_status =
                    route_diagnostic_hvc_exit_through_eret_landing(vcpu, eret_pc, landing_pc);
                let elr_status = route_status.elr_status;
                let pc_status = route_status.pc_status;
                if route_status.succeeded() {
                    run_loop_exit.handled = true;
                    exits.push(run_loop_exit);
                    continue;
                }

                unsupported_exit_observed = true;
                blockers.push(format!(
                    "firmware run-loop failed to route low-vector diagnostic HVC exit {index} through ERET landing: hv_vcpu_set_sys_reg(ELR_EL1={landing_pc:#x})={elr_status:#x}, hv_vcpu_set_reg(PC={eret_pc:#x})={pc_status:#x}"
                ));
                exits.push(run_loop_exit);
                break;
            }

            if low_vector_diagnostic_page_eret_landing_stop(&run_loop_exit) {
                if repair_low_vector_diagnostic_page
                    && low_vector_diagnostic_page_repaired
                    && !low_vector_resume.attempted
                {
                    low_vector_resume.mark_attempted();
                    if continue_after_low_vector_repair {
                        low_vector_post_repair.mark_continue_attempted();
                    }
                    let Some(original_elr_el1) = low_vector_resume.original_elr_el1 else {
                        unsupported_exit_observed = true;
                        blockers.push(
                            "firmware run-loop reached low-vector diagnostic ERET landing, but original ELR_EL1 was not captured before repair"
                                .to_string(),
                        );
                        exits.push(run_loop_exit);
                        break;
                    };
                    let Some(original_spsr_el1) = low_vector_resume.original_spsr_el1 else {
                        unsupported_exit_observed = true;
                        blockers.push(
                            "firmware run-loop reached low-vector diagnostic ERET landing, but original SPSR_EL1 was not captured before repair"
                                .to_string(),
                        );
                        exits.push(run_loop_exit);
                        break;
                    };

                    let mut eret_pc = low_vector_diagnostic_page_route().eret_pc();
                    if restore_low_vector_slot_before_eret && continue_after_low_vector_repair {
                        low_vector_diagnostic_page_restore_before_eret_attempted = true;
                        let Some(snapshot) = low_vector_diagnostic_page_slot_snapshot else {
                            unsupported_exit_observed = true;
                            blockers.push(
                                "firmware run-loop requested low-vector slot restore before ERET, but no preserved low-vector diagnostic slot snapshot was captured"
                                    .to_string(),
                            );
                            exits.push(run_loop_exit);
                            break;
                        };
                        let trampoline_snapshot =
                            install_diagnostic_exception_vector_slot_preserving(
                                firmware_memory,
                                slot_bytes_usize,
                                WINDOWS_ARM_EXECUTABLE_DIAGNOSTIC_VECTOR_IPA as usize,
                                "executable ERET trampoline for low-vector restore",
                                &mut blockers,
                            );
                        if trampoline_snapshot.is_none() {
                            unsupported_exit_observed = true;
                            blockers.push(
                                "firmware run-loop could not populate executable ERET trampoline before low-vector slot restore"
                                    .to_string(),
                            );
                            exits.push(run_loop_exit);
                            break;
                        }
                        if !restore_diagnostic_exception_vector_slot(
                            firmware_memory,
                            slot_bytes_usize,
                            snapshot,
                            "low-vector diagnostic page restore before ERET",
                            &mut blockers,
                        ) {
                            unsupported_exit_observed = true;
                            blockers.push(
                                "firmware run-loop failed to restore the preserved low-vector slot before ERET"
                                    .to_string(),
                            );
                            exits.push(run_loop_exit);
                            break;
                        }
                        low_vector_diagnostic_page_slot_restored = true;
                        eret_pc = executable_diagnostic_vector_route().eret_pc();
                    }

                    let resume_target_instruction_before_eret = read_guest_instruction_word(
                        Some(original_elr_el1),
                        firmware_memory.cast_const(),
                        vars_memory.cast_const(),
                        guest_ram_memory.cast_const(),
                        guest_ram_bytes_usize,
                    );
                    let resume_target_stage1_leaf_before_eret = read_stage1_leaf_descriptor(
                        Some(original_elr_el1),
                        tcr_el1_after_exit,
                        ttbr0_el1_after_exit,
                        firmware_memory.cast_const(),
                        vars_memory.cast_const(),
                        guest_ram_memory.cast_const(),
                        guest_ram_bytes_usize,
                    );
                    low_vector_resume.record_eret_target_snapshot(
                        resume_target_instruction_before_eret,
                        resume_target_stage1_leaf_before_eret.map(|leaf| leaf.descriptor),
                        resume_target_stage1_leaf_before_eret
                            .map(|leaf| leaf.kind)
                            .unwrap_or("not observed"),
                    );
                    let resume_status = arm_diagnostic_eret_resume(
                        vcpu,
                        &mut low_vector_resume,
                        original_elr_el1,
                        original_spsr_el1,
                        eret_pc,
                    );
                    let elr_status = resume_status.elr_status;
                    let spsr_status = resume_status.spsr_status;
                    let pc_status = resume_status.pc_status;

                    if resume_status.succeeded() {
                        run_loop_exit.handled = true;
                        exits.push(run_loop_exit);
                        continue;
                    }

                    unsupported_exit_observed = true;
                    if continue_after_low_vector_repair {
                        blockers.push(format!(
                            "firmware run-loop failed to keep the repaired low-vector diagnostic page installed and arm ERET resume to original context: hv_vcpu_set_sys_reg(ELR_EL1={original_elr_el1:#x})={elr_status:#x}, hv_vcpu_set_sys_reg(SPSR_EL1={original_spsr_el1:#x})={spsr_status:#x}, hv_vcpu_set_reg(PC={eret_pc:#x})={pc_status:#x}"
                        ));
                    } else {
                        blockers.push(format!(
                            "firmware run-loop failed to arm low-vector diagnostic ERET resume to original context: hv_vcpu_set_sys_reg(ELR_EL1={original_elr_el1:#x})={elr_status:#x}, hv_vcpu_set_sys_reg(SPSR_EL1={original_spsr_el1:#x})={spsr_status:#x}, hv_vcpu_set_reg(PC={eret_pc:#x})={pc_status:#x}"
                        ));
                    }
                    exits.push(run_loop_exit);
                    break;
                }
                exits.push(run_loop_exit);
                break;
            }

            let reason_name = observation
                .exit_reason
                .map(hv_exit_reason_name)
                .unwrap_or("not observed");
            let exception_class_name = exit_exception_class
                .map(arm_exception_class_name)
                .unwrap_or("not observed");
            if low_vector_post_repair.continue_attempted {
                low_vector_post_repair.observe_unsupported_exit(&run_loop_exit);
            }
            unsupported_exit_observed = true;
            blockers.push(format!(
                "firmware run-loop stopped at unsupported exit {index}: reason {reason_name}, exception class {exception_class_name}"
            ));
            exits.push(run_loop_exit);
            break;
        }
    }

    if vcpu_created {
        let status = unsafe { hv_vcpu_destroy(vcpu) };
        vcpu_destroy_status = Some(status);
        vcpu_destroyed = status == HV_SUCCESS;
        if !vcpu_destroyed {
            blockers.push(format!("hv_vcpu_destroy failed: {status:#x}"));
        }
    }

    if guest_ram_memory_mapped {
        let status = unsafe { hv_vm_unmap(WINDOWS_ARM_GUEST_RAM_IPA, guest_ram_bytes_usize) };
        guest_ram_unmap_status = Some(status);
        guest_ram_memory_unmapped = status == HV_SUCCESS;
        if !guest_ram_memory_unmapped {
            blockers.push(format!("hv_vm_unmap guest RAM failed: {status:#x}"));
        }
    }

    if vars_memory_mapped {
        let status = unsafe { hv_vm_unmap(WINDOWS_ARM_UEFI_VARS_IPA, slot_bytes_usize) };
        vars_unmap_status = Some(status);
        vars_memory_unmapped = status == HV_SUCCESS;
        if !vars_memory_unmapped {
            blockers.push(format!("hv_vm_unmap vars pflash failed: {status:#x}"));
        }
    }

    if low_vars_alias_mapped {
        let status = unsafe { hv_vm_unmap(WINDOWS_ARM_UEFI_LOW_VARS_ALIAS_IPA, slot_bytes_usize) };
        low_vars_alias_unmap_status = Some(status);
        if status != HV_SUCCESS {
            blockers.push(format!(
                "hv_vm_unmap low vars pflash alias failed: {status:#x}"
            ));
        }
    }

    if low_firmware_alias_mapped {
        let status = unsafe { hv_vm_unmap(WINDOWS_ARM_UEFI_LOW_CODE_ALIAS_IPA, slot_bytes_usize) };
        low_firmware_alias_unmap_status = Some(status);
        if status != HV_SUCCESS {
            blockers.push(format!(
                "hv_vm_unmap low firmware pflash alias failed: {status:#x}"
            ));
        }
    }

    if firmware_memory_mapped {
        let status = unsafe { hv_vm_unmap(WINDOWS_ARM_UEFI_CODE_IPA, slot_bytes_usize) };
        firmware_unmap_status = Some(status);
        firmware_memory_unmapped = status == HV_SUCCESS;
        if !firmware_memory_unmapped {
            blockers.push(format!("hv_vm_unmap firmware pflash failed: {status:#x}"));
        }
    }

    let vm_destroy_status = if vm_created {
        let status = unsafe { hv_vm_destroy() };
        if status != HV_SUCCESS {
            blockers.push(format!("hv_vm_destroy failed: {status:#x}"));
        }
        Some(status)
    } else {
        None
    };
    let vm_destroyed = vm_destroy_status == Some(HV_SUCCESS);

    if firmware_memory_allocated {
        let status = unsafe { hv_vm_deallocate(firmware_memory, slot_bytes_usize) };
        firmware_deallocate_status = Some(status);
        firmware_memory_deallocated = status == HV_SUCCESS;
        if !firmware_memory_deallocated {
            blockers.push(format!(
                "hv_vm_deallocate firmware pflash failed: {status:#x}"
            ));
        }
    }
    if vars_memory_allocated {
        let status = unsafe { hv_vm_deallocate(vars_memory, slot_bytes_usize) };
        vars_deallocate_status = Some(status);
        vars_memory_deallocated = status == HV_SUCCESS;
        if !vars_memory_deallocated {
            blockers.push(format!("hv_vm_deallocate vars pflash failed: {status:#x}"));
        }
    }
    if guest_ram_memory_allocated {
        let status = unsafe { hv_vm_deallocate(guest_ram_memory, guest_ram_bytes_usize) };
        guest_ram_deallocate_status = Some(status);
        guest_ram_memory_deallocated = status == HV_SUCCESS;
        if !guest_ram_memory_deallocated {
            blockers.push(format!("hv_vm_deallocate guest RAM failed: {status:#x}"));
        }
    }

    WindowsArmUefiFirmwareRunLoopProbe {
        allowed: true,
        attempted: true,
        vm_created,
        firmware_memory_allocated,
        vars_memory_allocated,
        guest_ram_memory_allocated,
        firmware_memory_populated,
        vars_memory_populated,
        firmware_memory_mapped,
        vars_memory_mapped,
        low_firmware_alias_mapped,
        low_vars_alias_mapped,
        guest_ram_memory_mapped,
        platform_dtb_populated,
        diagnostic_vector_seed_requested,
        diagnostic_vector_populated,
        low_vector_diagnostic_page_repair_requested: repair_low_vector_diagnostic_page,
        low_vector_diagnostic_page_repaired,
        low_vector_diagnostic_page_slot_restored,
        low_vector_diagnostic_page_restore_before_eret_requested:
            restore_low_vector_slot_before_eret,
        low_vector_diagnostic_page_restore_before_eret_attempted,
        low_vector_diagnostic_page_entry_ipa,
        low_vector_diagnostic_page_previous_descriptor,
        low_vector_diagnostic_page_descriptor,
        low_vector_diagnostic_page_repeated_fault_observed,
        low_vector_recommended_vector_remap_requested: remap_low_vector_to_recommended_vector,
        low_vector_recommended_vector_remap_attempted,
        low_vector_recommended_vector_remap_succeeded,
        low_vector_recommended_vector_remap_target_physical_address,
        low_vector_recommended_vector_remap_descriptor,
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
        low_vector_diagnostic_page_resume_attempted: low_vector_resume.attempted,
        low_vector_diagnostic_page_resume_armed: low_vector_resume.armed,
        low_vector_diagnostic_page_resume_original_pc: low_vector_resume.original_pc,
        low_vector_diagnostic_page_resume_original_elr_el1: low_vector_resume.original_elr_el1,
        low_vector_diagnostic_page_resume_original_esr_el1: low_vector_resume.original_esr_el1,
        low_vector_diagnostic_page_resume_original_far_el1: low_vector_resume.original_far_el1,
        low_vector_diagnostic_page_resume_original_spsr_el1: low_vector_resume.original_spsr_el1,
        low_vector_diagnostic_page_original_slot_bytes: low_vector_resume.original_slot_bytes,
        low_vector_diagnostic_page_resume_target_instruction_before_eret: low_vector_resume
            .target_instruction_word_before_eret,
        low_vector_diagnostic_page_resume_target_stage1_leaf_descriptor_before_eret:
            low_vector_resume.target_stage1_leaf_descriptor_before_eret,
        low_vector_diagnostic_page_resume_target_stage1_leaf_kind_before_eret: low_vector_resume
            .target_stage1_leaf_kind_before_eret,
        low_vector_diagnostic_page_resume_target_is_installed_diagnostic_hvc_before_eret:
            low_vector_resume.target_is_installed_diagnostic_hvc_before_eret,
        low_vector_diagnostic_page_resume_elr_el1_set_status: low_vector_resume.elr_el1_set_status,
        low_vector_diagnostic_page_resume_spsr_el1_set_status: low_vector_resume
            .spsr_el1_set_status,
        low_vector_diagnostic_page_resume_cpsr_set_status: low_vector_resume.cpsr_set_status,
        low_vector_diagnostic_page_resume_pc_set_status: low_vector_resume.pc_set_status,
        vcpu_created,
        pc_set,
        x0_dtb_ipa_set,
        cpsr_set,
        sp_el1_set,
        diagnostic_vector_vbar_el1_set,
        recommended_vector_base_vbar_requested: try_recommended_vector_base_vbar,
        recommended_vector_base_vbar_attempted,
        recommended_vector_base_vbar_set,
        recommended_vector_base_vbar_diagnostic_vector_populated,
        recommended_vector_base_vbar_resume_requested: continue_after_recommended_vector_base_vbar,
        recommended_vector_base_vbar_resume_attempted,
        recommended_vector_base_vbar_resume_armed,
        interrupt_timer_wiring_requested: wire_interrupt_timer,
        interrupt_timer_initialized,
        run_loop_attempted,
        firmware_progress_observed,
        unsupported_exit_observed,
        watchdog_cancel_fired,
        vcpu_destroyed,
        firmware_memory_unmapped,
        vars_memory_unmapped,
        guest_ram_memory_unmapped,
        firmware_memory_deallocated,
        vars_memory_deallocated,
        guest_ram_memory_deallocated,
        vm_destroyed,
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
        sp_el1_seed_ipa,
        diagnostic_vector_location,
        diagnostic_vector_ipa,
        diagnostic_vector_bytes: WINDOWS_ARM_DIAGNOSTIC_VECTOR_BYTES,
        recommended_vector_base_vbar_source_exit_index,
        recommended_vector_base_vbar_target,
        recommended_vector_base_vbar_target_physical_address,
        recommended_vector_base_vbar_reason,
        recommended_vector_base_vbar_current_el_spx_sync_instruction_word,
        recommended_vector_base_vbar_current_el_spx_sync_instruction_hint,
        recommended_vector_base_vbar_followup_exit_observed,
        recommended_vector_base_vbar_followup_exit_index,
        recommended_vector_base_vbar_followup_exit_reason,
        recommended_vector_base_vbar_followup_exit_diagnosis,
        recommended_vector_base_vbar_followup_pc,
        recommended_vector_base_vbar_followup_vbar_el1,
        recommended_vector_base_vbar_followup_target_still_set,
        recommended_vector_base_vbar_resume_original_pc,
        recommended_vector_base_vbar_resume_original_elr_el1,
        recommended_vector_base_vbar_resume_original_esr_el1,
        recommended_vector_base_vbar_resume_original_far_el1,
        recommended_vector_base_vbar_resume_original_spsr_el1,
        slot_bytes: WINDOWS_ARM_UEFI_SLOT_BYTES,
        guest_ram_bytes,
        platform_dtb_bytes,
        platform_dtb_magic,
        platform_dtb_magic_verified,
        requested_exits: bounded_requested_exits,
        observed_exits: exits.len() as u32,
        watchdog_timeout_ms: bounded_watchdog_timeout_ms,
        vtimer_offset_value: wire_interrupt_timer.then_some(WINDOWS_ARM_VTIMER_OFFSET_VALUE),
        cntv_cval_value: wire_interrupt_timer.then_some(cntv_cval_value),
        cntv_ctl_value: wire_interrupt_timer.then_some(cntv_ctl_value),
        vtimer_exit_count,
        pending_irq_injected_count,
        device_irq_injected_count,
        device_irq_cleared_count,
        handled_mmio_read_count,
        handled_mmio_write_count,
        handled_pl011_mmio_count,
        handled_pl031_mmio_count,
        handled_gicd_mmio_count,
        handled_gicr_mmio_count,
        handled_virtio_installer_iso_mmio_count,
        handled_virtio_target_disk_mmio_count,
        virtio_queue_notify_count,
        virtio_request_completion_count,
        handled_icc_read_count,
        handled_icc_write_count,
        handled_icc_iar1_read_count,
        handled_icc_eoir1_write_count,
        handled_icc_dir_write_count,
        last_icc_iar1_intid,
        last_icc_eoir1_intid,
        last_icc_dir_intid,
        firmware_source_bytes,
        vars_source_bytes,
        installer_iso_path,
        writable_target_disk_path,
        block_devices,
        firmware_map_flags: "read|exec",
        vars_map_flags: "read|write",
        low_firmware_alias_map_flags: "read|exec",
        low_vars_alias_map_flags: "read|write",
        guest_ram_map_flags: "read|write|exec",
        low_pflash_alias_requested: map_low_pflash_alias,
        vm_create_status: Some(vm_create_status),
        firmware_allocate_status,
        vars_allocate_status,
        guest_ram_allocate_status,
        firmware_map_status,
        vars_map_status,
        low_firmware_alias_map_status,
        low_vars_alias_map_status,
        guest_ram_map_status,
        vcpu_create_status,
        pc_set_status,
        x0_dtb_ipa_set_status,
        cpsr_set_status,
        sp_el1_set_status,
        diagnostic_vector_vbar_el1_set_status,
        recommended_vector_base_vbar_set_status,
        recommended_vector_base_vbar_resume_vbar_el1_set_status,
        recommended_vector_base_vbar_resume_elr_el1_set_status,
        recommended_vector_base_vbar_resume_spsr_el1_set_status,
        recommended_vector_base_vbar_resume_pc_set_status,
        vtimer_offset_set_status,
        cntv_cval_set_status,
        cntv_ctl_set_status,
        vtimer_initial_unmask_status,
        last_pending_irq_set_status,
        last_device_irq_set_status,
        last_device_irq_clear_status,
        last_vtimer_unmask_status,
        final_pc_status,
        final_pc,
        vcpu_destroy_status,
        firmware_unmap_status,
        vars_unmap_status,
        low_firmware_alias_unmap_status,
        low_vars_alias_unmap_status,
        guest_ram_unmap_status,
        firmware_deallocate_status,
        vars_deallocate_status,
        guest_ram_deallocate_status,
        vm_destroy_status,
        exits,
        blockers,
    }
}

pub(crate) struct FirmwareRunLoopProbeResultInput<'a> {
    pub(crate) allowed: bool,
    pub(crate) attempted: bool,
    pub(crate) host: HvfHostCapabilities,
    pub(crate) pflash_map_verified: bool,
    pub(crate) guest_ram_bytes: u64,
    pub(crate) requested_exits: u32,
    pub(crate) watchdog_timeout_ms: u64,
    pub(crate) options: &'a WindowsArmUefiFirmwareRunLoopExecutionOptions,
    pub(crate) firmware_source_bytes: Option<u64>,
    pub(crate) vars_source_bytes: Option<u64>,
    pub(crate) blockers: Vec<String>,
}

pub(crate) fn firmware_run_loop_probe_result(
    input: FirmwareRunLoopProbeResultInput<'_>,
) -> WindowsArmUefiFirmwareRunLoopProbe {
    let FirmwareRunLoopProbeResultInput {
        allowed,
        attempted,
        host,
        pflash_map_verified,
        guest_ram_bytes,
        requested_exits,
        watchdog_timeout_ms,
        options,
        firmware_source_bytes,
        vars_source_bytes,
        blockers,
    } = input;
    let map_low_pflash_alias = options.map_low_pflash_alias;
    let seed_diagnostic_vector = options.seed_diagnostic_vector;
    let seed_guest_ram_diagnostic_vector = options.seed_guest_ram_diagnostic_vector;
    let seed_executable_diagnostic_vector = options.seed_executable_diagnostic_vector;
    let try_recommended_vector_base_vbar = options.try_recommended_vector_base_vbar;
    let continue_after_recommended_vector_base_vbar =
        options.continue_after_recommended_vector_base_vbar;
    let repair_low_vector_diagnostic_page = options.repair_low_vector_diagnostic_page;
    let continue_after_low_vector_repair = options.continue_after_low_vector_repair;
    let wire_interrupt_timer = options.wire_interrupt_timer;
    let installer_iso_path = options.installer_iso_path.clone();
    let writable_target_disk_path = options.writable_target_disk_path.clone();
    let diagnostic_vector = windows_arm_diagnostic_vector_selection(
        seed_diagnostic_vector,
        seed_guest_ram_diagnostic_vector,
        seed_executable_diagnostic_vector,
    );
    let diagnostic_vector_seed_requested = diagnostic_vector.requested;
    let diagnostic_vector_location = diagnostic_vector.location;
    let diagnostic_vector_ipa = diagnostic_vector.ipa;
    let block_devices = windows_arm_firmware_block_devices(
        installer_iso_path.clone(),
        writable_target_disk_path.clone(),
    );
    let (platform_dtb_bytes, platform_dtb_magic, platform_dtb_magic_verified) =
        windows_arm_firmware_run_loop_dtb_metadata(guest_ram_bytes);
    let recommended_vector_base_vbar_reason = recommended_vector_base_vbar_initial_reason(
        try_recommended_vector_base_vbar,
        diagnostic_vector_seed_requested,
        repair_low_vector_diagnostic_page,
    );
    let low_vector_post_repair = LowVectorPostRepairTelemetry::default();
    WindowsArmUefiFirmwareRunLoopProbe {
        allowed,
        attempted,
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
        stop_at_first_post_repair_device_boundary_requested: options
            .stop_at_first_post_repair_device_boundary,
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
        pflash_map_verified,
        reset_vector_ipa: WINDOWS_ARM_UEFI_CODE_IPA,
        firmware_slot_ipa: WINDOWS_ARM_UEFI_CODE_IPA,
        vars_slot_ipa: WINDOWS_ARM_UEFI_VARS_IPA,
        low_firmware_alias_ipa: WINDOWS_ARM_UEFI_LOW_CODE_ALIAS_IPA,
        low_vars_alias_ipa: WINDOWS_ARM_UEFI_LOW_VARS_ALIAS_IPA,
        guest_ram_ipa: WINDOWS_ARM_GUEST_RAM_IPA,
        platform_dtb_ipa: WINDOWS_ARM_PLATFORM_DTB_IPA,
        platform_dtb_guest_ram_offset: WINDOWS_ARM_PLATFORM_DTB_GUEST_RAM_OFFSET,
        sp_el1_seed_ipa: windows_arm_initial_sp_el1_ipa(guest_ram_bytes),
        diagnostic_vector_location,
        diagnostic_vector_ipa,
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
        requested_exits,
        observed_exits: 0,
        watchdog_timeout_ms,
        vtimer_offset_value: wire_interrupt_timer.then_some(WINDOWS_ARM_VTIMER_OFFSET_VALUE),
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
        firmware_source_bytes,
        vars_source_bytes,
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
