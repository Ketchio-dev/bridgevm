//! VM/vCPU lifecycle probes: create, run, interrupt timer and vtimer exit.
//!
//! Split out of the single 12,111-line apple.rs backend.

use super::*;
use crate::*;

pub fn probe_hvf_vm_create(allow_create: bool, host: HvfHostCapabilities) -> HvfVmCreateProbe {
    let mut blockers = Vec::new();

    if !allow_create {
        blockers.push(
            "set BRIDGEVM_HVF_ALLOW_VM_CREATE=1 or pass --allow-create to create and destroy an empty HVF VM".to_string(),
        );
        return HvfVmCreateProbe {
            allowed: false,
            attempted: false,
            created: false,
            destroyed: false,
            host,
            create_status: None,
            destroy_status: None,
            blockers,
        };
    }

    if !host.available {
        blockers.push("Hypervisor.framework host capabilities are not available".to_string());
        return HvfVmCreateProbe {
            allowed: true,
            attempted: false,
            created: false,
            destroyed: false,
            host,
            create_status: None,
            destroy_status: None,
            blockers,
        };
    }

    let create_status = unsafe { hv_vm_create(ptr::null_mut()) };
    let created = create_status == HV_SUCCESS;
    if !created {
        blockers.push(format!("hv_vm_create failed: {create_status:#x}"));
        return HvfVmCreateProbe {
            allowed: true,
            attempted: true,
            created,
            destroyed: false,
            host,
            create_status: Some(create_status),
            destroy_status: None,
            blockers,
        };
    }

    let destroy_status = unsafe { hv_vm_destroy() };
    let destroyed = destroy_status == HV_SUCCESS;
    if !destroyed {
        blockers.push(format!("hv_vm_destroy failed: {destroy_status:#x}"));
    }

    HvfVmCreateProbe {
        allowed: true,
        attempted: true,
        created,
        destroyed,
        host,
        create_status: Some(create_status),
        destroy_status: Some(destroy_status),
        blockers,
    }
}

pub fn probe_hvf_vcpu_create(allow_create: bool, host: HvfHostCapabilities) -> HvfVcpuCreateProbe {
    let mut blockers = Vec::new();

    if !allow_create {
        blockers.push(
            "set BRIDGEVM_HVF_ALLOW_VM_CREATE=1 or pass --allow-create to create and destroy an empty HVF VM and vCPU".to_string(),
        );
        return HvfVcpuCreateProbe {
            allowed: false,
            attempted: false,
            vm_created: false,
            vcpu_created: false,
            vcpu_destroyed: false,
            vm_destroyed: false,
            host,
            vm_create_status: None,
            vcpu_create_status: None,
            vcpu_destroy_status: None,
            vm_destroy_status: None,
            blockers,
        };
    }

    if !host.available {
        blockers.push("Hypervisor.framework host capabilities are not available".to_string());
        return HvfVcpuCreateProbe {
            allowed: true,
            attempted: false,
            vm_created: false,
            vcpu_created: false,
            vcpu_destroyed: false,
            vm_destroyed: false,
            host,
            vm_create_status: None,
            vcpu_create_status: None,
            vcpu_destroy_status: None,
            vm_destroy_status: None,
            blockers,
        };
    }

    let vm_create_status = unsafe { hv_vm_create(ptr::null_mut()) };
    let vm_created = vm_create_status == HV_SUCCESS;
    if !vm_created {
        blockers.push(format!("hv_vm_create failed: {vm_create_status:#x}"));
        return HvfVcpuCreateProbe {
            allowed: true,
            attempted: true,
            vm_created,
            vcpu_created: false,
            vcpu_destroyed: false,
            vm_destroyed: false,
            host,
            vm_create_status: Some(vm_create_status),
            vcpu_create_status: None,
            vcpu_destroy_status: None,
            vm_destroy_status: None,
            blockers,
        };
    }

    let mut vcpu = 0;
    let mut exit = ptr::null_mut();
    let vcpu_create_status = unsafe { hv_vcpu_create(&mut vcpu, &mut exit, ptr::null_mut()) };
    let vcpu_created = vcpu_create_status == HV_SUCCESS;
    if !vcpu_created {
        blockers.push(format!("hv_vcpu_create failed: {vcpu_create_status:#x}"));
        let vm_destroy_status = unsafe { hv_vm_destroy() };
        let vm_destroyed = vm_destroy_status == HV_SUCCESS;
        if !vm_destroyed {
            blockers.push(format!("hv_vm_destroy failed: {vm_destroy_status:#x}"));
        }
        return HvfVcpuCreateProbe {
            allowed: true,
            attempted: true,
            vm_created,
            vcpu_created,
            vcpu_destroyed: false,
            vm_destroyed,
            host,
            vm_create_status: Some(vm_create_status),
            vcpu_create_status: Some(vcpu_create_status),
            vcpu_destroy_status: None,
            vm_destroy_status: Some(vm_destroy_status),
            blockers,
        };
    }

    let vcpu_destroy_status = unsafe { hv_vcpu_destroy(vcpu) };
    let vcpu_destroyed = vcpu_destroy_status == HV_SUCCESS;
    if !vcpu_destroyed {
        blockers.push(format!("hv_vcpu_destroy failed: {vcpu_destroy_status:#x}"));
    }

    let vm_destroy_status = unsafe { hv_vm_destroy() };
    let vm_destroyed = vm_destroy_status == HV_SUCCESS;
    if !vm_destroyed {
        blockers.push(format!("hv_vm_destroy failed: {vm_destroy_status:#x}"));
    }

    HvfVcpuCreateProbe {
        allowed: true,
        attempted: true,
        vm_created,
        vcpu_created,
        vcpu_destroyed,
        vm_destroyed,
        host,
        vm_create_status: Some(vm_create_status),
        vcpu_create_status: Some(vcpu_create_status),
        vcpu_destroy_status: Some(vcpu_destroy_status),
        vm_destroy_status: Some(vm_destroy_status),
        blockers,
    }
}

pub fn probe_hvf_vcpu_run(allow_run: bool, host: HvfHostCapabilities) -> HvfVcpuRunProbe {
    let mut blockers = Vec::new();

    if !allow_run {
        blockers.push(
            "set BRIDGEVM_HVF_ALLOW_VCPU_RUN=1 or pass --allow-run to pre-cancel and observe one hv_vcpu_run boundary".to_string(),
        );
        return HvfVcpuRunProbe {
            allowed: false,
            attempted: false,
            vm_created: false,
            vcpu_created: false,
            cancel_requested: false,
            run_attempted: false,
            run_boundary_observed: false,
            vcpu_destroyed: false,
            vm_destroyed: false,
            host,
            vm_create_status: None,
            vcpu_create_status: None,
            cancel_status: None,
            run_status: None,
            exit_reason: None,
            vcpu_destroy_status: None,
            vm_destroy_status: None,
            blockers,
        };
    }

    if !host.available {
        blockers.push("Hypervisor.framework host capabilities are not available".to_string());
        return HvfVcpuRunProbe {
            allowed: true,
            attempted: false,
            vm_created: false,
            vcpu_created: false,
            cancel_requested: false,
            run_attempted: false,
            run_boundary_observed: false,
            vcpu_destroyed: false,
            vm_destroyed: false,
            host,
            vm_create_status: None,
            vcpu_create_status: None,
            cancel_status: None,
            run_status: None,
            exit_reason: None,
            vcpu_destroy_status: None,
            vm_destroy_status: None,
            blockers,
        };
    }

    let vm_create_status = unsafe { hv_vm_create(ptr::null_mut()) };
    let vm_created = vm_create_status == HV_SUCCESS;
    if !vm_created {
        blockers.push(format!("hv_vm_create failed: {vm_create_status:#x}"));
        return HvfVcpuRunProbe {
            allowed: true,
            attempted: true,
            vm_created,
            vcpu_created: false,
            cancel_requested: false,
            run_attempted: false,
            run_boundary_observed: false,
            vcpu_destroyed: false,
            vm_destroyed: false,
            host,
            vm_create_status: Some(vm_create_status),
            vcpu_create_status: None,
            cancel_status: None,
            run_status: None,
            exit_reason: None,
            vcpu_destroy_status: None,
            vm_destroy_status: None,
            blockers,
        };
    }

    let mut vcpu = 0;
    let mut exit = ptr::null_mut();
    let vcpu_create_status = unsafe { hv_vcpu_create(&mut vcpu, &mut exit, ptr::null_mut()) };
    let vcpu_created = vcpu_create_status == HV_SUCCESS;
    if !vcpu_created {
        blockers.push(format!("hv_vcpu_create failed: {vcpu_create_status:#x}"));
        let vm_destroy_status = unsafe { hv_vm_destroy() };
        let vm_destroyed = vm_destroy_status == HV_SUCCESS;
        if !vm_destroyed {
            blockers.push(format!("hv_vm_destroy failed: {vm_destroy_status:#x}"));
        }
        return HvfVcpuRunProbe {
            allowed: true,
            attempted: true,
            vm_created,
            vcpu_created,
            cancel_requested: false,
            run_attempted: false,
            run_boundary_observed: false,
            vcpu_destroyed: false,
            vm_destroyed,
            host,
            vm_create_status: Some(vm_create_status),
            vcpu_create_status: Some(vcpu_create_status),
            cancel_status: None,
            run_status: None,
            exit_reason: None,
            vcpu_destroy_status: None,
            vm_destroy_status: Some(vm_destroy_status),
            blockers,
        };
    }

    let cancel_status = unsafe { hv_vcpus_exit(&mut vcpu, 1) };
    let cancel_requested = cancel_status == HV_SUCCESS;
    if !cancel_requested {
        blockers.push(format!("hv_vcpus_exit failed: {cancel_status:#x}"));
    }

    let mut run_attempted = false;
    let mut run_status = None;
    let mut exit_reason = None;
    if cancel_requested {
        run_attempted = true;
        let status = unsafe { hv_vcpu_run(vcpu) };
        run_status = Some(status);
        if status == HV_SUCCESS {
            if exit.is_null() {
                blockers
                    .push("hv_vcpu_run returned success without an exit info pointer".to_string());
            } else {
                exit_reason = Some(unsafe { (*exit).reason });
                if exit_reason != Some(HV_EXIT_REASON_CANCELED) {
                    blockers.push(format!(
                        "hv_vcpu_run returned unexpected exit reason: {}",
                        exit_reason.unwrap_or_default()
                    ));
                }
            }
        } else {
            blockers.push(format!("hv_vcpu_run failed: {status:#x}"));
        }
    }

    let run_boundary_observed =
        run_status == Some(HV_SUCCESS) && exit_reason == Some(HV_EXIT_REASON_CANCELED);

    let vcpu_destroy_status = unsafe { hv_vcpu_destroy(vcpu) };
    let vcpu_destroyed = vcpu_destroy_status == HV_SUCCESS;
    if !vcpu_destroyed {
        blockers.push(format!("hv_vcpu_destroy failed: {vcpu_destroy_status:#x}"));
    }

    let vm_destroy_status = unsafe { hv_vm_destroy() };
    let vm_destroyed = vm_destroy_status == HV_SUCCESS;
    if !vm_destroyed {
        blockers.push(format!("hv_vm_destroy failed: {vm_destroy_status:#x}"));
    }

    HvfVcpuRunProbe {
        allowed: true,
        attempted: true,
        vm_created,
        vcpu_created,
        cancel_requested,
        run_attempted,
        run_boundary_observed,
        vcpu_destroyed,
        vm_destroyed,
        host,
        vm_create_status: Some(vm_create_status),
        vcpu_create_status: Some(vcpu_create_status),
        cancel_status: Some(cancel_status),
        run_status,
        exit_reason,
        vcpu_destroy_status: Some(vcpu_destroy_status),
        vm_destroy_status: Some(vm_destroy_status),
        blockers,
    }
}

pub fn probe_hvf_interrupt_timer(
    allow_probe: bool,
    host: HvfHostCapabilities,
) -> HvfInterruptTimerProbe {
    let mut blockers = Vec::new();
    let vtimer_offset_value = 0x1000;

    if !allow_probe {
        blockers.push(
            "set BRIDGEVM_HVF_ALLOW_INTERRUPT_TIMER=1 or pass --allow-interrupt-timer to create an empty HVF VM/vCPU and verify pending IRQ plus virtual timer controls".to_string(),
        );
        return HvfInterruptTimerProbe {
            allowed: false,
            attempted: false,
            vm_created: false,
            vcpu_created: false,
            pending_irq_set: false,
            pending_irq_cleared: false,
            vtimer_masked: false,
            vtimer_unmasked: false,
            vtimer_offset_set: false,
            boundary_observed: false,
            vcpu_destroyed: false,
            vm_destroyed: false,
            host,
            vtimer_offset_value,
            vm_create_status: None,
            vcpu_create_status: None,
            irq_set_status: None,
            irq_get_after_set_status: None,
            irq_pending_after_set: None,
            irq_clear_status: None,
            irq_get_after_clear_status: None,
            irq_pending_after_clear: None,
            vtimer_mask_set_status: None,
            vtimer_mask_get_status: None,
            vtimer_mask_after_set: None,
            vtimer_unmask_status: None,
            vtimer_unmask_get_status: None,
            vtimer_mask_after_clear: None,
            vtimer_offset_set_status: None,
            vtimer_offset_get_status: None,
            vtimer_offset_after_set: None,
            vcpu_destroy_status: None,
            vm_destroy_status: None,
            blockers,
        };
    }

    if !host.available {
        blockers.push("Hypervisor.framework host capabilities are not available".to_string());
        return HvfInterruptTimerProbe {
            allowed: true,
            attempted: false,
            vm_created: false,
            vcpu_created: false,
            pending_irq_set: false,
            pending_irq_cleared: false,
            vtimer_masked: false,
            vtimer_unmasked: false,
            vtimer_offset_set: false,
            boundary_observed: false,
            vcpu_destroyed: false,
            vm_destroyed: false,
            host,
            vtimer_offset_value,
            vm_create_status: None,
            vcpu_create_status: None,
            irq_set_status: None,
            irq_get_after_set_status: None,
            irq_pending_after_set: None,
            irq_clear_status: None,
            irq_get_after_clear_status: None,
            irq_pending_after_clear: None,
            vtimer_mask_set_status: None,
            vtimer_mask_get_status: None,
            vtimer_mask_after_set: None,
            vtimer_unmask_status: None,
            vtimer_unmask_get_status: None,
            vtimer_mask_after_clear: None,
            vtimer_offset_set_status: None,
            vtimer_offset_get_status: None,
            vtimer_offset_after_set: None,
            vcpu_destroy_status: None,
            vm_destroy_status: None,
            blockers,
        };
    }

    let vm_create_status = unsafe { hv_vm_create(ptr::null_mut()) };
    let vm_created = vm_create_status == HV_SUCCESS;
    if !vm_created {
        blockers.push(format!("hv_vm_create failed: {vm_create_status:#x}"));
        return HvfInterruptTimerProbe {
            allowed: true,
            attempted: true,
            vm_created,
            vcpu_created: false,
            pending_irq_set: false,
            pending_irq_cleared: false,
            vtimer_masked: false,
            vtimer_unmasked: false,
            vtimer_offset_set: false,
            boundary_observed: false,
            vcpu_destroyed: false,
            vm_destroyed: false,
            host,
            vtimer_offset_value,
            vm_create_status: Some(vm_create_status),
            vcpu_create_status: None,
            irq_set_status: None,
            irq_get_after_set_status: None,
            irq_pending_after_set: None,
            irq_clear_status: None,
            irq_get_after_clear_status: None,
            irq_pending_after_clear: None,
            vtimer_mask_set_status: None,
            vtimer_mask_get_status: None,
            vtimer_mask_after_set: None,
            vtimer_unmask_status: None,
            vtimer_unmask_get_status: None,
            vtimer_mask_after_clear: None,
            vtimer_offset_set_status: None,
            vtimer_offset_get_status: None,
            vtimer_offset_after_set: None,
            vcpu_destroy_status: None,
            vm_destroy_status: None,
            blockers,
        };
    }

    let mut vcpu = 0;
    let mut exit = ptr::null_mut();
    let vcpu_create_status = unsafe { hv_vcpu_create(&mut vcpu, &mut exit, ptr::null_mut()) };
    let vcpu_created = vcpu_create_status == HV_SUCCESS;
    if !vcpu_created {
        blockers.push(format!("hv_vcpu_create failed: {vcpu_create_status:#x}"));
        let vm_destroy_status = unsafe { hv_vm_destroy() };
        let vm_destroyed = vm_destroy_status == HV_SUCCESS;
        if !vm_destroyed {
            blockers.push(format!("hv_vm_destroy failed: {vm_destroy_status:#x}"));
        }
        return HvfInterruptTimerProbe {
            allowed: true,
            attempted: true,
            vm_created,
            vcpu_created,
            pending_irq_set: false,
            pending_irq_cleared: false,
            vtimer_masked: false,
            vtimer_unmasked: false,
            vtimer_offset_set: false,
            boundary_observed: false,
            vcpu_destroyed: false,
            vm_destroyed,
            host,
            vtimer_offset_value,
            vm_create_status: Some(vm_create_status),
            vcpu_create_status: Some(vcpu_create_status),
            irq_set_status: None,
            irq_get_after_set_status: None,
            irq_pending_after_set: None,
            irq_clear_status: None,
            irq_get_after_clear_status: None,
            irq_pending_after_clear: None,
            vtimer_mask_set_status: None,
            vtimer_mask_get_status: None,
            vtimer_mask_after_set: None,
            vtimer_unmask_status: None,
            vtimer_unmask_get_status: None,
            vtimer_mask_after_clear: None,
            vtimer_offset_set_status: None,
            vtimer_offset_get_status: None,
            vtimer_offset_after_set: None,
            vcpu_destroy_status: None,
            vm_destroy_status: Some(vm_destroy_status),
            blockers,
        };
    }

    let irq_set_status =
        unsafe { hv_vcpu_set_pending_interrupt(vcpu, HV_INTERRUPT_TYPE_IRQ, true) };
    let pending_irq_set = irq_set_status == HV_SUCCESS;
    if !pending_irq_set {
        blockers.push(format!(
            "hv_vcpu_set_pending_interrupt IRQ=true failed: {irq_set_status:#x}"
        ));
    }
    let mut irq_pending_after_set_value = false;
    let irq_get_after_set_status = unsafe {
        hv_vcpu_get_pending_interrupt(
            vcpu,
            HV_INTERRUPT_TYPE_IRQ,
            &mut irq_pending_after_set_value,
        )
    };
    let irq_pending_after_set =
        (irq_get_after_set_status == HV_SUCCESS).then_some(irq_pending_after_set_value);
    if irq_get_after_set_status != HV_SUCCESS {
        blockers.push(format!(
            "hv_vcpu_get_pending_interrupt after IRQ set failed: {irq_get_after_set_status:#x}"
        ));
    } else if irq_pending_after_set != Some(true) {
        blockers.push("pending IRQ was not true after set".to_string());
    }

    let irq_clear_status =
        unsafe { hv_vcpu_set_pending_interrupt(vcpu, HV_INTERRUPT_TYPE_IRQ, false) };
    let pending_irq_cleared = irq_clear_status == HV_SUCCESS;
    if !pending_irq_cleared {
        blockers.push(format!(
            "hv_vcpu_set_pending_interrupt IRQ=false failed: {irq_clear_status:#x}"
        ));
    }
    let mut irq_pending_after_clear_value = true;
    let irq_get_after_clear_status = unsafe {
        hv_vcpu_get_pending_interrupt(
            vcpu,
            HV_INTERRUPT_TYPE_IRQ,
            &mut irq_pending_after_clear_value,
        )
    };
    let irq_pending_after_clear =
        (irq_get_after_clear_status == HV_SUCCESS).then_some(irq_pending_after_clear_value);
    if irq_get_after_clear_status != HV_SUCCESS {
        blockers.push(format!(
            "hv_vcpu_get_pending_interrupt after IRQ clear failed: {irq_get_after_clear_status:#x}"
        ));
    } else if irq_pending_after_clear != Some(false) {
        blockers.push("pending IRQ was not false after clear".to_string());
    }

    let vtimer_mask_set_status = unsafe { hv_vcpu_set_vtimer_mask(vcpu, true) };
    let vtimer_masked = vtimer_mask_set_status == HV_SUCCESS;
    if !vtimer_masked {
        blockers.push(format!(
            "hv_vcpu_set_vtimer_mask true failed: {vtimer_mask_set_status:#x}"
        ));
    }
    let mut vtimer_mask_after_set_value = false;
    let vtimer_mask_get_status =
        unsafe { hv_vcpu_get_vtimer_mask(vcpu, &mut vtimer_mask_after_set_value) };
    let vtimer_mask_after_set =
        (vtimer_mask_get_status == HV_SUCCESS).then_some(vtimer_mask_after_set_value);
    if vtimer_mask_get_status != HV_SUCCESS {
        blockers.push(format!(
            "hv_vcpu_get_vtimer_mask after set failed: {vtimer_mask_get_status:#x}"
        ));
    } else if vtimer_mask_after_set != Some(true) {
        blockers.push("VTimer mask was not true after set".to_string());
    }

    let vtimer_unmask_status = unsafe { hv_vcpu_set_vtimer_mask(vcpu, false) };
    let vtimer_unmasked = vtimer_unmask_status == HV_SUCCESS;
    if !vtimer_unmasked {
        blockers.push(format!(
            "hv_vcpu_set_vtimer_mask false failed: {vtimer_unmask_status:#x}"
        ));
    }
    let mut vtimer_mask_after_clear_value = true;
    let vtimer_unmask_get_status =
        unsafe { hv_vcpu_get_vtimer_mask(vcpu, &mut vtimer_mask_after_clear_value) };
    let vtimer_mask_after_clear =
        (vtimer_unmask_get_status == HV_SUCCESS).then_some(vtimer_mask_after_clear_value);
    if vtimer_unmask_get_status != HV_SUCCESS {
        blockers.push(format!(
            "hv_vcpu_get_vtimer_mask after clear failed: {vtimer_unmask_get_status:#x}"
        ));
    } else if vtimer_mask_after_clear != Some(false) {
        blockers.push("VTimer mask was not false after clear".to_string());
    }

    let vtimer_offset_set_status = unsafe { hv_vcpu_set_vtimer_offset(vcpu, vtimer_offset_value) };
    let vtimer_offset_set = vtimer_offset_set_status == HV_SUCCESS;
    if !vtimer_offset_set {
        blockers.push(format!(
            "hv_vcpu_set_vtimer_offset failed: {vtimer_offset_set_status:#x}"
        ));
    }
    let mut vtimer_offset_after_set_value = 0;
    let vtimer_offset_get_status =
        unsafe { hv_vcpu_get_vtimer_offset(vcpu, &mut vtimer_offset_after_set_value) };
    let vtimer_offset_after_set =
        (vtimer_offset_get_status == HV_SUCCESS).then_some(vtimer_offset_after_set_value);
    if vtimer_offset_get_status != HV_SUCCESS {
        blockers.push(format!(
            "hv_vcpu_get_vtimer_offset failed: {vtimer_offset_get_status:#x}"
        ));
    } else if vtimer_offset_after_set != Some(vtimer_offset_value) {
        blockers.push(format!(
            "VTimer offset was not preserved after set: expected {vtimer_offset_value:#x}, got {}",
            vtimer_offset_after_set
                .map_or_else(|| "not observed".to_string(), |value| format!("{value:#x}"))
        ));
    }

    let boundary_observed = pending_irq_set
        && irq_pending_after_set == Some(true)
        && pending_irq_cleared
        && irq_pending_after_clear == Some(false)
        && vtimer_masked
        && vtimer_mask_after_set == Some(true)
        && vtimer_unmasked
        && vtimer_mask_after_clear == Some(false)
        && vtimer_offset_set
        && vtimer_offset_after_set == Some(vtimer_offset_value);

    let vcpu_destroy_status = unsafe { hv_vcpu_destroy(vcpu) };
    let vcpu_destroyed = vcpu_destroy_status == HV_SUCCESS;
    if !vcpu_destroyed {
        blockers.push(format!("hv_vcpu_destroy failed: {vcpu_destroy_status:#x}"));
    }

    let vm_destroy_status = unsafe { hv_vm_destroy() };
    let vm_destroyed = vm_destroy_status == HV_SUCCESS;
    if !vm_destroyed {
        blockers.push(format!("hv_vm_destroy failed: {vm_destroy_status:#x}"));
    }

    HvfInterruptTimerProbe {
        allowed: true,
        attempted: true,
        vm_created,
        vcpu_created,
        pending_irq_set,
        pending_irq_cleared,
        vtimer_masked,
        vtimer_unmasked,
        vtimer_offset_set,
        boundary_observed,
        vcpu_destroyed,
        vm_destroyed,
        host,
        vtimer_offset_value,
        vm_create_status: Some(vm_create_status),
        vcpu_create_status: Some(vcpu_create_status),
        irq_set_status: Some(irq_set_status),
        irq_get_after_set_status: Some(irq_get_after_set_status),
        irq_pending_after_set,
        irq_clear_status: Some(irq_clear_status),
        irq_get_after_clear_status: Some(irq_get_after_clear_status),
        irq_pending_after_clear,
        vtimer_mask_set_status: Some(vtimer_mask_set_status),
        vtimer_mask_get_status: Some(vtimer_mask_get_status),
        vtimer_mask_after_set,
        vtimer_unmask_status: Some(vtimer_unmask_status),
        vtimer_unmask_get_status: Some(vtimer_unmask_get_status),
        vtimer_mask_after_clear,
        vtimer_offset_set_status: Some(vtimer_offset_set_status),
        vtimer_offset_get_status: Some(vtimer_offset_get_status),
        vtimer_offset_after_set,
        vcpu_destroy_status: Some(vcpu_destroy_status),
        vm_destroy_status: Some(vm_destroy_status),
        blockers,
    }
}

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
