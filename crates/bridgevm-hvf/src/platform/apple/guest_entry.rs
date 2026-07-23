//! Guest entry and guest exit-loop probes.
//!
//! Split out of the single 12,111-line apple.rs backend.

use super::*;
use crate::*;

pub fn probe_hvf_guest_entry(allow_entry: bool, host: HvfHostCapabilities) -> HvfGuestEntryProbe {
    let mut blockers = Vec::new();

    if !allow_entry {
        blockers.push(
            "set BRIDGEVM_HVF_ALLOW_GUEST_ENTRY=1 or pass --allow-entry to map one HVC instruction, set PC/CPSR, and run with a watchdog".to_string(),
        );
        return guest_entry_probe_result(false, false, host, blockers);
    }

    if !host.available {
        blockers.push("Hypervisor.framework host capabilities are not available".to_string());
        return guest_entry_probe_result(true, false, host, blockers);
    }

    let mut memory_allocated = false;
    let mut memory_mapped = false;
    let mut vcpu_created = false;
    let mut pc_set = false;
    let mut cpsr_set = false;
    let mut run_attempted = false;
    let mut entry_boundary_observed = false;
    let mut watchdog_cancel_fired = false;
    let mut vcpu_destroyed = false;
    let mut memory_unmapped = false;
    let mut vm_destroyed = false;
    let mut memory_deallocated = false;

    let mut allocate_status = None;
    let mut map_status = None;
    let mut vcpu_create_status = None;
    let mut pc_set_status = None;
    let mut cpsr_set_status = None;
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
            let instruction = AARCH64_HVC_0.to_le_bytes();
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

    if vcpu_created && pc_set && cpsr_set {
        run_attempted = true;
        let done = Arc::new(AtomicBool::new(false));
        let watchdog_done = Arc::clone(&done);
        let vcpu_for_watchdog = vcpu;
        let watchdog = thread::spawn(move || {
            for _ in 0..100 {
                if watchdog_done.load(Ordering::SeqCst) {
                    return None;
                }
                thread::sleep(Duration::from_millis(1));
            }
            let mut vcpu = vcpu_for_watchdog;
            Some(unsafe { hv_vcpus_exit(&mut vcpu, 1) })
        });

        let status = unsafe { hv_vcpu_run(vcpu) };
        run_status = Some(status);
        done.store(true, Ordering::SeqCst);
        watchdog_cancel_status = watchdog.join().ok().flatten();
        watchdog_cancel_fired = watchdog_cancel_status.is_some();

        if status == HV_SUCCESS {
            if exit.is_null() {
                blockers
                    .push("hv_vcpu_run returned success without an exit info pointer".to_string());
            } else {
                let exit_info = unsafe { &*exit };
                exit_reason = Some(exit_info.reason);
                exit_syndrome = Some(exit_info.exception.syndrome);
                exit_virtual_address = Some(exit_info.exception.virtual_address);
                exit_physical_address = Some(exit_info.exception.physical_address);
                entry_boundary_observed = exit_reason == Some(1);
                if !entry_boundary_observed {
                    blockers.push(format!(
                        "hv_vcpu_run returned non-exception exit reason: {}",
                        exit_reason.unwrap_or_default()
                    ));
                }
            }
        } else {
            blockers.push(format!("hv_vcpu_run failed: {status:#x}"));
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

    HvfGuestEntryProbe {
        allowed: true,
        attempted: true,
        vm_created,
        memory_allocated,
        memory_mapped,
        vcpu_created,
        pc_set,
        cpsr_set,
        run_attempted,
        entry_boundary_observed,
        watchdog_cancel_fired,
        vcpu_destroyed,
        memory_unmapped,
        vm_destroyed,
        memory_deallocated,
        host,
        ipa_start: PROBE_IPA_START,
        bytes: PROBE_BYTES,
        instruction: "HVC #0",
        vm_create_status,
        allocate_status,
        map_status,
        vcpu_create_status,
        pc_set_status,
        cpsr_set_status,
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

pub(crate) fn guest_entry_probe_result(
    allowed: bool,
    attempted: bool,
    host: HvfHostCapabilities,
    blockers: Vec<String>,
) -> HvfGuestEntryProbe {
    HvfGuestEntryProbe {
        allowed,
        attempted,
        vm_created: false,
        memory_allocated: false,
        memory_mapped: false,
        vcpu_created: false,
        pc_set: false,
        cpsr_set: false,
        run_attempted: false,
        entry_boundary_observed: false,
        watchdog_cancel_fired: false,
        vcpu_destroyed: false,
        memory_unmapped: false,
        vm_destroyed: false,
        memory_deallocated: false,
        host,
        ipa_start: PROBE_IPA_START,
        bytes: PROBE_BYTES,
        instruction: "HVC #0",
        vm_create_status: None,
        allocate_status: None,
        map_status: None,
        vcpu_create_status: None,
        pc_set_status: None,
        cpsr_set_status: None,
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

pub fn probe_hvf_guest_exit_loop(
    allow_loop: bool,
    host: HvfHostCapabilities,
) -> HvfGuestExitLoopProbe {
    let mut blockers = Vec::new();

    if !allow_loop {
        blockers.push(
            "set BRIDGEVM_HVF_ALLOW_EXIT_LOOP=1 or pass --allow-loop to run two HVC exits with an explicit PC advance".to_string(),
        );
        return guest_exit_loop_probe_result(false, false, host, blockers);
    }

    if !host.available {
        blockers.push("Hypervisor.framework host capabilities are not available".to_string());
        return guest_exit_loop_probe_result(true, false, host, blockers);
    }

    let mut memory_allocated = false;
    let mut memory_mapped = false;
    let mut vcpu_created = false;
    let mut initial_pc_set = false;
    let mut cpsr_set = false;
    let mut first_run_attempted = false;
    let mut first_exit_observed = false;
    let mut pc_read_after_first_exit = false;
    let mut pc_advanced = false;
    let mut second_run_attempted = false;
    let mut second_exit_observed = false;
    let mut vcpu_destroyed = false;
    let mut memory_unmapped = false;
    let mut vm_destroyed = false;
    let mut memory_deallocated = false;

    let mut allocate_status = None;
    let mut map_status = None;
    let mut vcpu_create_status = None;
    let mut initial_pc_set_status = None;
    let mut cpsr_set_status = None;
    let mut first_run_status = None;
    let mut first_exit_reason = None;
    let mut first_exit_syndrome = None;
    let mut first_exit_virtual_address = None;
    let mut first_exit_physical_address = None;
    let mut first_watchdog_cancel_status = None;
    let mut pc_read_status = None;
    let mut pc_after_first_exit = None;
    let mut pc_advance_status = None;
    let mut second_run_status = None;
    let mut second_exit_reason = None;
    let mut second_exit_syndrome = None;
    let mut second_exit_virtual_address = None;
    let mut second_exit_physical_address = None;
    let mut second_watchdog_cancel_status = None;
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
            let first = AARCH64_HVC_0.to_le_bytes();
            let second = AARCH64_HVC_1.to_le_bytes();
            unsafe {
                ptr::copy_nonoverlapping(first.as_ptr(), memory.cast::<u8>(), first.len());
                ptr::copy_nonoverlapping(
                    second.as_ptr(),
                    memory.cast::<u8>().add(first.len()),
                    second.len(),
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
        initial_pc_set_status = Some(status);
        initial_pc_set = status == HV_SUCCESS;
        if !initial_pc_set {
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

    if vcpu_created && initial_pc_set && cpsr_set {
        first_run_attempted = true;
        let first = run_vcpu_once_with_watchdog(vcpu, exit);
        first_run_status = Some(first.run_status);
        first_exit_reason = first.exit_reason;
        first_exit_syndrome = first.exit_syndrome;
        first_exit_virtual_address = first.exit_virtual_address;
        first_exit_physical_address = first.exit_physical_address;
        first_watchdog_cancel_status = first.watchdog_cancel_status;
        if first_watchdog_cancel_status.is_some() {
            blockers.push("first run watchdog fired before guest exception exit".to_string());
        }

        if first.run_status == HV_SUCCESS {
            if first_exit_reason.is_none() {
                blockers.push(
                    "first hv_vcpu_run returned success without an exit info pointer".to_string(),
                );
            } else {
                first_exit_observed = first_exit_reason == Some(HV_EXIT_REASON_EXCEPTION)
                    && first_exit_syndrome == Some(AARCH64_HVC_0_SYNDROME);
                if first_exit_reason != Some(HV_EXIT_REASON_EXCEPTION) {
                    blockers.push(format!(
                        "first hv_vcpu_run returned non-exception exit reason: {}",
                        first_exit_reason.unwrap_or_default()
                    ));
                }
                if first_exit_syndrome != Some(AARCH64_HVC_0_SYNDROME) {
                    blockers.push(format!(
                        "first hv_vcpu_run returned unexpected syndrome: {}",
                        first_exit_syndrome.map_or_else(
                            || "not observed".to_string(),
                            |value| { format!("{value:#x}") }
                        )
                    ));
                }
            }
        } else {
            blockers.push(format!("first hv_vcpu_run failed: {:#x}", first.run_status));
        }
    }

    if first_exit_observed {
        let mut pc = 0;
        let status = unsafe { hv_vcpu_get_reg(vcpu, HV_REG_PC, &mut pc) };
        pc_read_status = Some(status);
        pc_read_after_first_exit = status == HV_SUCCESS;
        if pc_read_after_first_exit {
            pc_after_first_exit = Some(pc);
        } else {
            blockers.push(format!("hv_vcpu_get_reg(PC) failed: {status:#x}"));
        }
    }

    if pc_read_after_first_exit {
        let status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_PC, PROBE_IPA_START + 4) };
        pc_advance_status = Some(status);
        pc_advanced = status == HV_SUCCESS;
        if !pc_advanced {
            blockers.push(format!("hv_vcpu_set_reg(PC + 4) failed: {status:#x}"));
        }
    }

    if pc_advanced {
        second_run_attempted = true;
        let second = run_vcpu_once_with_watchdog(vcpu, exit);
        second_run_status = Some(second.run_status);
        second_exit_reason = second.exit_reason;
        second_exit_syndrome = second.exit_syndrome;
        second_exit_virtual_address = second.exit_virtual_address;
        second_exit_physical_address = second.exit_physical_address;
        second_watchdog_cancel_status = second.watchdog_cancel_status;
        if second_watchdog_cancel_status.is_some() {
            blockers.push("second run watchdog fired before guest exception exit".to_string());
        }

        if second.run_status == HV_SUCCESS {
            if second_exit_reason.is_none() {
                blockers.push(
                    "second hv_vcpu_run returned success without an exit info pointer".to_string(),
                );
            } else {
                second_exit_observed = second_exit_reason == Some(HV_EXIT_REASON_EXCEPTION)
                    && second_exit_syndrome == Some(AARCH64_HVC_1_SYNDROME);
                if second_exit_reason != Some(HV_EXIT_REASON_EXCEPTION) {
                    blockers.push(format!(
                        "second hv_vcpu_run returned non-exception exit reason: {}",
                        second_exit_reason.unwrap_or_default()
                    ));
                }
                if second_exit_syndrome != Some(AARCH64_HVC_1_SYNDROME) {
                    blockers.push(format!(
                        "second hv_vcpu_run returned unexpected syndrome: {}",
                        second_exit_syndrome.map_or_else(
                            || "not observed".to_string(),
                            |value| { format!("{value:#x}") }
                        )
                    ));
                }
            }
        } else {
            blockers.push(format!(
                "second hv_vcpu_run failed: {:#x}",
                second.run_status
            ));
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
    let exit_loop_observed = first_exit_observed && pc_advanced && second_exit_observed;

    HvfGuestExitLoopProbe {
        allowed: true,
        attempted: true,
        vm_created,
        memory_allocated,
        memory_mapped,
        vcpu_created,
        initial_pc_set,
        cpsr_set,
        first_run_attempted,
        first_exit_observed,
        pc_read_after_first_exit,
        pc_advanced,
        second_run_attempted,
        second_exit_observed,
        exit_loop_observed,
        watchdog_cancel_fired,
        vcpu_destroyed,
        memory_unmapped,
        vm_destroyed,
        memory_deallocated,
        host,
        ipa_start: PROBE_IPA_START,
        bytes: PROBE_BYTES,
        instructions: "HVC #0; HVC #1",
        vm_create_status,
        allocate_status,
        map_status,
        vcpu_create_status,
        initial_pc_set_status,
        cpsr_set_status,
        first_run_status,
        first_exit_reason,
        first_exit_syndrome,
        first_exit_virtual_address,
        first_exit_physical_address,
        first_watchdog_cancel_status,
        pc_read_status,
        pc_after_first_exit,
        pc_advance_status,
        second_run_status,
        second_exit_reason,
        second_exit_syndrome,
        second_exit_virtual_address,
        second_exit_physical_address,
        second_watchdog_cancel_status,
        vcpu_destroy_status,
        unmap_status,
        vm_destroy_status,
        deallocate_status,
        blockers,
    }
}

pub(crate) fn guest_exit_loop_probe_result(
    allowed: bool,
    attempted: bool,
    host: HvfHostCapabilities,
    blockers: Vec<String>,
) -> HvfGuestExitLoopProbe {
    HvfGuestExitLoopProbe {
        allowed,
        attempted,
        vm_created: false,
        memory_allocated: false,
        memory_mapped: false,
        vcpu_created: false,
        initial_pc_set: false,
        cpsr_set: false,
        first_run_attempted: false,
        first_exit_observed: false,
        pc_read_after_first_exit: false,
        pc_advanced: false,
        second_run_attempted: false,
        second_exit_observed: false,
        exit_loop_observed: false,
        watchdog_cancel_fired: false,
        vcpu_destroyed: false,
        memory_unmapped: false,
        vm_destroyed: false,
        memory_deallocated: false,
        host,
        ipa_start: PROBE_IPA_START,
        bytes: PROBE_BYTES,
        instructions: "HVC #0; HVC #1",
        vm_create_status: None,
        allocate_status: None,
        map_status: None,
        vcpu_create_status: None,
        initial_pc_set_status: None,
        cpsr_set_status: None,
        first_run_status: None,
        first_exit_reason: None,
        first_exit_syndrome: None,
        first_exit_virtual_address: None,
        first_exit_physical_address: None,
        first_watchdog_cancel_status: None,
        pc_read_status: None,
        pc_after_first_exit: None,
        pc_advance_status: None,
        second_run_status: None,
        second_exit_reason: None,
        second_exit_syndrome: None,
        second_exit_virtual_address: None,
        second_exit_physical_address: None,
        second_watchdog_cancel_status: None,
        vcpu_destroy_status: None,
        unmap_status: None,
        vm_destroy_status: None,
        deallocate_status: None,
        blockers,
    }
}
