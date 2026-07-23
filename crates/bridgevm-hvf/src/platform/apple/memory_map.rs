//! Guest memory allocate/map/unmap probe.
//!
//! Split out of the single 12,111-line apple.rs backend.

use super::*;
use crate::*;

pub fn probe_hvf_memory_map(allow_map: bool, host: HvfHostCapabilities) -> HvfMemoryMapProbe {
    let mut blockers = Vec::new();

    if !allow_map {
        blockers.push(
            "set BRIDGEVM_HVF_ALLOW_MEMORY_MAP=1 or pass --allow-map to create an empty HVF VM and map/unmap one guest RAM page".to_string(),
        );
        return HvfMemoryMapProbe {
            allowed: false,
            attempted: false,
            vm_created: false,
            memory_allocated: false,
            memory_mapped: false,
            memory_unmapped: false,
            memory_deallocated: false,
            vm_destroyed: false,
            host,
            ipa_start: PROBE_IPA_START,
            bytes: PROBE_BYTES,
            vm_create_status: None,
            allocate_status: None,
            map_status: None,
            unmap_status: None,
            deallocate_status: None,
            vm_destroy_status: None,
            blockers,
        };
    }

    if !host.available {
        blockers.push("Hypervisor.framework host capabilities are not available".to_string());
        return HvfMemoryMapProbe {
            allowed: true,
            attempted: false,
            vm_created: false,
            memory_allocated: false,
            memory_mapped: false,
            memory_unmapped: false,
            memory_deallocated: false,
            vm_destroyed: false,
            host,
            ipa_start: PROBE_IPA_START,
            bytes: PROBE_BYTES,
            vm_create_status: None,
            allocate_status: None,
            map_status: None,
            unmap_status: None,
            deallocate_status: None,
            vm_destroy_status: None,
            blockers,
        };
    }

    let vm_create_status = unsafe { hv_vm_create(ptr::null_mut()) };
    let vm_created = vm_create_status == HV_SUCCESS;
    if !vm_created {
        blockers.push(format!("hv_vm_create failed: {vm_create_status:#x}"));
        return HvfMemoryMapProbe {
            allowed: true,
            attempted: true,
            vm_created,
            memory_allocated: false,
            memory_mapped: false,
            memory_unmapped: false,
            memory_deallocated: false,
            vm_destroyed: false,
            host,
            ipa_start: PROBE_IPA_START,
            bytes: PROBE_BYTES,
            vm_create_status: Some(vm_create_status),
            allocate_status: None,
            map_status: None,
            unmap_status: None,
            deallocate_status: None,
            vm_destroy_status: None,
            blockers,
        };
    }

    let mut memory = ptr::null_mut();
    let allocate_status = unsafe { hv_vm_allocate(&mut memory, PROBE_BYTES, HV_ALLOCATE_DEFAULT) };
    let memory_allocated = allocate_status == HV_SUCCESS && !memory.is_null();
    if !memory_allocated {
        blockers.push(format!("hv_vm_allocate failed: {allocate_status:#x}"));
        let vm_destroy_status = unsafe { hv_vm_destroy() };
        let vm_destroyed = vm_destroy_status == HV_SUCCESS;
        if !vm_destroyed {
            blockers.push(format!("hv_vm_destroy failed: {vm_destroy_status:#x}"));
        }
        return HvfMemoryMapProbe {
            allowed: true,
            attempted: true,
            vm_created,
            memory_allocated,
            memory_mapped: false,
            memory_unmapped: false,
            memory_deallocated: false,
            vm_destroyed,
            host,
            ipa_start: PROBE_IPA_START,
            bytes: PROBE_BYTES,
            vm_create_status: Some(vm_create_status),
            allocate_status: Some(allocate_status),
            map_status: None,
            unmap_status: None,
            deallocate_status: None,
            vm_destroy_status: Some(vm_destroy_status),
            blockers,
        };
    }

    let map_status = unsafe {
        hv_vm_map(
            memory,
            PROBE_IPA_START,
            PROBE_BYTES,
            HV_MEMORY_READ | HV_MEMORY_WRITE | HV_MEMORY_EXEC,
        )
    };
    let memory_mapped = map_status == HV_SUCCESS;
    if !memory_mapped {
        blockers.push(format!("hv_vm_map failed: {map_status:#x}"));
    }

    let mut unmap_status = None;
    let mut memory_unmapped = false;
    if memory_mapped {
        let status = unsafe { hv_vm_unmap(PROBE_IPA_START, PROBE_BYTES) };
        memory_unmapped = status == HV_SUCCESS;
        unmap_status = Some(status);
        if !memory_unmapped {
            blockers.push(format!("hv_vm_unmap failed: {status:#x}"));
        }
    }

    let vm_destroy_status = unsafe { hv_vm_destroy() };
    let vm_destroyed = vm_destroy_status == HV_SUCCESS;
    if !vm_destroyed {
        blockers.push(format!("hv_vm_destroy failed: {vm_destroy_status:#x}"));
    }

    let deallocate_status = unsafe { hv_vm_deallocate(memory, PROBE_BYTES) };
    let memory_deallocated = deallocate_status == HV_SUCCESS;
    if !memory_deallocated {
        blockers.push(format!("hv_vm_deallocate failed: {deallocate_status:#x}"));
    }

    HvfMemoryMapProbe {
        allowed: true,
        attempted: true,
        vm_created,
        memory_allocated,
        memory_mapped,
        memory_unmapped,
        memory_deallocated,
        vm_destroyed,
        host,
        ipa_start: PROBE_IPA_START,
        bytes: PROBE_BYTES,
        vm_create_status: Some(vm_create_status),
        allocate_status: Some(allocate_status),
        map_status: Some(map_status),
        unmap_status,
        deallocate_status: Some(deallocate_status),
        vm_destroy_status: Some(vm_destroy_status),
        blockers,
    }
}
