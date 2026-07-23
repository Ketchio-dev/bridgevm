//! Split out of lifecycle.rs by responsibility.

use super::super::*;
use crate::*;

pub fn probe_hvf_vtimer_exit(allow_probe: bool, host: HvfHostCapabilities) -> HvfVtimerExitProbe {
    let mut blockers = Vec::new();
    let vtimer_offset_value = 0;
    let cntv_cval_value = 0;
    let cntv_ctl_value = 1;

    if !allow_probe {
        blockers.push(
            "set BRIDGEVM_HVF_ALLOW_VTIMER_EXIT=1 or pass --allow-vtimer-exit to map a tiny WFI guest, program CNTV_CVAL_EL0/CNTV_CTL_EL0, and observe a real HV_EXIT_REASON_VTIMER_ACTIVATED boundary".to_string(),
        );
        return vtimer_exit_probe_result(false, false, host, blockers);
    }

    if !host.available {
        blockers.push("Hypervisor.framework host capabilities are not available".to_string());
        return vtimer_exit_probe_result(true, false, host, blockers);
    }

    let mut memory_allocated = false;
    let mut memory_mapped = false;
    let mut vcpu_created = false;
    let mut pc_set = false;
    let mut cpsr_set = false;
    let mut vtimer_offset_set = false;
    let mut cntv_cval_set = false;
    let mut cntv_ctl_set = false;
    let mut vtimer_unmasked = false;
    let mut run_attempted = false;
    let mut vtimer_exit_observed = false;
    let mut pending_irq_injected = false;
    let mut vtimer_mask_observed_after_exit = None;
    let mut vtimer_unmasked_after_exit = false;
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
    let mut vtimer_offset_set_status = None;
    let mut cntv_cval_set_status = None;
    let mut cntv_ctl_set_status = None;
    let mut vtimer_unmask_status = None;
    let mut run_status = None;
    let mut exit_reason = None;
    let mut exit_syndrome = None;
    let mut exit_virtual_address = None;
    let mut exit_physical_address = None;
    let mut watchdog_cancel_status = None;
    let mut pending_irq_set_status = None;
    let mut vtimer_mask_get_after_exit_status = None;
    let mut vtimer_unmask_after_exit_status = None;
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
            let instructions = [AARCH64_WFI, AARCH64_HVC_0];
            let mut bytes = Vec::with_capacity(instructions.len() * 4);
            for instruction in instructions {
                bytes.extend_from_slice(&instruction.to_le_bytes());
            }
            unsafe {
                ptr::copy_nonoverlapping(bytes.as_ptr(), memory.cast::<u8>(), bytes.len());
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
        let status = unsafe { hv_vcpu_set_vtimer_offset(vcpu, vtimer_offset_value) };
        vtimer_offset_set_status = Some(status);
        vtimer_offset_set = status == HV_SUCCESS;
        if !vtimer_offset_set {
            blockers.push(format!("hv_vcpu_set_vtimer_offset failed: {status:#x}"));
        }
    }

    if vcpu_created {
        let status =
            unsafe { hv_vcpu_set_sys_reg(vcpu, HV_SYS_REG_CNTV_CVAL_EL0, cntv_cval_value) };
        cntv_cval_set_status = Some(status);
        cntv_cval_set = status == HV_SUCCESS;
        if !cntv_cval_set {
            blockers.push(format!(
                "hv_vcpu_set_sys_reg(CNTV_CVAL_EL0) failed: {status:#x}"
            ));
        }
    }

    if vcpu_created {
        let status = unsafe { hv_vcpu_set_sys_reg(vcpu, HV_SYS_REG_CNTV_CTL_EL0, cntv_ctl_value) };
        cntv_ctl_set_status = Some(status);
        cntv_ctl_set = status == HV_SUCCESS;
        if !cntv_ctl_set {
            blockers.push(format!(
                "hv_vcpu_set_sys_reg(CNTV_CTL_EL0) failed: {status:#x}"
            ));
        }
    }

    if vcpu_created {
        let status = unsafe { hv_vcpu_set_vtimer_mask(vcpu, false) };
        vtimer_unmask_status = Some(status);
        vtimer_unmasked = status == HV_SUCCESS;
        if !vtimer_unmasked {
            blockers.push(format!(
                "hv_vcpu_set_vtimer_mask(false) failed: {status:#x}"
            ));
        }
    }

    if vcpu_created
        && pc_set
        && cpsr_set
        && vtimer_offset_set
        && cntv_cval_set
        && cntv_ctl_set
        && vtimer_unmasked
    {
        run_attempted = true;
        let observation = run_vcpu_once_with_watchdog(vcpu, exit);
        run_status = Some(observation.run_status);
        exit_reason = observation.exit_reason;
        exit_syndrome = observation.exit_syndrome;
        exit_virtual_address = observation.exit_virtual_address;
        exit_physical_address = observation.exit_physical_address;
        watchdog_cancel_status = observation.watchdog_cancel_status;
        watchdog_cancel_fired = watchdog_cancel_status.is_some();

        if observation.run_status == HV_SUCCESS {
            vtimer_exit_observed = observation.exit_reason == Some(HV_EXIT_REASON_VTIMER_ACTIVATED);
            if !vtimer_exit_observed {
                let reason_name = observation
                    .exit_reason
                    .map(hv_exit_reason_name)
                    .unwrap_or("not observed");
                blockers.push(format!(
                    "hv_vcpu_run did not return HV_EXIT_REASON_VTIMER_ACTIVATED; got {reason_name}"
                ));
            }
        } else {
            blockers.push(format!("hv_vcpu_run failed: {:#x}", observation.run_status));
        }

        if vtimer_exit_observed {
            let mut masked = false;
            let status = unsafe { hv_vcpu_get_vtimer_mask(vcpu, &mut masked) };
            vtimer_mask_get_after_exit_status = Some(status);
            if status == HV_SUCCESS {
                vtimer_mask_observed_after_exit = Some(masked);
                if !masked {
                    blockers.push(
                        "VTimer was not automatically masked after HV_EXIT_REASON_VTIMER_ACTIVATED"
                            .to_string(),
                    );
                }
            } else {
                blockers.push(format!(
                    "hv_vcpu_get_vtimer_mask after VTimer exit failed: {status:#x}"
                ));
            }

            let status =
                unsafe { hv_vcpu_set_pending_interrupt(vcpu, HV_INTERRUPT_TYPE_IRQ, true) };
            pending_irq_set_status = Some(status);
            pending_irq_injected = status == HV_SUCCESS;
            if !pending_irq_injected {
                blockers.push(format!(
                    "hv_vcpu_set_pending_interrupt IRQ=true after VTimer exit failed: {status:#x}"
                ));
            }

            let status = unsafe { hv_vcpu_set_vtimer_mask(vcpu, false) };
            vtimer_unmask_after_exit_status = Some(status);
            vtimer_unmasked_after_exit = status == HV_SUCCESS;
            if !vtimer_unmasked_after_exit {
                blockers.push(format!(
                    "hv_vcpu_set_vtimer_mask(false) after VTimer exit failed: {status:#x}"
                ));
            }
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

    HvfVtimerExitProbe {
        allowed: true,
        attempted: true,
        vm_created,
        memory_allocated,
        memory_mapped,
        vcpu_created,
        pc_set,
        cpsr_set,
        vtimer_offset_set,
        cntv_cval_set,
        cntv_ctl_set,
        vtimer_unmasked,
        run_attempted,
        vtimer_exit_observed,
        pending_irq_injected,
        vtimer_mask_observed_after_exit,
        vtimer_unmasked_after_exit,
        watchdog_cancel_fired,
        vcpu_destroyed,
        memory_unmapped,
        vm_destroyed,
        memory_deallocated,
        host,
        ipa_start: PROBE_IPA_START,
        bytes: PROBE_BYTES,
        instructions: "WFI; HVC #0",
        vtimer_offset_value,
        cntv_cval_value,
        cntv_ctl_value,
        vm_create_status,
        allocate_status,
        map_status,
        vcpu_create_status,
        pc_set_status,
        cpsr_set_status,
        vtimer_offset_set_status,
        cntv_cval_set_status,
        cntv_ctl_set_status,
        vtimer_unmask_status,
        run_status,
        exit_reason,
        exit_syndrome,
        exit_virtual_address,
        exit_physical_address,
        watchdog_cancel_status,
        pending_irq_set_status,
        vtimer_mask_get_after_exit_status,
        vtimer_unmask_after_exit_status,
        vcpu_destroy_status,
        unmap_status,
        vm_destroy_status,
        deallocate_status,
        blockers,
    }
}

pub(crate) fn vtimer_exit_probe_result(
    allowed: bool,
    attempted: bool,
    host: HvfHostCapabilities,
    blockers: Vec<String>,
) -> HvfVtimerExitProbe {
    HvfVtimerExitProbe {
        allowed,
        attempted,
        vm_created: false,
        memory_allocated: false,
        memory_mapped: false,
        vcpu_created: false,
        pc_set: false,
        cpsr_set: false,
        vtimer_offset_set: false,
        cntv_cval_set: false,
        cntv_ctl_set: false,
        vtimer_unmasked: false,
        run_attempted: false,
        vtimer_exit_observed: false,
        pending_irq_injected: false,
        vtimer_mask_observed_after_exit: None,
        vtimer_unmasked_after_exit: false,
        watchdog_cancel_fired: false,
        vcpu_destroyed: false,
        memory_unmapped: false,
        vm_destroyed: false,
        memory_deallocated: false,
        host,
        ipa_start: PROBE_IPA_START,
        bytes: PROBE_BYTES,
        instructions: "WFI; HVC #0",
        vtimer_offset_value: 0,
        cntv_cval_value: 0,
        cntv_ctl_value: 1,
        vm_create_status: None,
        allocate_status: None,
        map_status: None,
        vcpu_create_status: None,
        pc_set_status: None,
        cpsr_set_status: None,
        vtimer_offset_set_status: None,
        cntv_cval_set_status: None,
        cntv_ctl_set_status: None,
        vtimer_unmask_status: None,
        run_status: None,
        exit_reason: None,
        exit_syndrome: None,
        exit_virtual_address: None,
        exit_physical_address: None,
        watchdog_cancel_status: None,
        pending_irq_set_status: None,
        vtimer_mask_get_after_exit_status: None,
        vtimer_unmask_after_exit_status: None,
        vcpu_destroy_status: None,
        unmap_status: None,
        vm_destroy_status: None,
        deallocate_status: None,
        blockers,
    }
}
