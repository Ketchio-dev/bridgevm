//! MMIO read-exit and read/write emulation probes.
//!
//! Split out of the single 12,111-line apple.rs backend.

use super::*;
use crate::*;

pub fn probe_hvf_mmio_read_exit(
    allow_mmio: bool,
    host: HvfHostCapabilities,
) -> HvfMmioReadExitProbe {
    let mut blockers = Vec::new();

    if !allow_mmio {
        blockers.push(
            "set BRIDGEVM_HVF_ALLOW_MMIO_READ=1 or pass --allow-mmio to run one unmapped LDR read and observe the MMIO/data-abort exit".to_string(),
        );
        return mmio_read_exit_probe_result(false, false, host, blockers);
    }

    if !host.available {
        blockers.push("Hypervisor.framework host capabilities are not available".to_string());
        return mmio_read_exit_probe_result(true, false, host, blockers);
    }

    let mut memory_allocated = false;
    let mut memory_mapped = false;
    let mut vcpu_created = false;
    let mut pc_set = false;
    let mut cpsr_set = false;
    let mut address_register_set = false;
    let mut run_attempted = false;
    let mut mmio_exit_observed = false;
    let mut vcpu_destroyed = false;
    let mut memory_unmapped = false;
    let mut vm_destroyed = false;
    let mut memory_deallocated = false;

    let mut allocate_status = None;
    let mut map_status = None;
    let mut vcpu_create_status = None;
    let mut pc_set_status = None;
    let mut cpsr_set_status = None;
    let mut address_register_set_status = None;
    let mut run_status = None;
    let mut exit_reason = None;
    let mut exit_syndrome = None;
    let mut exit_virtual_address = None;
    let mut exit_physical_address = None;
    let mut watchdog_cancel_status = None;
    let mut vcpu_destroy_status = None;
    let mut unmap_status = None;
    let mut vm_destroy_status = None;
    let mut deallocate_status = None;

    let mut memory = ptr::null_mut();
    let mut vcpu = 0;
    let mut exit = ptr::null_mut();

    let status = unsafe { hv_vm_create(ptr::null_mut()) };
    let vm_create_status = Some(status);
    let vm_created = status == HV_SUCCESS;
    if !vm_created {
        blockers.push(format!("hv_vm_create failed: {status:#x}"));
    }

    if vm_created {
        let status = unsafe { hv_vm_allocate(&mut memory, PROBE_BYTES, HV_ALLOCATE_DEFAULT) };
        allocate_status = Some(status);
        memory_allocated = status == HV_SUCCESS && !memory.is_null();
        if memory_allocated {
            let instruction = AARCH64_LDR_X0_FROM_X1.to_le_bytes();
            unsafe {
                ptr::copy_nonoverlapping(
                    instruction.as_ptr(),
                    memory.cast::<u8>(),
                    instruction.len(),
                );
            }
        } else {
            blockers.push(format!("hv_vm_allocate failed: {status:#x}"));
        }
    }

    if vm_created && memory_allocated {
        let status = unsafe {
            hv_vm_map(
                memory,
                PROBE_IPA_START,
                PROBE_BYTES,
                HV_MEMORY_READ | HV_MEMORY_WRITE | HV_MEMORY_EXEC,
            )
        };
        map_status = Some(status);
        memory_mapped = status == HV_SUCCESS;
        if !memory_mapped {
            blockers.push(format!("hv_vm_map failed: {status:#x}"));
        }
    }

    if vm_created && memory_mapped {
        let status = unsafe { hv_vcpu_create(&mut vcpu, &mut exit, ptr::null_mut()) };
        vcpu_create_status = Some(status);
        vcpu_created = status == HV_SUCCESS;
        if !vcpu_created {
            blockers.push(format!("hv_vcpu_create failed: {status:#x}"));
        }
    }

    if vcpu_created {
        let status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_PC, PROBE_IPA_START) };
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

    if vcpu_created {
        let status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_X1, PROBE_MMIO_IPA) };
        address_register_set_status = Some(status);
        address_register_set = status == HV_SUCCESS;
        if !address_register_set {
            blockers.push(format!("hv_vcpu_set_reg(X1) failed: {status:#x}"));
        }
    }

    if vcpu_created && pc_set && cpsr_set && address_register_set {
        run_attempted = true;
        let observation = run_vcpu_once_with_watchdog(vcpu, exit);
        run_status = Some(observation.run_status);
        exit_reason = observation.exit_reason;
        exit_syndrome = observation.exit_syndrome;
        exit_virtual_address = observation.exit_virtual_address;
        exit_physical_address = observation.exit_physical_address;
        watchdog_cancel_status = observation.watchdog_cancel_status;
        if watchdog_cancel_status.is_some() {
            blockers.push("MMIO read watchdog fired before exception exit".to_string());
        }

        if observation.run_status == HV_SUCCESS {
            if exit_reason.is_none() {
                blockers
                    .push("hv_vcpu_run returned success without an exit info pointer".to_string());
            } else {
                mmio_exit_observed = exit_reason == Some(HV_EXIT_REASON_EXCEPTION)
                    && (exit_virtual_address == Some(PROBE_MMIO_IPA)
                        || exit_physical_address == Some(PROBE_MMIO_IPA)
                        || exit_syndrome.is_some_and(is_data_abort_syndrome));
                if exit_reason != Some(HV_EXIT_REASON_EXCEPTION) {
                    blockers.push(format!(
                        "hv_vcpu_run returned non-exception exit reason: {}",
                        exit_reason.unwrap_or_default()
                    ));
                }
                if !mmio_exit_observed {
                    blockers.push(format!(
                        "hv_vcpu_run did not report an MMIO/data-abort style exit for {PROBE_MMIO_IPA:#x}"
                    ));
                }
            }
        } else {
            blockers.push(format!("hv_vcpu_run failed: {:#x}", observation.run_status));
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

    if memory_mapped {
        let status = unsafe { hv_vm_unmap(PROBE_IPA_START, PROBE_BYTES) };
        unmap_status = Some(status);
        memory_unmapped = status == HV_SUCCESS;
        if !memory_unmapped {
            blockers.push(format!("hv_vm_unmap failed: {status:#x}"));
        }
    }

    if vm_created {
        let status = unsafe { hv_vm_destroy() };
        vm_destroy_status = Some(status);
        vm_destroyed = status == HV_SUCCESS;
        if !vm_destroyed {
            blockers.push(format!("hv_vm_destroy failed: {status:#x}"));
        }
    }

    if memory_allocated {
        let status = unsafe { hv_vm_deallocate(memory, PROBE_BYTES) };
        deallocate_status = Some(status);
        memory_deallocated = status == HV_SUCCESS;
        if !memory_deallocated {
            blockers.push(format!("hv_vm_deallocate failed: {status:#x}"));
        }
    }

    HvfMmioReadExitProbe {
        allowed: true,
        attempted: true,
        vm_created,
        memory_allocated,
        memory_mapped,
        vcpu_created,
        pc_set,
        cpsr_set,
        address_register_set,
        run_attempted,
        mmio_exit_observed,
        watchdog_cancel_fired: watchdog_cancel_status.is_some(),
        vcpu_destroyed,
        memory_unmapped,
        vm_destroyed,
        memory_deallocated,
        host,
        code_ipa_start: PROBE_IPA_START,
        mmio_ipa: PROBE_MMIO_IPA,
        bytes: PROBE_BYTES,
        instruction: "LDR X0, [X1]",
        vm_create_status,
        allocate_status,
        map_status,
        vcpu_create_status,
        pc_set_status,
        cpsr_set_status,
        address_register_set_status,
        run_status,
        exit_reason,
        exit_syndrome,
        exit_virtual_address,
        exit_physical_address,
        watchdog_cancel_status,
        vcpu_destroy_status,
        unmap_status,
        vm_destroy_status,
        deallocate_status,
        blockers,
    }
}

pub(crate) fn mmio_read_exit_probe_result(
    allowed: bool,
    attempted: bool,
    host: HvfHostCapabilities,
    blockers: Vec<String>,
) -> HvfMmioReadExitProbe {
    HvfMmioReadExitProbe {
        allowed,
        attempted,
        vm_created: false,
        memory_allocated: false,
        memory_mapped: false,
        vcpu_created: false,
        pc_set: false,
        cpsr_set: false,
        address_register_set: false,
        run_attempted: false,
        mmio_exit_observed: false,
        watchdog_cancel_fired: false,
        vcpu_destroyed: false,
        memory_unmapped: false,
        vm_destroyed: false,
        memory_deallocated: false,
        host,
        code_ipa_start: PROBE_IPA_START,
        mmio_ipa: PROBE_MMIO_IPA,
        bytes: PROBE_BYTES,
        instruction: "LDR X0, [X1]",
        vm_create_status: None,
        allocate_status: None,
        map_status: None,
        vcpu_create_status: None,
        pc_set_status: None,
        cpsr_set_status: None,
        address_register_set_status: None,
        run_status: None,
        exit_reason: None,
        exit_syndrome: None,
        exit_virtual_address: None,
        exit_physical_address: None,
        watchdog_cancel_status: None,
        vcpu_destroy_status: None,
        unmap_status: None,
        vm_destroy_status: None,
        deallocate_status: None,
        blockers,
    }
}

pub fn probe_hvf_mmio_read_emulation(
    allow_emulate: bool,
    host: HvfHostCapabilities,
) -> HvfMmioReadEmulationProbe {
    let mut blockers = Vec::new();

    if !allow_emulate {
        blockers.push(
            "set BRIDGEVM_HVF_ALLOW_MMIO_EMULATION=1 or pass --allow-emulate to handle one unmapped LDR read, inject X0, advance PC, and continue to HVC".to_string(),
        );
        return mmio_read_emulation_probe_result(false, false, host, blockers);
    }

    if !host.available {
        blockers.push("Hypervisor.framework host capabilities are not available".to_string());
        return mmio_read_emulation_probe_result(true, false, host, blockers);
    }

    let mut memory_allocated = false;
    let mut memory_mapped = false;
    let mut vcpu_created = false;
    let mut pc_set = false;
    let mut cpsr_set = false;
    let mut address_register_set = false;
    let mut first_run_attempted = false;
    let mut mmio_exit_observed = false;
    let mut pc_read_after_mmio_exit = false;
    let mut emulated_value_injected = false;
    let mut pc_advanced = false;
    let mut second_run_attempted = false;
    let mut continuation_exit_observed = false;
    let mut emulated_value_preserved = false;
    let mut vcpu_destroyed = false;
    let mut memory_unmapped = false;
    let mut vm_destroyed = false;
    let mut memory_deallocated = false;

    let mut allocate_status = None;
    let mut map_status = None;
    let mut vcpu_create_status = None;
    let mut pc_set_status = None;
    let mut cpsr_set_status = None;
    let mut address_register_set_status = None;
    let mut first_run_status = None;
    let mut mmio_exit_reason = None;
    let mut mmio_exit_syndrome = None;
    let mut mmio_exit_virtual_address = None;
    let mut mmio_exit_physical_address = None;
    let mut first_watchdog_cancel_status = None;
    let mut pc_read_status = None;
    let mut pc_after_mmio_exit = None;
    let mut emulated_value_set_status = None;
    let mut pc_advance_status = None;
    let mut second_run_status = None;
    let mut continuation_exit_reason = None;
    let mut continuation_exit_syndrome = None;
    let mut continuation_exit_virtual_address = None;
    let mut continuation_exit_physical_address = None;
    let mut second_watchdog_cancel_status = None;
    let mut emulated_value_read_status = None;
    let mut emulated_value_after_continue = None;
    let mut vcpu_destroy_status = None;
    let mut unmap_status = None;
    let mut vm_destroy_status = None;
    let mut deallocate_status = None;

    let mut memory = ptr::null_mut();
    let mut vcpu = 0;
    let mut exit = ptr::null_mut();

    let status = unsafe { hv_vm_create(ptr::null_mut()) };
    let vm_create_status = Some(status);
    let vm_created = status == HV_SUCCESS;
    if !vm_created {
        blockers.push(format!("hv_vm_create failed: {status:#x}"));
    }

    if vm_created {
        let status = unsafe { hv_vm_allocate(&mut memory, PROBE_BYTES, HV_ALLOCATE_DEFAULT) };
        allocate_status = Some(status);
        memory_allocated = status == HV_SUCCESS && !memory.is_null();
        if memory_allocated {
            let load = AARCH64_LDR_X0_FROM_X1.to_le_bytes();
            let hvc = AARCH64_HVC_0.to_le_bytes();
            unsafe {
                ptr::copy_nonoverlapping(load.as_ptr(), memory.cast::<u8>(), load.len());
                ptr::copy_nonoverlapping(
                    hvc.as_ptr(),
                    memory.cast::<u8>().add(load.len()),
                    hvc.len(),
                );
            }
        } else {
            blockers.push(format!("hv_vm_allocate failed: {status:#x}"));
        }
    }

    if vm_created && memory_allocated {
        let status = unsafe {
            hv_vm_map(
                memory,
                PROBE_IPA_START,
                PROBE_BYTES,
                HV_MEMORY_READ | HV_MEMORY_WRITE | HV_MEMORY_EXEC,
            )
        };
        map_status = Some(status);
        memory_mapped = status == HV_SUCCESS;
        if !memory_mapped {
            blockers.push(format!("hv_vm_map failed: {status:#x}"));
        }
    }

    if vm_created && memory_mapped {
        let status = unsafe { hv_vcpu_create(&mut vcpu, &mut exit, ptr::null_mut()) };
        vcpu_create_status = Some(status);
        vcpu_created = status == HV_SUCCESS;
        if !vcpu_created {
            blockers.push(format!("hv_vcpu_create failed: {status:#x}"));
        }
    }

    if vcpu_created {
        let status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_PC, PROBE_IPA_START) };
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

    if vcpu_created {
        let status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_X1, PROBE_MMIO_IPA) };
        address_register_set_status = Some(status);
        address_register_set = status == HV_SUCCESS;
        if !address_register_set {
            blockers.push(format!("hv_vcpu_set_reg(X1) failed: {status:#x}"));
        }
    }

    if vcpu_created && pc_set && cpsr_set && address_register_set {
        first_run_attempted = true;
        let observation = run_vcpu_once_with_watchdog(vcpu, exit);
        first_run_status = Some(observation.run_status);
        mmio_exit_reason = observation.exit_reason;
        mmio_exit_syndrome = observation.exit_syndrome;
        mmio_exit_virtual_address = observation.exit_virtual_address;
        mmio_exit_physical_address = observation.exit_physical_address;
        first_watchdog_cancel_status = observation.watchdog_cancel_status;
        if first_watchdog_cancel_status.is_some() {
            blockers
                .push("MMIO emulation first run watchdog fired before exception exit".to_string());
        }

        if observation.run_status == HV_SUCCESS {
            if mmio_exit_reason.is_none() {
                blockers.push(
                    "first hv_vcpu_run returned success without an exit info pointer".to_string(),
                );
            } else {
                mmio_exit_observed = mmio_exit_reason == Some(HV_EXIT_REASON_EXCEPTION)
                    && (mmio_exit_virtual_address == Some(PROBE_MMIO_IPA)
                        || mmio_exit_physical_address == Some(PROBE_MMIO_IPA)
                        || mmio_exit_syndrome.is_some_and(is_data_abort_syndrome));
                if !mmio_exit_observed {
                    blockers.push(format!(
                        "first hv_vcpu_run did not report an MMIO/data-abort style exit for {PROBE_MMIO_IPA:#x}"
                    ));
                }
            }
        } else {
            blockers.push(format!(
                "first hv_vcpu_run failed: {:#x}",
                observation.run_status
            ));
        }
    }

    if mmio_exit_observed {
        let mut pc = 0;
        let status = unsafe { hv_vcpu_get_reg(vcpu, HV_REG_PC, &mut pc) };
        pc_read_status = Some(status);
        pc_read_after_mmio_exit = status == HV_SUCCESS;
        if pc_read_after_mmio_exit {
            pc_after_mmio_exit = Some(pc);
        } else {
            blockers.push(format!("hv_vcpu_get_reg(PC) failed: {status:#x}"));
        }
    }

    if mmio_exit_observed {
        let status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_X0, EMULATED_MMIO_READ_VALUE) };
        emulated_value_set_status = Some(status);
        emulated_value_injected = status == HV_SUCCESS;
        if !emulated_value_injected {
            blockers.push(format!("hv_vcpu_set_reg(X0) failed: {status:#x}"));
        }
    }

    if pc_read_after_mmio_exit && emulated_value_injected {
        let status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_PC, PROBE_IPA_START + 4) };
        pc_advance_status = Some(status);
        pc_advanced = status == HV_SUCCESS;
        if !pc_advanced {
            blockers.push(format!("hv_vcpu_set_reg(PC + 4) failed: {status:#x}"));
        }
    }

    if pc_advanced {
        second_run_attempted = true;
        let observation = run_vcpu_once_with_watchdog(vcpu, exit);
        second_run_status = Some(observation.run_status);
        continuation_exit_reason = observation.exit_reason;
        continuation_exit_syndrome = observation.exit_syndrome;
        continuation_exit_virtual_address = observation.exit_virtual_address;
        continuation_exit_physical_address = observation.exit_physical_address;
        second_watchdog_cancel_status = observation.watchdog_cancel_status;
        if second_watchdog_cancel_status.is_some() {
            blockers.push("MMIO emulation second run watchdog fired before HVC exit".to_string());
        }

        if observation.run_status == HV_SUCCESS {
            if continuation_exit_reason.is_none() {
                blockers.push(
                    "second hv_vcpu_run returned success without an exit info pointer".to_string(),
                );
            } else {
                continuation_exit_observed = continuation_exit_reason
                    == Some(HV_EXIT_REASON_EXCEPTION)
                    && continuation_exit_syndrome == Some(AARCH64_HVC_0_SYNDROME);
                if !continuation_exit_observed {
                    blockers.push(format!(
                        "second hv_vcpu_run did not reach HVC continuation exit; syndrome: {}",
                        continuation_exit_syndrome.map_or_else(
                            || "not observed".to_string(),
                            |value| { format!("{value:#x}") }
                        )
                    ));
                }
            }
        } else {
            blockers.push(format!(
                "second hv_vcpu_run failed: {:#x}",
                observation.run_status
            ));
        }
    }

    if continuation_exit_observed {
        let mut value = 0;
        let status = unsafe { hv_vcpu_get_reg(vcpu, HV_REG_X0, &mut value) };
        emulated_value_read_status = Some(status);
        if status == HV_SUCCESS {
            emulated_value_after_continue = Some(value);
            emulated_value_preserved = value == EMULATED_MMIO_READ_VALUE;
            if !emulated_value_preserved {
                blockers.push(format!(
                    "emulated MMIO value changed before continuation HVC: {value:#x}"
                ));
            }
        } else {
            blockers.push(format!("hv_vcpu_get_reg(X0) failed: {status:#x}"));
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

    if memory_mapped {
        let status = unsafe { hv_vm_unmap(PROBE_IPA_START, PROBE_BYTES) };
        unmap_status = Some(status);
        memory_unmapped = status == HV_SUCCESS;
        if !memory_unmapped {
            blockers.push(format!("hv_vm_unmap failed: {status:#x}"));
        }
    }

    if vm_created {
        let status = unsafe { hv_vm_destroy() };
        vm_destroy_status = Some(status);
        vm_destroyed = status == HV_SUCCESS;
        if !vm_destroyed {
            blockers.push(format!("hv_vm_destroy failed: {status:#x}"));
        }
    }

    if memory_allocated {
        let status = unsafe { hv_vm_deallocate(memory, PROBE_BYTES) };
        deallocate_status = Some(status);
        memory_deallocated = status == HV_SUCCESS;
        if !memory_deallocated {
            blockers.push(format!("hv_vm_deallocate failed: {status:#x}"));
        }
    }

    let watchdog_cancel_fired =
        first_watchdog_cancel_status.is_some() || second_watchdog_cancel_status.is_some();

    HvfMmioReadEmulationProbe {
        allowed: true,
        attempted: true,
        vm_created,
        memory_allocated,
        memory_mapped,
        vcpu_created,
        pc_set,
        cpsr_set,
        address_register_set,
        first_run_attempted,
        mmio_exit_observed,
        pc_read_after_mmio_exit,
        emulated_value_injected,
        pc_advanced,
        second_run_attempted,
        continuation_exit_observed,
        emulated_value_preserved,
        watchdog_cancel_fired,
        vcpu_destroyed,
        memory_unmapped,
        vm_destroyed,
        memory_deallocated,
        host,
        code_ipa_start: PROBE_IPA_START,
        mmio_ipa: PROBE_MMIO_IPA,
        bytes: PROBE_BYTES,
        instructions: "LDR X0, [X1]; HVC #0",
        emulated_value: EMULATED_MMIO_READ_VALUE,
        vm_create_status,
        allocate_status,
        map_status,
        vcpu_create_status,
        pc_set_status,
        cpsr_set_status,
        address_register_set_status,
        first_run_status,
        mmio_exit_reason,
        mmio_exit_syndrome,
        mmio_exit_virtual_address,
        mmio_exit_physical_address,
        first_watchdog_cancel_status,
        pc_read_status,
        pc_after_mmio_exit,
        emulated_value_set_status,
        pc_advance_status,
        second_run_status,
        continuation_exit_reason,
        continuation_exit_syndrome,
        continuation_exit_virtual_address,
        continuation_exit_physical_address,
        second_watchdog_cancel_status,
        emulated_value_read_status,
        emulated_value_after_continue,
        vcpu_destroy_status,
        unmap_status,
        vm_destroy_status,
        deallocate_status,
        blockers,
    }
}

pub(crate) fn mmio_read_emulation_probe_result(
    allowed: bool,
    attempted: bool,
    host: HvfHostCapabilities,
    blockers: Vec<String>,
) -> HvfMmioReadEmulationProbe {
    HvfMmioReadEmulationProbe {
        allowed,
        attempted,
        vm_created: false,
        memory_allocated: false,
        memory_mapped: false,
        vcpu_created: false,
        pc_set: false,
        cpsr_set: false,
        address_register_set: false,
        first_run_attempted: false,
        mmio_exit_observed: false,
        pc_read_after_mmio_exit: false,
        emulated_value_injected: false,
        pc_advanced: false,
        second_run_attempted: false,
        continuation_exit_observed: false,
        emulated_value_preserved: false,
        watchdog_cancel_fired: false,
        vcpu_destroyed: false,
        memory_unmapped: false,
        vm_destroyed: false,
        memory_deallocated: false,
        host,
        code_ipa_start: PROBE_IPA_START,
        mmio_ipa: PROBE_MMIO_IPA,
        bytes: PROBE_BYTES,
        instructions: "LDR X0, [X1]; HVC #0",
        emulated_value: EMULATED_MMIO_READ_VALUE,
        vm_create_status: None,
        allocate_status: None,
        map_status: None,
        vcpu_create_status: None,
        pc_set_status: None,
        cpsr_set_status: None,
        address_register_set_status: None,
        first_run_status: None,
        mmio_exit_reason: None,
        mmio_exit_syndrome: None,
        mmio_exit_virtual_address: None,
        mmio_exit_physical_address: None,
        first_watchdog_cancel_status: None,
        pc_read_status: None,
        pc_after_mmio_exit: None,
        emulated_value_set_status: None,
        pc_advance_status: None,
        second_run_status: None,
        continuation_exit_reason: None,
        continuation_exit_syndrome: None,
        continuation_exit_virtual_address: None,
        continuation_exit_physical_address: None,
        second_watchdog_cancel_status: None,
        emulated_value_read_status: None,
        emulated_value_after_continue: None,
        vcpu_destroy_status: None,
        unmap_status: None,
        vm_destroy_status: None,
        deallocate_status: None,
        blockers,
    }
}

pub fn probe_hvf_mmio_write_emulation(
    allow_emulate: bool,
    host: HvfHostCapabilities,
) -> HvfMmioWriteEmulationProbe {
    let mut blockers = Vec::new();

    if !allow_emulate {
        blockers.push(
            "set BRIDGEVM_HVF_ALLOW_MMIO_WRITE_EMULATION=1 or pass --allow-emulate to handle one unmapped STR write, capture X0, advance PC, and continue to HVC".to_string(),
        );
        return mmio_write_emulation_probe_result(false, false, host, blockers);
    }

    if !host.available {
        blockers.push("Hypervisor.framework host capabilities are not available".to_string());
        return mmio_write_emulation_probe_result(true, false, host, blockers);
    }

    let mut memory_allocated = false;
    let mut memory_mapped = false;
    let mut vcpu_created = false;
    let mut pc_set = false;
    let mut cpsr_set = false;
    let mut write_value_register_set = false;
    let mut address_register_set = false;
    let mut first_run_attempted = false;
    let mut mmio_exit_observed = false;
    let mut pc_read_after_mmio_exit = false;
    let mut write_value_captured = false;
    let mut pc_advanced = false;
    let mut second_run_attempted = false;
    let mut continuation_exit_observed = false;
    let mut write_value_preserved = false;
    let mut vcpu_destroyed = false;
    let mut memory_unmapped = false;
    let mut vm_destroyed = false;
    let mut memory_deallocated = false;

    let mut allocate_status = None;
    let mut map_status = None;
    let mut vcpu_create_status = None;
    let mut pc_set_status = None;
    let mut cpsr_set_status = None;
    let mut write_value_register_set_status = None;
    let mut address_register_set_status = None;
    let mut first_run_status = None;
    let mut mmio_exit_reason = None;
    let mut mmio_exit_syndrome = None;
    let mut mmio_exit_virtual_address = None;
    let mut mmio_exit_physical_address = None;
    let mut first_watchdog_cancel_status = None;
    let mut pc_read_status = None;
    let mut pc_after_mmio_exit = None;
    let mut write_value_capture_status = None;
    let mut captured_write_value = None;
    let mut pc_advance_status = None;
    let mut second_run_status = None;
    let mut continuation_exit_reason = None;
    let mut continuation_exit_syndrome = None;
    let mut continuation_exit_virtual_address = None;
    let mut continuation_exit_physical_address = None;
    let mut second_watchdog_cancel_status = None;
    let mut write_value_after_continue_status = None;
    let mut write_value_after_continue = None;
    let mut vcpu_destroy_status = None;
    let mut unmap_status = None;
    let mut vm_destroy_status = None;
    let mut deallocate_status = None;

    let mut memory = ptr::null_mut();
    let mut vcpu = 0;
    let mut exit = ptr::null_mut();

    let status = unsafe { hv_vm_create(ptr::null_mut()) };
    let vm_create_status = Some(status);
    let vm_created = status == HV_SUCCESS;
    if !vm_created {
        blockers.push(format!("hv_vm_create failed: {status:#x}"));
    }

    if vm_created {
        let status = unsafe { hv_vm_allocate(&mut memory, PROBE_BYTES, HV_ALLOCATE_DEFAULT) };
        allocate_status = Some(status);
        memory_allocated = status == HV_SUCCESS && !memory.is_null();
        if memory_allocated {
            let store = AARCH64_STR_X0_TO_X1.to_le_bytes();
            let hvc = AARCH64_HVC_0.to_le_bytes();
            unsafe {
                ptr::copy_nonoverlapping(store.as_ptr(), memory.cast::<u8>(), store.len());
                ptr::copy_nonoverlapping(
                    hvc.as_ptr(),
                    memory.cast::<u8>().add(store.len()),
                    hvc.len(),
                );
            }
        } else {
            blockers.push(format!("hv_vm_allocate failed: {status:#x}"));
        }
    }

    if vm_created && memory_allocated {
        let status = unsafe {
            hv_vm_map(
                memory,
                PROBE_IPA_START,
                PROBE_BYTES,
                HV_MEMORY_READ | HV_MEMORY_WRITE | HV_MEMORY_EXEC,
            )
        };
        map_status = Some(status);
        memory_mapped = status == HV_SUCCESS;
        if !memory_mapped {
            blockers.push(format!("hv_vm_map failed: {status:#x}"));
        }
    }

    if vm_created && memory_mapped {
        let status = unsafe { hv_vcpu_create(&mut vcpu, &mut exit, ptr::null_mut()) };
        vcpu_create_status = Some(status);
        vcpu_created = status == HV_SUCCESS;
        if !vcpu_created {
            blockers.push(format!("hv_vcpu_create failed: {status:#x}"));
        }
    }

    if vcpu_created {
        let status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_PC, PROBE_IPA_START) };
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

    if vcpu_created {
        let status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_X0, EMULATED_MMIO_WRITE_VALUE) };
        write_value_register_set_status = Some(status);
        write_value_register_set = status == HV_SUCCESS;
        if !write_value_register_set {
            blockers.push(format!("hv_vcpu_set_reg(X0) failed: {status:#x}"));
        }
    }

    if vcpu_created {
        let status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_X1, PROBE_MMIO_IPA) };
        address_register_set_status = Some(status);
        address_register_set = status == HV_SUCCESS;
        if !address_register_set {
            blockers.push(format!("hv_vcpu_set_reg(X1) failed: {status:#x}"));
        }
    }

    if vcpu_created && pc_set && cpsr_set && write_value_register_set && address_register_set {
        first_run_attempted = true;
        let observation = run_vcpu_once_with_watchdog(vcpu, exit);
        first_run_status = Some(observation.run_status);
        mmio_exit_reason = observation.exit_reason;
        mmio_exit_syndrome = observation.exit_syndrome;
        mmio_exit_virtual_address = observation.exit_virtual_address;
        mmio_exit_physical_address = observation.exit_physical_address;
        first_watchdog_cancel_status = observation.watchdog_cancel_status;
        if first_watchdog_cancel_status.is_some() {
            blockers.push(
                "MMIO write emulation first run watchdog fired before exception exit".to_string(),
            );
        }

        if observation.run_status == HV_SUCCESS {
            if mmio_exit_reason.is_none() {
                blockers.push(
                    "first hv_vcpu_run returned success without an exit info pointer".to_string(),
                );
            } else {
                mmio_exit_observed = mmio_exit_reason == Some(HV_EXIT_REASON_EXCEPTION)
                    && (mmio_exit_virtual_address == Some(PROBE_MMIO_IPA)
                        || mmio_exit_physical_address == Some(PROBE_MMIO_IPA)
                        || mmio_exit_syndrome.is_some_and(is_data_abort_syndrome));
                if !mmio_exit_observed {
                    blockers.push(format!(
                        "first hv_vcpu_run did not report an MMIO/data-abort style write exit for {PROBE_MMIO_IPA:#x}"
                    ));
                }
            }
        } else {
            blockers.push(format!(
                "first hv_vcpu_run failed: {:#x}",
                observation.run_status
            ));
        }
    }

    if mmio_exit_observed {
        let mut pc = 0;
        let status = unsafe { hv_vcpu_get_reg(vcpu, HV_REG_PC, &mut pc) };
        pc_read_status = Some(status);
        pc_read_after_mmio_exit = status == HV_SUCCESS;
        if pc_read_after_mmio_exit {
            pc_after_mmio_exit = Some(pc);
        } else {
            blockers.push(format!("hv_vcpu_get_reg(PC) failed: {status:#x}"));
        }
    }

    if mmio_exit_observed {
        let mut value = 0;
        let status = unsafe { hv_vcpu_get_reg(vcpu, HV_REG_X0, &mut value) };
        write_value_capture_status = Some(status);
        if status == HV_SUCCESS {
            captured_write_value = Some(value);
            write_value_captured = value == EMULATED_MMIO_WRITE_VALUE;
            if !write_value_captured {
                blockers.push(format!(
                    "captured MMIO write value did not match X0 seed: {value:#x}"
                ));
            }
        } else {
            blockers.push(format!("hv_vcpu_get_reg(X0) failed: {status:#x}"));
        }
    }

    if pc_read_after_mmio_exit && write_value_captured {
        let status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_PC, PROBE_IPA_START + 4) };
        pc_advance_status = Some(status);
        pc_advanced = status == HV_SUCCESS;
        if !pc_advanced {
            blockers.push(format!("hv_vcpu_set_reg(PC + 4) failed: {status:#x}"));
        }
    }

    if pc_advanced {
        second_run_attempted = true;
        let observation = run_vcpu_once_with_watchdog(vcpu, exit);
        second_run_status = Some(observation.run_status);
        continuation_exit_reason = observation.exit_reason;
        continuation_exit_syndrome = observation.exit_syndrome;
        continuation_exit_virtual_address = observation.exit_virtual_address;
        continuation_exit_physical_address = observation.exit_physical_address;
        second_watchdog_cancel_status = observation.watchdog_cancel_status;
        if second_watchdog_cancel_status.is_some() {
            blockers
                .push("MMIO write emulation second run watchdog fired before HVC exit".to_string());
        }

        if observation.run_status == HV_SUCCESS {
            if continuation_exit_reason.is_none() {
                blockers.push(
                    "second hv_vcpu_run returned success without an exit info pointer".to_string(),
                );
            } else {
                continuation_exit_observed = continuation_exit_reason
                    == Some(HV_EXIT_REASON_EXCEPTION)
                    && continuation_exit_syndrome == Some(AARCH64_HVC_0_SYNDROME);
                if !continuation_exit_observed {
                    blockers.push(format!(
                        "second hv_vcpu_run did not reach HVC continuation exit; syndrome: {}",
                        continuation_exit_syndrome.map_or_else(
                            || "not observed".to_string(),
                            |value| { format!("{value:#x}") }
                        )
                    ));
                }
            }
        } else {
            blockers.push(format!(
                "second hv_vcpu_run failed: {:#x}",
                observation.run_status
            ));
        }
    }

    if continuation_exit_observed {
        let mut value = 0;
        let status = unsafe { hv_vcpu_get_reg(vcpu, HV_REG_X0, &mut value) };
        write_value_after_continue_status = Some(status);
        if status == HV_SUCCESS {
            write_value_after_continue = Some(value);
            write_value_preserved = value == EMULATED_MMIO_WRITE_VALUE;
            if !write_value_preserved {
                blockers.push(format!(
                    "MMIO write value changed before continuation HVC: {value:#x}"
                ));
            }
        } else {
            blockers.push(format!("hv_vcpu_get_reg(X0) failed: {status:#x}"));
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

    if memory_mapped {
        let status = unsafe { hv_vm_unmap(PROBE_IPA_START, PROBE_BYTES) };
        unmap_status = Some(status);
        memory_unmapped = status == HV_SUCCESS;
        if !memory_unmapped {
            blockers.push(format!("hv_vm_unmap failed: {status:#x}"));
        }
    }

    if vm_created {
        let status = unsafe { hv_vm_destroy() };
        vm_destroy_status = Some(status);
        vm_destroyed = status == HV_SUCCESS;
        if !vm_destroyed {
            blockers.push(format!("hv_vm_destroy failed: {status:#x}"));
        }
    }

    if memory_allocated {
        let status = unsafe { hv_vm_deallocate(memory, PROBE_BYTES) };
        deallocate_status = Some(status);
        memory_deallocated = status == HV_SUCCESS;
        if !memory_deallocated {
            blockers.push(format!("hv_vm_deallocate failed: {status:#x}"));
        }
    }

    let watchdog_cancel_fired =
        first_watchdog_cancel_status.is_some() || second_watchdog_cancel_status.is_some();

    HvfMmioWriteEmulationProbe {
        allowed: true,
        attempted: true,
        vm_created,
        memory_allocated,
        memory_mapped,
        vcpu_created,
        pc_set,
        cpsr_set,
        write_value_register_set,
        address_register_set,
        first_run_attempted,
        mmio_exit_observed,
        pc_read_after_mmio_exit,
        write_value_captured,
        pc_advanced,
        second_run_attempted,
        continuation_exit_observed,
        write_value_preserved,
        watchdog_cancel_fired,
        vcpu_destroyed,
        memory_unmapped,
        vm_destroyed,
        memory_deallocated,
        host,
        code_ipa_start: PROBE_IPA_START,
        mmio_ipa: PROBE_MMIO_IPA,
        bytes: PROBE_BYTES,
        instructions: "STR X0, [X1]; HVC #0",
        write_value: EMULATED_MMIO_WRITE_VALUE,
        vm_create_status,
        allocate_status,
        map_status,
        vcpu_create_status,
        pc_set_status,
        cpsr_set_status,
        write_value_register_set_status,
        address_register_set_status,
        first_run_status,
        mmio_exit_reason,
        mmio_exit_syndrome,
        mmio_exit_virtual_address,
        mmio_exit_physical_address,
        first_watchdog_cancel_status,
        pc_read_status,
        pc_after_mmio_exit,
        write_value_capture_status,
        captured_write_value,
        pc_advance_status,
        second_run_status,
        continuation_exit_reason,
        continuation_exit_syndrome,
        continuation_exit_virtual_address,
        continuation_exit_physical_address,
        second_watchdog_cancel_status,
        write_value_after_continue_status,
        write_value_after_continue,
        vcpu_destroy_status,
        unmap_status,
        vm_destroy_status,
        deallocate_status,
        blockers,
    }
}

pub(crate) fn mmio_write_emulation_probe_result(
    allowed: bool,
    attempted: bool,
    host: HvfHostCapabilities,
    blockers: Vec<String>,
) -> HvfMmioWriteEmulationProbe {
    HvfMmioWriteEmulationProbe {
        allowed,
        attempted,
        vm_created: false,
        memory_allocated: false,
        memory_mapped: false,
        vcpu_created: false,
        pc_set: false,
        cpsr_set: false,
        write_value_register_set: false,
        address_register_set: false,
        first_run_attempted: false,
        mmio_exit_observed: false,
        pc_read_after_mmio_exit: false,
        write_value_captured: false,
        pc_advanced: false,
        second_run_attempted: false,
        continuation_exit_observed: false,
        write_value_preserved: false,
        watchdog_cancel_fired: false,
        vcpu_destroyed: false,
        memory_unmapped: false,
        vm_destroyed: false,
        memory_deallocated: false,
        host,
        code_ipa_start: PROBE_IPA_START,
        mmio_ipa: PROBE_MMIO_IPA,
        bytes: PROBE_BYTES,
        instructions: "STR X0, [X1]; HVC #0",
        write_value: EMULATED_MMIO_WRITE_VALUE,
        vm_create_status: None,
        allocate_status: None,
        map_status: None,
        vcpu_create_status: None,
        pc_set_status: None,
        cpsr_set_status: None,
        write_value_register_set_status: None,
        address_register_set_status: None,
        first_run_status: None,
        mmio_exit_reason: None,
        mmio_exit_syndrome: None,
        mmio_exit_virtual_address: None,
        mmio_exit_physical_address: None,
        first_watchdog_cancel_status: None,
        pc_read_status: None,
        pc_after_mmio_exit: None,
        write_value_capture_status: None,
        captured_write_value: None,
        pc_advance_status: None,
        second_run_status: None,
        continuation_exit_reason: None,
        continuation_exit_syndrome: None,
        continuation_exit_virtual_address: None,
        continuation_exit_physical_address: None,
        second_watchdog_cancel_status: None,
        write_value_after_continue_status: None,
        write_value_after_continue: None,
        vcpu_destroy_status: None,
        unmap_status: None,
        vm_destroy_status: None,
        deallocate_status: None,
        blockers,
    }
}
