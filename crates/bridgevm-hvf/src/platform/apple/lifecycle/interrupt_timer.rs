//! Split out of lifecycle.rs by responsibility.

use super::super::*;
use crate::*;

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
