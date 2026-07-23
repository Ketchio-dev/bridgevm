//! Split out of mmio.rs by responsibility.

use super::super::super::*;
use super::*;
use crate::*;

pub fn probe_hvf_memory_map(allow_map: bool, host: HvfHostCapabilities) -> HvfMemoryMapProbe {
    HvfMemoryMapProbe {
        allowed: allow_map,
        attempted: false,
        vm_created: false,
        memory_allocated: false,
        memory_mapped: false,
        memory_unmapped: false,
        memory_deallocated: false,
        vm_destroyed: false,
        host,
        ipa_start: 0x4000_0000,
        bytes: 16 * 1024,
        vm_create_status: None,
        allocate_status: None,
        map_status: None,
        unmap_status: None,
        deallocate_status: None,
        vm_destroy_status: None,
        blockers: vec![
            "Apple Hypervisor.framework memory map/unmap probe is only available on Apple Silicon macOS".to_string(),
        ],
    }
}

pub fn probe_windows_11_arm_uefi_pflash_hvf_map(
    allow_map: bool,
    pflash_map: WindowsArmUefiPflashMapProbe,
    host: HvfHostCapabilities,
) -> WindowsArmUefiPflashHvfMapProbe {
    let mut blockers = pflash_map.blockers.clone();
    if !allow_map {
        blockers.push(
            "set BRIDGEVM_HVF_ALLOW_UEFI_PFLASH_MAP=1 or pass --allow-map to create an empty HVF VM and map/unmap Windows UEFI code/vars pflash slots".to_string(),
        );
    }
    blockers.push(
        "Apple Hypervisor.framework Windows UEFI pflash map/unmap probe is only available on Apple Silicon macOS".to_string(),
    );
    WindowsArmUefiPflashHvfMapProbe {
        allowed: allow_map,
        attempted: false,
        vm_created: false,
        firmware_memory_allocated: false,
        vars_memory_allocated: false,
        firmware_memory_populated: false,
        vars_memory_populated: false,
        firmware_memory_mapped: false,
        vars_memory_mapped: false,
        firmware_memory_unmapped: false,
        vars_memory_unmapped: false,
        firmware_memory_deallocated: false,
        vars_memory_deallocated: false,
        vm_destroyed: false,
        host,
        pflash_map_verified: pflash_map.pflash_map_verified,
        firmware_slot_ipa: WINDOWS_ARM_UEFI_CODE_IPA,
        vars_slot_ipa: WINDOWS_ARM_UEFI_VARS_IPA,
        slot_bytes: WINDOWS_ARM_UEFI_SLOT_BYTES,
        firmware_source_bytes: pflash_map
            .firmware_slot
            .as_ref()
            .map(|slot| slot.source_bytes),
        vars_source_bytes: pflash_map.vars_slot.as_ref().map(|slot| slot.source_bytes),
        firmware_map_flags: "read|exec",
        vars_map_flags: "read|write",
        vm_create_status: None,
        firmware_allocate_status: None,
        vars_allocate_status: None,
        firmware_map_status: None,
        vars_map_status: None,
        firmware_unmap_status: None,
        vars_unmap_status: None,
        firmware_deallocate_status: None,
        vars_deallocate_status: None,
        vm_destroy_status: None,
        blockers,
    }
}

pub fn probe_windows_11_arm_uefi_reset_vector_entry(
    allow_entry: bool,
    pflash_map: WindowsArmUefiPflashMapProbe,
    host: HvfHostCapabilities,
) -> WindowsArmUefiResetVectorEntryProbe {
    let mut blockers = pflash_map.blockers.clone();
    if !allow_entry {
        blockers.push(
            "set BRIDGEVM_HVF_ALLOW_UEFI_RESET_VECTOR_ENTRY=1 or pass --allow-entry to map Windows UEFI pflash slots, create one vCPU, set PC to the reset vector, and run once under a watchdog".to_string(),
        );
    }
    blockers.push(
        "Apple Hypervisor.framework Windows UEFI reset-vector entry probe is only available on Apple Silicon macOS".to_string(),
    );
    WindowsArmUefiResetVectorEntryProbe {
        allowed: allow_entry,
        attempted: false,
        vm_created: false,
        firmware_memory_allocated: false,
        vars_memory_allocated: false,
        firmware_memory_populated: false,
        vars_memory_populated: false,
        firmware_memory_mapped: false,
        vars_memory_mapped: false,
        vcpu_created: false,
        pc_set: false,
        cpsr_set: false,
        run_attempted: false,
        reset_vector_entry_observed: false,
        firmware_progress_observed: false,
        watchdog_cancel_fired: false,
        vcpu_destroyed: false,
        firmware_memory_unmapped: false,
        vars_memory_unmapped: false,
        firmware_memory_deallocated: false,
        vars_memory_deallocated: false,
        vm_destroyed: false,
        host,
        pflash_map_verified: pflash_map.pflash_map_verified,
        reset_vector_ipa: WINDOWS_ARM_UEFI_CODE_IPA,
        firmware_slot_ipa: WINDOWS_ARM_UEFI_CODE_IPA,
        vars_slot_ipa: WINDOWS_ARM_UEFI_VARS_IPA,
        slot_bytes: WINDOWS_ARM_UEFI_SLOT_BYTES,
        firmware_source_bytes: pflash_map
            .firmware_slot
            .as_ref()
            .map(|slot| slot.source_bytes),
        vars_source_bytes: pflash_map.vars_slot.as_ref().map(|slot| slot.source_bytes),
        firmware_map_flags: "read|exec",
        vars_map_flags: "read|write",
        vm_create_status: None,
        firmware_allocate_status: None,
        vars_allocate_status: None,
        firmware_map_status: None,
        vars_map_status: None,
        vcpu_create_status: None,
        pc_set_status: None,
        cpsr_set_status: None,
        run_status: None,
        exit_reason: None,
        exit_syndrome: None,
        exit_exception_class: None,
        exit_virtual_address: None,
        exit_physical_address: None,
        pc_after_run_status: None,
        pc_after_run: None,
        watchdog_cancel_status: None,
        vcpu_destroy_status: None,
        firmware_unmap_status: None,
        vars_unmap_status: None,
        firmware_deallocate_status: None,
        vars_deallocate_status: None,
        vm_destroy_status: None,
        blockers,
    }
}
