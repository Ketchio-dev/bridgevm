//! UEFI pflash mapping into HVF, diagnostic vector-slot install/restore, and DTB placement.
//!
//! Split out of the single 12,111-line apple.rs backend.

use super::*;
use crate::*;

pub fn probe_windows_11_arm_uefi_pflash_hvf_map(
    allow_map: bool,
    pflash_map: WindowsArmUefiPflashMapProbe,
    host: HvfHostCapabilities,
) -> WindowsArmUefiPflashHvfMapProbe {
    let mut blockers = pflash_map.blockers.clone();
    let firmware_source_bytes = pflash_map
        .firmware_slot
        .as_ref()
        .map(|slot| slot.source_bytes);
    let vars_source_bytes = pflash_map.vars_slot.as_ref().map(|slot| slot.source_bytes);

    if !allow_map {
        blockers.push(
            "set BRIDGEVM_HVF_ALLOW_UEFI_PFLASH_MAP=1 or pass --allow-map to create an empty HVF VM and map/unmap Windows UEFI code/vars pflash slots".to_string(),
        );
        return pflash_hvf_map_result(
            false,
            false,
            host,
            pflash_map.pflash_map_verified,
            firmware_source_bytes,
            vars_source_bytes,
            PflashHvfMapOutcome {
                blockers,
                ..PflashHvfMapOutcome::default()
            },
        );
    }

    if !pflash_map.pflash_map_verified {
        blockers.push(
            "pflash memory-image mapper did not verify code/vars slots; refusing live HVF map"
                .to_string(),
        );
        return pflash_hvf_map_result(
            true,
            false,
            host,
            false,
            firmware_source_bytes,
            vars_source_bytes,
            PflashHvfMapOutcome {
                blockers,
                ..PflashHvfMapOutcome::default()
            },
        );
    }

    if !host.available {
        blockers.push("Hypervisor.framework host capabilities are not available".to_string());
        return pflash_hvf_map_result(
            true,
            false,
            host,
            true,
            firmware_source_bytes,
            vars_source_bytes,
            PflashHvfMapOutcome {
                blockers,
                ..PflashHvfMapOutcome::default()
            },
        );
    }

    let slot_bytes_usize: usize = WINDOWS_ARM_UEFI_SLOT_BYTES
        .try_into()
        .expect("Windows UEFI pflash slot fits in usize");
    let mut firmware_memory = ptr::null_mut();
    let mut vars_memory = ptr::null_mut();
    let mut firmware_memory_populated = false;
    let mut vars_memory_populated = false;
    let mut firmware_memory_mapped = false;
    let mut vars_memory_mapped = false;
    let mut firmware_memory_unmapped = false;
    let mut vars_memory_unmapped = false;
    let mut firmware_memory_deallocated = false;
    let mut vars_memory_deallocated = false;
    let mut firmware_map_status = None;
    let mut vars_map_status = None;
    let mut firmware_unmap_status = None;
    let mut vars_unmap_status = None;
    let mut firmware_deallocate_status = None;
    let mut vars_deallocate_status = None;

    let vm_create_status = unsafe { hv_vm_create(ptr::null_mut()) };
    let vm_created = vm_create_status == HV_SUCCESS;
    if !vm_created {
        blockers.push(format!("hv_vm_create failed: {vm_create_status:#x}"));
        return pflash_hvf_map_result(
            true,
            true,
            host,
            true,
            firmware_source_bytes,
            vars_source_bytes,
            PflashHvfMapOutcome {
                vm_create_status: Some(vm_create_status),
                blockers,
                ..PflashHvfMapOutcome::default()
            },
        );
    }

    let firmware_status =
        unsafe { hv_vm_allocate(&mut firmware_memory, slot_bytes_usize, HV_ALLOCATE_DEFAULT) };
    let firmware_allocate_status = Some(firmware_status);
    let firmware_memory_allocated = firmware_status == HV_SUCCESS && !firmware_memory.is_null();
    if !firmware_memory_allocated {
        blockers.push(format!(
            "hv_vm_allocate firmware pflash failed: {firmware_status:#x}"
        ));
    }

    let vars_status =
        unsafe { hv_vm_allocate(&mut vars_memory, slot_bytes_usize, HV_ALLOCATE_DEFAULT) };
    let vars_allocate_status = Some(vars_status);
    let vars_memory_allocated = vars_status == HV_SUCCESS && !vars_memory.is_null();
    if !vars_memory_allocated {
        blockers.push(format!(
            "hv_vm_allocate vars pflash failed: {vars_status:#x}"
        ));
    }

    if firmware_memory_allocated {
        firmware_memory_populated = populate_pflash_hvf_memory(
            firmware_memory,
            pflash_map.firmware_slot.as_ref(),
            "firmware",
            &mut blockers,
        );
    }
    if vars_memory_allocated {
        vars_memory_populated = populate_pflash_hvf_memory(
            vars_memory,
            pflash_map.vars_slot.as_ref(),
            "vars",
            &mut blockers,
        );
    }

    if firmware_memory_populated {
        let status = unsafe {
            hv_vm_map(
                firmware_memory,
                WINDOWS_ARM_UEFI_CODE_IPA,
                slot_bytes_usize,
                HV_MEMORY_READ | HV_MEMORY_EXEC,
            )
        };
        firmware_map_status = Some(status);
        firmware_memory_mapped = status == HV_SUCCESS;
        if !firmware_memory_mapped {
            blockers.push(format!("hv_vm_map firmware pflash failed: {status:#x}"));
        }
    }

    if vars_memory_populated {
        let status = unsafe {
            hv_vm_map(
                vars_memory,
                WINDOWS_ARM_UEFI_VARS_IPA,
                slot_bytes_usize,
                HV_MEMORY_READ | HV_MEMORY_WRITE,
            )
        };
        vars_map_status = Some(status);
        vars_memory_mapped = status == HV_SUCCESS;
        if !vars_memory_mapped {
            blockers.push(format!("hv_vm_map vars pflash failed: {status:#x}"));
        }
    }

    if vars_memory_mapped {
        let status = unsafe { hv_vm_unmap(WINDOWS_ARM_UEFI_VARS_IPA, slot_bytes_usize) };
        vars_unmap_status = Some(status);
        vars_memory_unmapped = status == HV_SUCCESS;
        if !vars_memory_unmapped {
            blockers.push(format!("hv_vm_unmap vars pflash failed: {status:#x}"));
        }
    }

    if firmware_memory_mapped {
        let status = unsafe { hv_vm_unmap(WINDOWS_ARM_UEFI_CODE_IPA, slot_bytes_usize) };
        firmware_unmap_status = Some(status);
        firmware_memory_unmapped = status == HV_SUCCESS;
        if !firmware_memory_unmapped {
            blockers.push(format!("hv_vm_unmap firmware pflash failed: {status:#x}"));
        }
    }

    let vm_destroy_status = unsafe { hv_vm_destroy() };
    let vm_destroyed = vm_destroy_status == HV_SUCCESS;
    if !vm_destroyed {
        blockers.push(format!("hv_vm_destroy failed: {vm_destroy_status:#x}"));
    }

    if firmware_memory_allocated {
        let status = unsafe { hv_vm_deallocate(firmware_memory, slot_bytes_usize) };
        firmware_deallocate_status = Some(status);
        firmware_memory_deallocated = status == HV_SUCCESS;
        if !firmware_memory_deallocated {
            blockers.push(format!(
                "hv_vm_deallocate firmware pflash failed: {status:#x}"
            ));
        }
    }
    if vars_memory_allocated {
        let status = unsafe { hv_vm_deallocate(vars_memory, slot_bytes_usize) };
        vars_deallocate_status = Some(status);
        vars_memory_deallocated = status == HV_SUCCESS;
        if !vars_memory_deallocated {
            blockers.push(format!("hv_vm_deallocate vars pflash failed: {status:#x}"));
        }
    }

    WindowsArmUefiPflashHvfMapProbe {
        allowed: true,
        attempted: true,
        vm_created,
        firmware_memory_allocated,
        vars_memory_allocated,
        firmware_memory_populated,
        vars_memory_populated,
        firmware_memory_mapped,
        vars_memory_mapped,
        firmware_memory_unmapped,
        vars_memory_unmapped,
        firmware_memory_deallocated,
        vars_memory_deallocated,
        vm_destroyed,
        host,
        pflash_map_verified: pflash_map.pflash_map_verified,
        firmware_slot_ipa: WINDOWS_ARM_UEFI_CODE_IPA,
        vars_slot_ipa: WINDOWS_ARM_UEFI_VARS_IPA,
        slot_bytes: WINDOWS_ARM_UEFI_SLOT_BYTES,
        firmware_source_bytes,
        vars_source_bytes,
        firmware_map_flags: "read|exec",
        vars_map_flags: "read|write",
        vm_create_status: Some(vm_create_status),
        firmware_allocate_status,
        vars_allocate_status,
        firmware_map_status,
        vars_map_status,
        firmware_unmap_status,
        vars_unmap_status,
        firmware_deallocate_status,
        vars_deallocate_status,
        vm_destroy_status: Some(vm_destroy_status),
        blockers,
    }
}

pub(crate) fn populate_pflash_hvf_memory(
    memory: *mut c_void,
    slot: Option<&WindowsArmUefiPflashSlotMap>,
    label: &str,
    blockers: &mut Vec<String>,
) -> bool {
    let Some(slot) = slot else {
        blockers.push(format!("{label} pflash slot was not prepared"));
        return false;
    };
    let source_limit = match usize::try_from(slot.source_bytes) {
        Ok(source_limit) => source_limit,
        Err(_) => {
            blockers.push(format!(
                "{label} pflash source length exceeds host address space"
            ));
            return false;
        }
    };
    let source = match crate::media::read_bounded_file(&slot.path, source_limit) {
        Ok(source) => source,
        Err(error) => {
            blockers.push(format!("{label} pflash source read failed: {error}"));
            return false;
        }
    };
    if source.len() as u64 != slot.source_bytes {
        blockers.push(format!(
            "{label} pflash source length changed during HVF map probe"
        ));
        return false;
    }
    let slot_len: usize = slot
        .slot_bytes
        .try_into()
        .expect("Windows UEFI pflash slot fits in usize");
    unsafe {
        ptr::write_bytes(memory.cast::<u8>(), 0, slot_len);
        ptr::copy_nonoverlapping(source.as_ptr(), memory.cast::<u8>(), source.len());
        let mapped = std::slice::from_raw_parts(memory.cast::<u8>(), slot_len);
        mapped[..source.len()] == source[..] && mapped[source.len()..].iter().all(|byte| *byte == 0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DiagnosticExceptionVectorSlotSnapshot {
    pub(crate) start: usize,
    pub(crate) original: [u8; DIAGNOSTIC_EXCEPTION_VECTOR_SLOT_BYTES],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DiagnosticExceptionVectorSlotRange {
    pub(crate) vector_start: usize,
    pub(crate) eret_start: usize,
    pub(crate) landing_start: usize,
    pub(crate) vector_end: usize,
}

pub(crate) fn diagnostic_exception_vector_slot_range(
    bytes: usize,
    base_offset: usize,
    location: &str,
    blockers: &mut Vec<String>,
) -> Option<DiagnosticExceptionVectorSlotRange> {
    let vector_offset = WINDOWS_ARM_DIAGNOSTIC_VECTOR_CURRENT_EL_SPX_SYNC_OFFSET;
    let Some(vector_start) = base_offset.checked_add(vector_offset) else {
        blockers.push(format!(
            "diagnostic exception vector {location} offset overflowed"
        ));
        return None;
    };
    let Some(eret_start) = vector_start.checked_add(4) else {
        blockers.push(format!(
            "diagnostic exception vector {location} ERET offset overflowed"
        ));
        return None;
    };
    let Some(landing_start) = eret_start.checked_add(4) else {
        blockers.push(format!(
            "diagnostic exception vector {location} landing offset overflowed"
        ));
        return None;
    };
    let Some(vector_end) = landing_start.checked_add(4) else {
        blockers.push(format!(
            "diagnostic exception vector {location} end offset overflowed"
        ));
        return None;
    };
    if bytes < vector_end {
        blockers.push(format!(
            "{location} is smaller than diagnostic exception vector slot ({bytes:#x} < {vector_end:#x})"
        ));
        return None;
    }

    Some(DiagnosticExceptionVectorSlotRange {
        vector_start,
        eret_start,
        landing_start,
        vector_end,
    })
}

pub(crate) fn write_diagnostic_exception_vector_slot(
    slot: &mut [u8],
    range: DiagnosticExceptionVectorSlotRange,
) -> bool {
    slot[range.vector_start..range.eret_start].copy_from_slice(&AARCH64_HVC_1.to_le_bytes());
    slot[range.eret_start..range.landing_start].copy_from_slice(&AARCH64_ERET.to_le_bytes());
    slot[range.landing_start..range.vector_end].copy_from_slice(&AARCH64_HVC_0.to_le_bytes());
    let first = u32::from_le_bytes(
        slot[range.vector_start..range.eret_start]
            .try_into()
            .expect("four-byte vector slot"),
    );
    let second = u32::from_le_bytes(
        slot[range.eret_start..range.landing_start]
            .try_into()
            .expect("four-byte ERET vector slot"),
    );
    let third = u32::from_le_bytes(
        slot[range.landing_start..range.vector_end]
            .try_into()
            .expect("four-byte landing vector slot"),
    );
    first == AARCH64_HVC_1 && second == AARCH64_ERET && third == AARCH64_HVC_0
}

pub(crate) fn install_diagnostic_exception_vector_slot_preserving(
    memory: *mut c_void,
    bytes: usize,
    base_offset: usize,
    location: &str,
    blockers: &mut Vec<String>,
) -> Option<DiagnosticExceptionVectorSlotSnapshot> {
    if memory.is_null() {
        blockers.push(format!(
            "diagnostic exception vector {location} pointer was null"
        ));
        return None;
    }
    let range = diagnostic_exception_vector_slot_range(bytes, base_offset, location, blockers)?;
    unsafe {
        let slot = std::slice::from_raw_parts_mut(memory.cast::<u8>(), bytes);
        let original = slot[range.vector_start..range.vector_end]
            .try_into()
            .expect("diagnostic exception vector snapshot is 12 bytes");
        write_diagnostic_exception_vector_slot(slot, range).then_some(
            DiagnosticExceptionVectorSlotSnapshot {
                start: range.vector_start,
                original,
            },
        )
    }
}

pub(crate) fn restore_diagnostic_exception_vector_slot(
    memory: *mut c_void,
    bytes: usize,
    snapshot: DiagnosticExceptionVectorSlotSnapshot,
    location: &str,
    blockers: &mut Vec<String>,
) -> bool {
    if memory.is_null() {
        blockers.push(format!(
            "diagnostic exception vector {location} restore pointer was null"
        ));
        return false;
    }
    let Some(end) = snapshot
        .start
        .checked_add(DIAGNOSTIC_EXCEPTION_VECTOR_SLOT_BYTES)
    else {
        blockers.push(format!(
            "diagnostic exception vector {location} restore end overflowed"
        ));
        return false;
    };
    if bytes < end {
        blockers.push(format!(
            "{location} is smaller than diagnostic exception vector restore slot ({bytes:#x} < {end:#x})"
        ));
        return false;
    }
    unsafe {
        let slot = std::slice::from_raw_parts_mut(memory.cast::<u8>(), bytes);
        slot[snapshot.start..end].copy_from_slice(&snapshot.original);
        slot[snapshot.start..end] == snapshot.original
    }
}

pub(crate) fn populate_diagnostic_exception_vector_slot(
    memory: *mut c_void,
    bytes: usize,
    base_offset: usize,
    location: &str,
    blockers: &mut Vec<String>,
) -> bool {
    if memory.is_null() {
        blockers.push(format!(
            "diagnostic exception vector {location} pointer was null"
        ));
        return false;
    }
    let Some(range) =
        diagnostic_exception_vector_slot_range(bytes, base_offset, location, blockers)
    else {
        return false;
    };
    unsafe {
        let slot = std::slice::from_raw_parts_mut(memory.cast::<u8>(), bytes);
        write_diagnostic_exception_vector_slot(slot, range)
    }
}

pub(crate) fn guest_backing_offset(
    base: u64,
    region_start: u64,
    region_bytes: usize,
) -> Option<usize> {
    let offset = base.checked_sub(region_start)?;
    (offset < region_bytes as u64)
        .then_some(offset)
        .and_then(|offset| offset.try_into().ok())
}

pub(crate) fn populate_recommended_vector_base_diagnostic_vector_slot(
    recommendation: &WindowsArmUefiVectorBaseRecommendation,
    firmware_memory: *mut c_void,
    vars_memory: *mut c_void,
    guest_ram_memory: *mut c_void,
    slot_bytes: usize,
    guest_ram_bytes: usize,
    blockers: &mut Vec<String>,
) -> bool {
    let base = recommendation.base_virtual_address;
    if let Some(offset) =
        guest_backing_offset(base, WINDOWS_ARM_UEFI_LOW_CODE_ALIAS_IPA, slot_bytes)
            .or_else(|| guest_backing_offset(base, WINDOWS_ARM_UEFI_CODE_IPA, slot_bytes))
    {
        return populate_diagnostic_exception_vector_slot(
            firmware_memory,
            slot_bytes,
            offset,
            "recommended vector-base firmware pflash",
            blockers,
        );
    }
    if let Some(offset) =
        guest_backing_offset(base, WINDOWS_ARM_UEFI_LOW_VARS_ALIAS_IPA, slot_bytes)
            .or_else(|| guest_backing_offset(base, WINDOWS_ARM_UEFI_VARS_IPA, slot_bytes))
    {
        return populate_diagnostic_exception_vector_slot(
            vars_memory,
            slot_bytes,
            offset,
            "recommended vector-base vars pflash",
            blockers,
        );
    }
    if let Some(offset) = guest_backing_offset(base, WINDOWS_ARM_GUEST_RAM_IPA, guest_ram_bytes) {
        return populate_diagnostic_exception_vector_slot(
            guest_ram_memory,
            guest_ram_bytes,
            offset,
            "recommended vector-base guest RAM",
            blockers,
        );
    }

    blockers.push(format!(
        "recommended vector-base {base:#x} does not map to a mutable BridgeVM diagnostic backing"
    ));
    false
}

#[cfg(test)]
mod diagnostic_exception_vector_slot_tests {
    use super::*;

    #[test]
    fn preserving_install_restores_original_low_vector_bytes() {
        let base_offset = 0x40usize;
        let slot_start = base_offset + WINDOWS_ARM_DIAGNOSTIC_VECTOR_CURRENT_EL_SPX_SYNC_OFFSET;
        let slot_end = slot_start + DIAGNOSTIC_EXCEPTION_VECTOR_SLOT_BYTES;
        let mut memory = (0..(slot_end + 0x20))
            .map(|index| (index % 251) as u8)
            .collect::<Vec<_>>();
        let original = memory.clone();
        let mut blockers = Vec::new();

        let snapshot = install_diagnostic_exception_vector_slot_preserving(
            memory.as_mut_ptr().cast(),
            memory.len(),
            base_offset,
            "unit-test",
            &mut blockers,
        )
        .expect("diagnostic vector slot installs");

        assert!(blockers.is_empty());
        assert_eq!(snapshot.start, slot_start);
        assert_eq!(&snapshot.original, &original[slot_start..slot_end]);
        assert_eq!(
            u32::from_le_bytes(memory[slot_start..slot_start + 4].try_into().unwrap()),
            AARCH64_HVC_1
        );
        assert_eq!(
            u32::from_le_bytes(memory[slot_start + 4..slot_start + 8].try_into().unwrap()),
            AARCH64_ERET
        );
        assert_eq!(
            u32::from_le_bytes(memory[slot_start + 8..slot_end].try_into().unwrap()),
            AARCH64_HVC_0
        );
        assert_eq!(&memory[..slot_start], &original[..slot_start]);
        assert_eq!(&memory[slot_end..], &original[slot_end..]);

        assert!(restore_diagnostic_exception_vector_slot(
            memory.as_mut_ptr().cast(),
            memory.len(),
            snapshot,
            "unit-test",
            &mut blockers,
        ));
        assert!(blockers.is_empty());
        assert_eq!(memory, original);
    }

    #[test]
    fn preserving_install_rejects_invalid_slots_without_mutating_memory() {
        let mut blockers = Vec::new();
        let mut memory = vec![0xa5; WINDOWS_ARM_DIAGNOSTIC_VECTOR_CURRENT_EL_SPX_SYNC_OFFSET];
        let original = memory.clone();

        assert!(install_diagnostic_exception_vector_slot_preserving(
            memory.as_mut_ptr().cast(),
            memory.len(),
            0,
            "too-small",
            &mut blockers,
        )
        .is_none());
        assert_eq!(memory, original);
        assert!(blockers
            .iter()
            .any(|blocker| blocker.contains("too-small is smaller")));

        blockers.clear();
        assert!(install_diagnostic_exception_vector_slot_preserving(
            ptr::null_mut(),
            0x1000,
            0,
            "null",
            &mut blockers,
        )
        .is_none());
        assert!(blockers
            .iter()
            .any(|blocker| blocker.contains("null pointer was null")));
    }

    #[test]
    fn non_preserving_populate_writes_only_diagnostic_vector_slot() {
        let base_offset = 0x80usize;
        let slot_start = base_offset + WINDOWS_ARM_DIAGNOSTIC_VECTOR_CURRENT_EL_SPX_SYNC_OFFSET;
        let slot_end = slot_start + DIAGNOSTIC_EXCEPTION_VECTOR_SLOT_BYTES;
        let mut memory = (0..(slot_end + 0x20))
            .map(|index| 0xffu8.wrapping_sub((index % 251) as u8))
            .collect::<Vec<_>>();
        let original = memory.clone();
        let mut blockers = Vec::new();

        assert!(populate_diagnostic_exception_vector_slot(
            memory.as_mut_ptr().cast(),
            memory.len(),
            base_offset,
            "unit-test",
            &mut blockers,
        ));

        assert!(blockers.is_empty());
        assert_eq!(&memory[..slot_start], &original[..slot_start]);
        assert_eq!(&memory[slot_end..], &original[slot_end..]);
        assert_eq!(
            u32::from_le_bytes(memory[slot_start..slot_start + 4].try_into().unwrap()),
            AARCH64_HVC_1
        );
        assert_eq!(
            u32::from_le_bytes(memory[slot_start + 4..slot_start + 8].try_into().unwrap()),
            AARCH64_ERET
        );
        assert_eq!(
            u32::from_le_bytes(memory[slot_start + 8..slot_end].try_into().unwrap()),
            AARCH64_HVC_0
        );
    }

    #[test]
    fn recommended_vector_base_slot_install_resolves_low_pflash_alias() {
        let base_offset = WINDOWS_ARM_EXECUTABLE_DIAGNOSTIC_VECTOR_IPA as usize;
        let slot_start = base_offset + WINDOWS_ARM_DIAGNOSTIC_VECTOR_CURRENT_EL_SPX_SYNC_OFFSET;
        let slot_end = slot_start + DIAGNOSTIC_EXCEPTION_VECTOR_SLOT_BYTES;
        let mut firmware = vec![0_u8; slot_end + 0x20];
        let mut vars = vec![0_u8; 0x1000];
        let mut guest_ram = vec![0_u8; 0x1000];
        let recommendation = WindowsArmUefiVectorBaseRecommendation {
            base_virtual_address: WINDOWS_ARM_EXECUTABLE_DIAGNOSTIC_VECTOR_IPA,
            base_physical_address: Some(WINDOWS_ARM_EXECUTABLE_DIAGNOSTIC_VECTOR_IPA),
            current_el_spx_sync_instruction_word: Some(0),
            current_el_spx_sync_instruction_hint: "zero",
            reason: "unit-test",
        };
        let mut blockers = Vec::new();

        assert!(populate_recommended_vector_base_diagnostic_vector_slot(
            &recommendation,
            firmware.as_mut_ptr().cast(),
            vars.as_mut_ptr().cast(),
            guest_ram.as_mut_ptr().cast(),
            firmware.len(),
            guest_ram.len(),
            &mut blockers,
        ));

        assert!(blockers.is_empty());
        assert_eq!(
            u32::from_le_bytes(firmware[slot_start..slot_start + 4].try_into().unwrap()),
            AARCH64_HVC_1
        );
        assert_eq!(
            u32::from_le_bytes(firmware[slot_start + 4..slot_start + 8].try_into().unwrap()),
            AARCH64_ERET
        );
        assert_eq!(
            u32::from_le_bytes(firmware[slot_start + 8..slot_end].try_into().unwrap()),
            AARCH64_HVC_0
        );
        assert!(vars.iter().all(|byte| *byte == 0));
        assert!(guest_ram.iter().all(|byte| *byte == 0));
    }

    #[test]
    fn low_vector_recommended_vector_descriptor_remaps_to_real_vector_page() {
        let mut guest_ram = vec![0_u8; 0x4000];
        let tcr_el1 = Some(43);
        let ttbr0_el1 = Some(WINDOWS_ARM_GUEST_RAM_IPA);
        let recommendation = WindowsArmUefiVectorBaseRecommendation {
            base_virtual_address: WINDOWS_ARM_EXECUTABLE_DIAGNOSTIC_VECTOR_IPA,
            base_physical_address: Some(WINDOWS_ARM_EXECUTABLE_DIAGNOSTIC_VECTOR_IPA),
            current_el_spx_sync_instruction_word: Some(AARCH64_WFI),
            current_el_spx_sync_instruction_hint: "wfi",
            reason: "unit-test",
        };

        let patched = patch_low_vector_recommended_vector_descriptor(
            &recommendation,
            tcr_el1,
            ttbr0_el1,
            ptr::null_mut(),
            ptr::null_mut(),
            guest_ram.as_mut_ptr().cast(),
            guest_ram.len(),
        )
        .expect("low-vector L3 descriptor patches to recommended vector page");

        assert_eq!(patched.0, WINDOWS_ARM_GUEST_RAM_IPA);
        assert_eq!(patched.1, 0);
        assert_eq!(patched.2, 0x200f8f);

        let leaf = read_stage1_leaf_descriptor(
            Some(WINDOWS_ARM_DIAGNOSTIC_VECTOR_CURRENT_EL_SPX_SYNC_OFFSET as u64),
            tcr_el1,
            ttbr0_el1,
            ptr::null(),
            ptr::null(),
            guest_ram.as_ptr().cast(),
            guest_ram.len(),
        )
        .expect("patched low-vector descriptor is readable");

        assert_eq!(leaf.level, 3);
        assert_eq!(leaf.kind, "page");
        assert_eq!(leaf.descriptor, 0x200f8f);
        assert_eq!(
            leaf.output_address,
            Some(WINDOWS_ARM_EXECUTABLE_DIAGNOSTIC_VECTOR_IPA)
        );
        assert!(stage1_leaf_is_el1_executable(leaf));
    }
}

pub(crate) fn populate_platform_dtb_guest_ram(
    memory: *mut c_void,
    guest_ram_bytes: usize,
    dtb_blob: &[u8],
    blockers: &mut Vec<String>,
) -> bool {
    if memory.is_null() {
        blockers.push("platform DTB guest RAM pointer was null".to_string());
        return false;
    }
    if dtb_blob.is_empty() {
        blockers.push("platform DTB blob was empty".to_string());
        return false;
    }
    let dtb_offset: usize = match WINDOWS_ARM_PLATFORM_DTB_GUEST_RAM_OFFSET.try_into() {
        Ok(offset) => offset,
        Err(_) => {
            blockers.push("platform DTB guest RAM offset does not fit in usize".to_string());
            return false;
        }
    };
    let Some(dtb_end) = dtb_offset.checked_add(dtb_blob.len()) else {
        blockers.push("platform DTB guest RAM range overflowed".to_string());
        return false;
    };
    if dtb_end > guest_ram_bytes {
        blockers.push(format!(
            "guest RAM is smaller than the platform DTB handoff range ({guest_ram_bytes:#x} < {dtb_end:#x})"
        ));
        return false;
    }

    unsafe {
        ptr::copy_nonoverlapping(
            dtb_blob.as_ptr(),
            memory.cast::<u8>().add(dtb_offset),
            dtb_blob.len(),
        );
        let mapped = std::slice::from_raw_parts(memory.cast::<u8>(), guest_ram_bytes);
        mapped[dtb_offset..dtb_end] == dtb_blob[..]
            && read_be_u32(&mapped[dtb_offset..dtb_end], 0) == Some(FDT_MAGIC)
    }
}

#[derive(Debug, Default)]
pub(crate) struct PflashHvfMapOutcome {
    pub(crate) vm_create_status: Option<i32>,
    pub(crate) firmware_allocate_status: Option<i32>,
    pub(crate) vars_allocate_status: Option<i32>,
    pub(crate) firmware_map_status: Option<i32>,
    pub(crate) vars_map_status: Option<i32>,
    pub(crate) firmware_unmap_status: Option<i32>,
    pub(crate) vars_unmap_status: Option<i32>,
    pub(crate) firmware_deallocate_status: Option<i32>,
    pub(crate) vars_deallocate_status: Option<i32>,
    pub(crate) vm_destroy_status: Option<i32>,
    pub(crate) blockers: Vec<String>,
}

pub(crate) fn pflash_hvf_map_result(
    allowed: bool,
    attempted: bool,
    host: HvfHostCapabilities,
    pflash_map_verified: bool,
    firmware_source_bytes: Option<u64>,
    vars_source_bytes: Option<u64>,
    outcome: PflashHvfMapOutcome,
) -> WindowsArmUefiPflashHvfMapProbe {
    WindowsArmUefiPflashHvfMapProbe {
        allowed,
        attempted,
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
        pflash_map_verified,
        firmware_slot_ipa: WINDOWS_ARM_UEFI_CODE_IPA,
        vars_slot_ipa: WINDOWS_ARM_UEFI_VARS_IPA,
        slot_bytes: WINDOWS_ARM_UEFI_SLOT_BYTES,
        firmware_source_bytes,
        vars_source_bytes,
        firmware_map_flags: "read|exec",
        vars_map_flags: "read|write",
        vm_create_status: outcome.vm_create_status,
        firmware_allocate_status: outcome.firmware_allocate_status,
        vars_allocate_status: outcome.vars_allocate_status,
        firmware_map_status: outcome.firmware_map_status,
        vars_map_status: outcome.vars_map_status,
        firmware_unmap_status: outcome.firmware_unmap_status,
        vars_unmap_status: outcome.vars_unmap_status,
        firmware_deallocate_status: outcome.firmware_deallocate_status,
        vars_deallocate_status: outcome.vars_deallocate_status,
        vm_destroy_status: outcome.vm_destroy_status,
        blockers: outcome.blockers,
    }
}
