//! Split out of lifecycle.rs by responsibility.

use super::super::*;
use crate::*;

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
