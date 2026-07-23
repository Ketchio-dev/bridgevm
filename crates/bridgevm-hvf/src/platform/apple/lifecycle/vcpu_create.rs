//! Split out of lifecycle.rs by responsibility.

use super::super::*;
use crate::*;

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
