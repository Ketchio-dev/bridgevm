//! Split out of mmio_emulation.rs by responsibility.

use super::super::*;
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
