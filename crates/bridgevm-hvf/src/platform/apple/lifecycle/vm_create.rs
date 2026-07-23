//! Split out of lifecycle.rs by responsibility.

use super::super::*;
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
