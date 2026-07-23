//! UEFI reset-vector first-entry probe.
//!
//! Split out of the single 12,111-line apple.rs backend.

use super::*;
use crate::*;

pub fn probe_windows_11_arm_uefi_reset_vector_entry(
    allow_entry: bool,
    pflash_map: WindowsArmUefiPflashMapProbe,
    host: HvfHostCapabilities,
) -> WindowsArmUefiResetVectorEntryProbe {
    let mut blockers = pflash_map.blockers.clone();
    let firmware_source_bytes = pflash_map
        .firmware_slot
        .as_ref()
        .map(|slot| slot.source_bytes);
    let vars_source_bytes = pflash_map.vars_slot.as_ref().map(|slot| slot.source_bytes);

    if !allow_entry {
        blockers.push(
            "set BRIDGEVM_HVF_ALLOW_UEFI_RESET_VECTOR_ENTRY=1 or pass --allow-entry to map Windows UEFI pflash slots, create one vCPU, set PC to the reset vector, and run once under a watchdog".to_string(),
        );
        return reset_vector_entry_probe_result(
            false,
            false,
            host,
            pflash_map.pflash_map_verified,
            firmware_source_bytes,
            vars_source_bytes,
            blockers,
        );
    }

    if !pflash_map.pflash_map_verified {
        blockers.push(
            "pflash memory-image mapper did not verify code/vars slots; refusing reset-vector entry"
                .to_string(),
        );
        return reset_vector_entry_probe_result(
            true,
            false,
            host,
            false,
            firmware_source_bytes,
            vars_source_bytes,
            blockers,
        );
    }

    if !host.available {
        blockers.push("Hypervisor.framework host capabilities are not available".to_string());
        return reset_vector_entry_probe_result(
            true,
            false,
            host,
            true,
            firmware_source_bytes,
            vars_source_bytes,
            blockers,
        );
    }

    let slot_bytes_usize: usize = WINDOWS_ARM_UEFI_SLOT_BYTES
        .try_into()
        .expect("Windows UEFI pflash slot fits in usize");
    let mut firmware_memory = ptr::null_mut();
    let mut vars_memory = ptr::null_mut();
    let mut firmware_memory_populated = false;
    let mut vars_memory_populated = false;
    let mut firmware_memory_mapped = false;
    let mut vars_memory_mapped = false;
    let mut vcpu_created = false;
    let mut pc_set = false;
    let mut cpsr_set = false;
    let mut run_attempted = false;
    let mut reset_vector_entry_observed = false;
    let mut firmware_progress_observed = false;
    let mut watchdog_cancel_fired = false;
    let mut vcpu_destroyed = false;
    let mut firmware_memory_unmapped = false;
    let mut vars_memory_unmapped = false;
    let mut firmware_memory_deallocated = false;
    let mut vars_memory_deallocated = false;

    let mut firmware_map_status = None;
    let mut vars_map_status = None;
    let mut vcpu_create_status = None;
    let mut pc_set_status = None;
    let mut cpsr_set_status = None;
    let mut run_status = None;
    let mut exit_reason = None;
    let mut exit_syndrome = None;
    let mut exit_exception_class = None;
    let mut exit_virtual_address = None;
    let mut exit_physical_address = None;
    let mut pc_after_run_status = None;
    let mut pc_after_run = None;
    let mut watchdog_cancel_status = None;
    let mut vcpu_destroy_status = None;
    let mut firmware_unmap_status = None;
    let mut vars_unmap_status = None;
    let mut firmware_deallocate_status = None;
    let mut vars_deallocate_status = None;

    let mut vcpu = 0;
    let mut exit = ptr::null_mut();

    let vm_create_status = unsafe { hv_vm_create(ptr::null_mut()) };
    let vm_created = vm_create_status == HV_SUCCESS;
    if !vm_created {
        blockers.push(format!("hv_vm_create failed: {vm_create_status:#x}"));
        return WindowsArmUefiResetVectorEntryProbe {
            allowed: true,
            attempted: true,
            vm_created,
            firmware_memory_allocated: false,
            vars_memory_allocated: false,
            firmware_memory_populated: false,
            vars_memory_populated: false,
            firmware_memory_mapped: false,
            vars_memory_mapped: false,
            vcpu_created: false,
            pc_set: false,
            cpsr_set: false,
            run_attempted: false,
            reset_vector_entry_observed: false,
            firmware_progress_observed: false,
            watchdog_cancel_fired: false,
            vcpu_destroyed: false,
            firmware_memory_unmapped: false,
            vars_memory_unmapped: false,
            firmware_memory_deallocated: false,
            vars_memory_deallocated: false,
            vm_destroyed: false,
            host,
            pflash_map_verified: true,
            reset_vector_ipa: WINDOWS_ARM_UEFI_CODE_IPA,
            firmware_slot_ipa: WINDOWS_ARM_UEFI_CODE_IPA,
            vars_slot_ipa: WINDOWS_ARM_UEFI_VARS_IPA,
            slot_bytes: WINDOWS_ARM_UEFI_SLOT_BYTES,
            firmware_source_bytes,
            vars_source_bytes,
            firmware_map_flags: "read|exec",
            vars_map_flags: "read|write",
            vm_create_status: Some(vm_create_status),
            firmware_allocate_status: None,
            vars_allocate_status: None,
            firmware_map_status: None,
            vars_map_status: None,
            vcpu_create_status: None,
            pc_set_status: None,
            cpsr_set_status: None,
            run_status: None,
            exit_reason: None,
            exit_syndrome: None,
            exit_exception_class: None,
            exit_virtual_address: None,
            exit_physical_address: None,
            pc_after_run_status: None,
            pc_after_run: None,
            watchdog_cancel_status: None,
            vcpu_destroy_status: None,
            firmware_unmap_status: None,
            vars_unmap_status: None,
            firmware_deallocate_status: None,
            vars_deallocate_status: None,
            vm_destroy_status: None,
            blockers,
        };
    }

    let firmware_status =
        unsafe { hv_vm_allocate(&mut firmware_memory, slot_bytes_usize, HV_ALLOCATE_DEFAULT) };
    let firmware_allocate_status = Some(firmware_status);
    let firmware_memory_allocated = firmware_status == HV_SUCCESS && !firmware_memory.is_null();
    if !firmware_memory_allocated {
        blockers.push(format!(
            "hv_vm_allocate firmware pflash failed: {firmware_status:#x}"
        ));
    }

    let vars_status =
        unsafe { hv_vm_allocate(&mut vars_memory, slot_bytes_usize, HV_ALLOCATE_DEFAULT) };
    let vars_allocate_status = Some(vars_status);
    let vars_memory_allocated = vars_status == HV_SUCCESS && !vars_memory.is_null();
    if !vars_memory_allocated {
        blockers.push(format!(
            "hv_vm_allocate vars pflash failed: {vars_status:#x}"
        ));
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

    if firmware_memory_mapped && vars_memory_mapped {
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

    if vcpu_created {
        let status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_CPSR, AARCH64_PSTATE_EL1H_DAIF_MASKED) };
        cpsr_set_status = Some(status);
        cpsr_set = status == HV_SUCCESS;
        if !cpsr_set {
            blockers.push(format!("hv_vcpu_set_reg(CPSR) failed: {status:#x}"));
        }
    }

    if vcpu_created && pc_set && cpsr_set {
        run_attempted = true;
        let observation = run_vcpu_once_with_watchdog(vcpu, exit);
        run_status = Some(observation.run_status);
        exit_reason = observation.exit_reason;
        exit_syndrome = observation.exit_syndrome;
        exit_exception_class = exit_syndrome.map(arm_exception_class);
        exit_virtual_address = observation.exit_virtual_address;
        exit_physical_address = observation.exit_physical_address;
        watchdog_cancel_status = observation.watchdog_cancel_status;
        watchdog_cancel_fired = watchdog_cancel_status.is_some();

        if observation.run_status == HV_SUCCESS {
            reset_vector_entry_observed = exit_reason.is_some();
            if !reset_vector_entry_observed {
                blockers
                    .push("hv_vcpu_run returned success without an exit info pointer".to_string());
            }
        } else {
            blockers.push(format!(
                "reset-vector hv_vcpu_run failed: {:#x}",
                observation.run_status
            ));
        }

        let mut pc = 0;
        let status = unsafe { hv_vcpu_get_reg(vcpu, HV_REG_PC, &mut pc) };
        pc_after_run_status = Some(status);
        if status == HV_SUCCESS {
            pc_after_run = Some(pc);
            firmware_progress_observed = pc != WINDOWS_ARM_UEFI_CODE_IPA;
        } else {
            blockers.push(format!("hv_vcpu_get_reg(PC) after run failed: {status:#x}"));
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

    if vars_memory_mapped {
        let status = unsafe { hv_vm_unmap(WINDOWS_ARM_UEFI_VARS_IPA, slot_bytes_usize) };
        vars_unmap_status = Some(status);
        vars_memory_unmapped = status == HV_SUCCESS;
        if !vars_memory_unmapped {
            blockers.push(format!("hv_vm_unmap vars pflash failed: {status:#x}"));
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

    let vm_destroy_status = unsafe { hv_vm_destroy() };
    let vm_destroyed = vm_destroy_status == HV_SUCCESS;
    if !vm_destroyed {
        blockers.push(format!("hv_vm_destroy failed: {vm_destroy_status:#x}"));
    }

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

    WindowsArmUefiResetVectorEntryProbe {
        allowed: true,
        attempted: true,
        vm_created,
        firmware_memory_allocated,
        vars_memory_allocated,
        firmware_memory_populated,
        vars_memory_populated,
        firmware_memory_mapped,
        vars_memory_mapped,
        vcpu_created,
        pc_set,
        cpsr_set,
        run_attempted,
        reset_vector_entry_observed,
        firmware_progress_observed,
        watchdog_cancel_fired,
        vcpu_destroyed,
        firmware_memory_unmapped,
        vars_memory_unmapped,
        firmware_memory_deallocated,
        vars_memory_deallocated,
        vm_destroyed,
        host,
        pflash_map_verified: pflash_map.pflash_map_verified,
        reset_vector_ipa: WINDOWS_ARM_UEFI_CODE_IPA,
        firmware_slot_ipa: WINDOWS_ARM_UEFI_CODE_IPA,
        vars_slot_ipa: WINDOWS_ARM_UEFI_VARS_IPA,
        slot_bytes: WINDOWS_ARM_UEFI_SLOT_BYTES,
        firmware_source_bytes,
        vars_source_bytes,
        firmware_map_flags: "read|exec",
        vars_map_flags: "read|write",
        vm_create_status: Some(vm_create_status),
        firmware_allocate_status,
        vars_allocate_status,
        firmware_map_status,
        vars_map_status,
        vcpu_create_status,
        pc_set_status,
        cpsr_set_status,
        run_status,
        exit_reason,
        exit_syndrome,
        exit_exception_class,
        exit_virtual_address,
        exit_physical_address,
        pc_after_run_status,
        pc_after_run,
        watchdog_cancel_status,
        vcpu_destroy_status,
        firmware_unmap_status,
        vars_unmap_status,
        firmware_deallocate_status,
        vars_deallocate_status,
        vm_destroy_status: Some(vm_destroy_status),
        blockers,
    }
}

pub(crate) fn reset_vector_entry_probe_result(
    allowed: bool,
    attempted: bool,
    host: HvfHostCapabilities,
    pflash_map_verified: bool,
    firmware_source_bytes: Option<u64>,
    vars_source_bytes: Option<u64>,
    blockers: Vec<String>,
) -> WindowsArmUefiResetVectorEntryProbe {
    WindowsArmUefiResetVectorEntryProbe {
        allowed,
        attempted,
        vm_created: false,
        firmware_memory_allocated: false,
        vars_memory_allocated: false,
        firmware_memory_populated: false,
        vars_memory_populated: false,
        firmware_memory_mapped: false,
        vars_memory_mapped: false,
        vcpu_created: false,
        pc_set: false,
        cpsr_set: false,
        run_attempted: false,
        reset_vector_entry_observed: false,
        firmware_progress_observed: false,
        watchdog_cancel_fired: false,
        vcpu_destroyed: false,
        firmware_memory_unmapped: false,
        vars_memory_unmapped: false,
        firmware_memory_deallocated: false,
        vars_memory_deallocated: false,
        vm_destroyed: false,
        host,
        pflash_map_verified,
        reset_vector_ipa: WINDOWS_ARM_UEFI_CODE_IPA,
        firmware_slot_ipa: WINDOWS_ARM_UEFI_CODE_IPA,
        vars_slot_ipa: WINDOWS_ARM_UEFI_VARS_IPA,
        slot_bytes: WINDOWS_ARM_UEFI_SLOT_BYTES,
        firmware_source_bytes,
        vars_source_bytes,
        firmware_map_flags: "read|exec",
        vars_map_flags: "read|write",
        vm_create_status: None,
        firmware_allocate_status: None,
        vars_allocate_status: None,
        firmware_map_status: None,
        vars_map_status: None,
        vcpu_create_status: None,
        pc_set_status: None,
        cpsr_set_status: None,
        run_status: None,
        exit_reason: None,
        exit_syndrome: None,
        exit_exception_class: None,
        exit_virtual_address: None,
        exit_physical_address: None,
        pc_after_run_status: None,
        pc_after_run: None,
        watchdog_cancel_status: None,
        vcpu_destroy_status: None,
        firmware_unmap_status: None,
        vars_unmap_status: None,
        firmware_deallocate_status: None,
        vars_deallocate_status: None,
        vm_destroy_status: None,
        blockers,
    }
}
