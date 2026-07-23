//! Stage-1 page-table walking, executable-leaf discovery and vector-base recommendation.
//!
//! Split out of the single 12,111-line apple.rs backend.

use super::*;
use crate::*;

pub(crate) fn read_stage1_leaf_descriptor(
    va: Option<u64>,
    tcr_el1: Option<u64>,
    ttbr0_el1: Option<u64>,
    firmware_memory: *const c_void,
    vars_memory: *const c_void,
    guest_ram_memory: *const c_void,
    guest_ram_bytes: usize,
) -> Option<Stage1LeafDescriptor> {
    let va = va?;
    let tcr = tcr_el1?;
    let ttbr0 = ttbr0_el1?;
    let tg0 = (tcr >> 14) & 0x3;
    if tg0 != 0 {
        return None;
    }
    let t0sz = tcr & 0x3f;
    if t0sz > 48 {
        return None;
    }
    let va_bits = 64 - t0sz;
    let start_level = match va_bits {
        40..=64 => 0,
        31..=39 => 1,
        22..=30 => 2,
        _ => 3,
    };
    let mut table_ipa = ttbr0 & 0x0000_ffff_ffff_f000;
    for level in start_level..=3 {
        let shift = 39u32.saturating_sub(level as u32 * 9);
        let index = (va >> shift) & 0x1ff;
        let entry_ipa = table_ipa.checked_add(index.checked_mul(8)?)?;
        let descriptor = read_known_guest_phys_u64(
            entry_ipa,
            firmware_memory,
            vars_memory,
            guest_ram_memory,
            guest_ram_bytes,
        )?;
        let kind = stage1_descriptor_kind(descriptor, level as u8);
        if kind == "table" {
            table_ipa = descriptor & 0x0000_ffff_ffff_f000;
            continue;
        }
        return Some(Stage1LeafDescriptor {
            level: level as u8,
            descriptor,
            kind,
            output_address: stage1_descriptor_output_address(descriptor, level as u8, kind),
            attr_index: ((descriptor >> 2) & 0x7) as u8,
            access_permissions: ((descriptor >> 6) & 0x3) as u8,
            shareability: ((descriptor >> 8) & 0x3) as u8,
            access_flag: descriptor & (1 << 10) != 0,
            pxn: descriptor & (1 << 53) != 0,
            uxn: descriptor & (1 << 54) != 0,
        });
    }
    None
}

pub(crate) fn collect_stage1_descriptor_samples(
    addresses: Stage1ExitAddresses,
    translation: Stage1TranslationContext,
) -> Vec<WindowsArmUefiStage1DescriptorSample> {
    let mut requests = vec![
        ("low-vector-base", Some(WINDOWS_ARM_UEFI_LOW_CODE_ALIAS_IPA)),
        (
            "low-vector-sync-slot",
            Some(
                WINDOWS_ARM_UEFI_LOW_CODE_ALIAS_IPA
                    + WINDOWS_ARM_DIAGNOSTIC_VECTOR_CURRENT_EL_SPX_SYNC_OFFSET as u64,
            ),
        ),
        ("firmware-reset-vector", Some(WINDOWS_ARM_UEFI_CODE_IPA)),
        (
            "pflash-diagnostic-vector-sync-slot",
            Some(
                WINDOWS_ARM_DIAGNOSTIC_VECTOR_IPA
                    + WINDOWS_ARM_DIAGNOSTIC_VECTOR_CURRENT_EL_SPX_SYNC_OFFSET as u64,
            ),
        ),
        (
            "guest-ram-diagnostic-vector-sync-slot",
            Some(
                WINDOWS_ARM_GUEST_RAM_DIAGNOSTIC_VECTOR_IPA
                    + WINDOWS_ARM_DIAGNOSTIC_VECTOR_CURRENT_EL_SPX_SYNC_OFFSET as u64,
            ),
        ),
        (
            "executable-diagnostic-vector-sync-slot",
            Some(
                WINDOWS_ARM_EXECUTABLE_DIAGNOSTIC_VECTOR_IPA
                    + WINDOWS_ARM_DIAGNOSTIC_VECTOR_CURRENT_EL_SPX_SYNC_OFFSET as u64,
            ),
        ),
        ("pc-after-exit", addresses.pc),
        ("vbar-el1", addresses.vbar_el1),
        ("elr-el1", addresses.elr_el1),
        ("far-el1", addresses.far_el1),
        ("sp-el1", addresses.sp_el1),
    ];
    requests.retain(|(_, va)| va.is_some());
    requests
        .into_iter()
        .map(|(label, va)| {
            stage1_descriptor_sample(
                label,
                va.expect("stage-1 descriptor sample VA is retained as Some"),
                translation,
            )
        })
        .collect()
}

pub(crate) fn collect_stage1_walk_entries(
    addresses: Stage1ExitAddresses,
    translation: Stage1TranslationContext,
) -> Vec<WindowsArmUefiStage1WalkEntry> {
    let mut requests = vec![
        (
            "low-vector-sync-slot",
            Some(
                WINDOWS_ARM_UEFI_LOW_CODE_ALIAS_IPA
                    + WINDOWS_ARM_DIAGNOSTIC_VECTOR_CURRENT_EL_SPX_SYNC_OFFSET as u64,
            ),
        ),
        ("pc-after-exit", addresses.pc),
        ("vbar-el1", addresses.vbar_el1),
        ("elr-el1", addresses.elr_el1),
        ("far-el1", addresses.far_el1),
        ("sp-el1", addresses.sp_el1),
        (
            "executable-diagnostic-vector-sync-slot",
            Some(
                WINDOWS_ARM_EXECUTABLE_DIAGNOSTIC_VECTOR_IPA
                    + WINDOWS_ARM_DIAGNOSTIC_VECTOR_CURRENT_EL_SPX_SYNC_OFFSET as u64,
            ),
        ),
    ];
    requests.retain(|(_, va)| va.is_some());
    let mut entries = Vec::new();
    for (label, va) in requests {
        entries.extend(stage1_walk_entries_for_address(
            label,
            va.expect("stage-1 walk VA is retained as Some"),
            translation,
        ));
    }
    entries
}

pub(crate) fn stage1_walk_entries_for_address(
    label: &'static str,
    virtual_address: u64,
    translation: Stage1TranslationContext,
) -> Vec<WindowsArmUefiStage1WalkEntry> {
    let Some(tcr) = translation.tcr_el1 else {
        return Vec::new();
    };
    let Some(ttbr0) = translation.ttbr0_el1 else {
        return Vec::new();
    };
    let tg0 = (tcr >> 14) & 0x3;
    if tg0 != 0 {
        return Vec::new();
    }
    let t0sz = tcr & 0x3f;
    if t0sz > 48 {
        return Vec::new();
    }
    let va_bits = 64 - t0sz;
    let start_level = match va_bits {
        40..=64 => 0,
        31..=39 => 1,
        22..=30 => 2,
        _ => 3,
    };
    let mut table_ipa = ttbr0 & 0x0000_ffff_ffff_f000;
    let mut entries = Vec::new();
    for level in start_level..=3 {
        let shift = 39u32.saturating_sub(level as u32 * 9);
        let index = (virtual_address >> shift) & 0x1ff;
        let Some(entry_ipa) = table_ipa.checked_add(index.saturating_mul(8)) else {
            break;
        };
        let descriptor = read_known_guest_phys_u64(
            entry_ipa,
            translation.memory.firmware_memory,
            translation.memory.vars_memory,
            translation.memory.guest_ram_memory,
            translation.memory.guest_ram_bytes,
        );
        let descriptor_kind = descriptor
            .map(|descriptor| stage1_descriptor_kind(descriptor, level as u8))
            .unwrap_or("not observed");
        let next_table_ipa = descriptor
            .filter(|_| descriptor_kind == "table")
            .map(|descriptor| descriptor & 0x0000_ffff_ffff_f000);
        entries.push(WindowsArmUefiStage1WalkEntry {
            label,
            virtual_address,
            region: windows_arm_guest_region_name(
                Some(virtual_address),
                translation.memory.guest_ram_bytes as u64,
            ),
            level: level as u8,
            table_ipa,
            index,
            entry_ipa,
            descriptor,
            descriptor_kind,
            next_table_ipa,
            output_address: descriptor.and_then(|descriptor| {
                stage1_descriptor_output_address(descriptor, level as u8, descriptor_kind)
            }),
            attr_index: descriptor.map(|descriptor| ((descriptor >> 2) & 0x7) as u8),
            access_permissions: descriptor.map(|descriptor| ((descriptor >> 6) & 0x3) as u8),
            shareability: descriptor.map(|descriptor| ((descriptor >> 8) & 0x3) as u8),
            access_flag: descriptor.map(|descriptor| descriptor & (1 << 10) != 0),
            pxn: descriptor.map(|descriptor| descriptor & (1 << 53) != 0),
            uxn: descriptor.map(|descriptor| descriptor & (1 << 54) != 0),
        });
        if let Some(next_table_ipa) = next_table_ipa {
            table_ipa = next_table_ipa;
            continue;
        }
        break;
    }
    entries
}

pub(crate) fn stage1_descriptor_sample(
    label: &'static str,
    virtual_address: u64,
    translation: Stage1TranslationContext,
) -> WindowsArmUefiStage1DescriptorSample {
    let leaf = read_stage1_leaf_descriptor(
        Some(virtual_address),
        translation.tcr_el1,
        translation.ttbr0_el1,
        translation.memory.firmware_memory,
        translation.memory.vars_memory,
        translation.memory.guest_ram_memory,
        translation.memory.guest_ram_bytes,
    );
    WindowsArmUefiStage1DescriptorSample {
        label,
        virtual_address,
        region: windows_arm_guest_region_name(
            Some(virtual_address),
            translation.memory.guest_ram_bytes as u64,
        ),
        level: leaf.map(|leaf| leaf.level),
        descriptor: leaf.map(|leaf| leaf.descriptor),
        descriptor_kind: leaf.map(|leaf| leaf.kind).unwrap_or("not observed"),
        output_address: leaf.and_then(|leaf| leaf.output_address),
        attr_index: leaf.map(|leaf| leaf.attr_index),
        access_permissions: leaf.map(|leaf| leaf.access_permissions),
        shareability: leaf.map(|leaf| leaf.shareability),
        access_flag: leaf.map(|leaf| leaf.access_flag),
        pxn: leaf.map(|leaf| leaf.pxn),
        uxn: leaf.map(|leaf| leaf.uxn),
    }
}

pub(crate) fn collect_stage1_executable_candidates(
    tcr_el1_after_exit: Option<u64>,
    ttbr0_el1_after_exit: Option<u64>,
    firmware_memory: *const c_void,
    vars_memory: *const c_void,
    guest_ram_memory: *const c_void,
    guest_ram_bytes: usize,
) -> Vec<WindowsArmUefiStage1ExecutableCandidate> {
    let memory = WindowsArmKnownGuestMemory {
        firmware_memory,
        vars_memory,
        guest_ram_memory,
        guest_ram_bytes,
    };
    let mut candidates = Vec::new();
    for (start, bytes) in stage1_executable_scan_ranges(guest_ram_bytes) {
        let Some(end) = start.checked_add(bytes) else {
            continue;
        };
        let mut va = start;
        while va < end && candidates.len() < WINDOWS_ARM_STAGE1_EXECUTABLE_SCAN_MAX_CANDIDATES {
            if let Some(leaf) = read_stage1_leaf_descriptor(
                Some(va),
                tcr_el1_after_exit,
                ttbr0_el1_after_exit,
                memory.firmware_memory,
                memory.vars_memory,
                memory.guest_ram_memory,
                guest_ram_bytes,
            ) {
                if stage1_leaf_is_el1_executable(leaf)
                    && !stage1_executable_leaf_already_reported(&candidates, leaf)
                {
                    candidates.push(build_stage1_executable_candidate(leaf, va, memory));
                }
            }
            va = match va.checked_add(WINDOWS_ARM_STAGE1_EXECUTABLE_SCAN_STEP) {
                Some(next) => next,
                None => break,
            };
        }
        if candidates.len() >= WINDOWS_ARM_STAGE1_EXECUTABLE_SCAN_MAX_CANDIDATES {
            break;
        }
    }
    candidates
}

pub(crate) fn stage1_executable_scan_ranges(guest_ram_bytes: usize) -> [(u64, u64); 5] {
    [
        (
            WINDOWS_ARM_UEFI_LOW_CODE_ALIAS_IPA,
            WINDOWS_ARM_UEFI_SLOT_BYTES,
        ),
        (
            WINDOWS_ARM_UEFI_LOW_VARS_ALIAS_IPA,
            WINDOWS_ARM_UEFI_SLOT_BYTES,
        ),
        (WINDOWS_ARM_UEFI_CODE_IPA, WINDOWS_ARM_UEFI_SLOT_BYTES),
        (WINDOWS_ARM_UEFI_VARS_IPA, WINDOWS_ARM_UEFI_SLOT_BYTES),
        (WINDOWS_ARM_GUEST_RAM_IPA, guest_ram_bytes as u64),
    ]
}

pub(crate) fn stage1_leaf_is_el1_executable(leaf: Stage1LeafDescriptor) -> bool {
    matches!(leaf.kind, "block" | "page") && !leaf.pxn
}

pub(crate) fn stage1_executable_leaf_already_reported(
    candidates: &[WindowsArmUefiStage1ExecutableCandidate],
    leaf: Stage1LeafDescriptor,
) -> bool {
    candidates.iter().any(|candidate| {
        candidate.descriptor == leaf.descriptor && candidate.output_address == leaf.output_address
    })
}

pub(crate) fn build_stage1_executable_candidate(
    leaf: Stage1LeafDescriptor,
    virtual_address: u64,
    memory: WindowsArmKnownGuestMemory,
) -> WindowsArmUefiStage1ExecutableCandidate {
    let vector_sync = collect_stage1_vector_sync_probe_for_leaf(leaf, virtual_address, memory);
    let vector_base_scan =
        collect_stage1_vector_base_candidates_for_leaf(leaf, virtual_address, memory);
    let recommended_vector_base_candidate =
        recommend_stage1_vector_base_candidate(&vector_base_scan.candidates).or_else(|| {
            recommend_stage1_executable_leaf_base_vector(leaf, virtual_address, vector_sync)
        });
    WindowsArmUefiStage1ExecutableCandidate {
        virtual_address,
        region: windows_arm_guest_region_name(Some(virtual_address), memory.guest_ram_bytes as u64),
        level: leaf.level,
        descriptor: leaf.descriptor,
        descriptor_kind: leaf.kind,
        output_address: leaf.output_address,
        span_bytes: stage1_descriptor_span_bytes(leaf.level, leaf.kind),
        vector_sync_virtual_address: vector_sync.virtual_address,
        vector_sync_physical_address: vector_sync.physical_address,
        vector_sync_instruction_word: vector_sync.instruction_word,
        vector_sync_instruction_hint: vector_sync.instruction_hint,
        vector_base_scan_scanned_count: vector_base_scan.scanned_count,
        vector_base_scan_suppressed_count: vector_base_scan.suppressed_count,
        vector_base_scan_limit_reached: vector_base_scan.limit_reached,
        recommended_vector_base_candidate,
        vector_base_candidates: vector_base_scan.candidates,
        attr_index: leaf.attr_index,
        access_permissions: leaf.access_permissions,
        shareability: leaf.shareability,
        access_flag: leaf.access_flag,
        pxn: leaf.pxn,
        uxn: leaf.uxn,
    }
}

pub(crate) fn collect_stage1_vector_sync_probe_for_leaf(
    leaf: Stage1LeafDescriptor,
    leaf_sample_virtual_address: u64,
    memory: WindowsArmKnownGuestMemory,
) -> WindowsArmUefiVectorSyncProbe {
    let virtual_address = leaf_sample_virtual_address
        .checked_add(WINDOWS_ARM_DIAGNOSTIC_VECTOR_CURRENT_EL_SPX_SYNC_OFFSET as u64);
    let physical_address = virtual_address.and_then(|sync_va| {
        translate_stage1_leaf_virtual_address(leaf, leaf_sample_virtual_address, sync_va)
    });
    let instruction_word = physical_address.and_then(|sync_ipa| memory.read_u32(sync_ipa));
    WindowsArmUefiVectorSyncProbe {
        virtual_address,
        physical_address,
        instruction_word,
        instruction_hint: instruction_word
            .map(aarch64_instruction_hint)
            .unwrap_or("not observed"),
    }
}

pub(crate) fn recommend_stage1_executable_leaf_base_vector(
    leaf: Stage1LeafDescriptor,
    leaf_sample_virtual_address: u64,
    vector_sync: WindowsArmUefiVectorSyncProbe,
) -> Option<WindowsArmUefiVectorBaseRecommendation> {
    let span_bytes = stage1_descriptor_span_bytes(leaf.level, leaf.kind)?;
    if span_bytes == 0 || !span_bytes.is_power_of_two() {
        return None;
    }
    let base_virtual_address = leaf_sample_virtual_address & !(span_bytes - 1);
    Some(WindowsArmUefiVectorBaseRecommendation {
        base_virtual_address,
        base_physical_address: translate_stage1_leaf_virtual_address(
            leaf,
            leaf_sample_virtual_address,
            base_virtual_address,
        ),
        current_el_spx_sync_instruction_word: vector_sync.instruction_word,
        current_el_spx_sync_instruction_hint: vector_sync.instruction_hint,
        reason: "fallback-el1-executable-leaf-base-empty-vector-scan",
    })
}

pub(crate) fn collect_stage1_vector_base_candidates_for_leaf(
    leaf: Stage1LeafDescriptor,
    leaf_sample_virtual_address: u64,
    memory: WindowsArmKnownGuestMemory,
) -> WindowsArmUefiVectorBaseCandidateScan {
    let empty_scan = || WindowsArmUefiVectorBaseCandidateScan {
        scanned_count: 0,
        suppressed_count: 0,
        limit_reached: false,
        candidates: Vec::new(),
    };
    let Some(span_bytes) = stage1_descriptor_span_bytes(leaf.level, leaf.kind) else {
        return empty_scan();
    };
    if span_bytes == 0 || !span_bytes.is_power_of_two() {
        return empty_scan();
    }

    let leaf_base_virtual_address = leaf_sample_virtual_address & !(span_bytes - 1);
    let Some(leaf_end_virtual_address) = leaf_base_virtual_address.checked_add(span_bytes) else {
        return empty_scan();
    };
    let mut candidates = Vec::new();
    let mut scanned_count = 0_u32;
    let mut suppressed_count = 0_u32;
    let mut limit_reached = false;
    let mut base_virtual_address = leaf_base_virtual_address;
    while base_virtual_address < leaf_end_virtual_address {
        scanned_count = scanned_count.saturating_add(1);
        let base_physical_address = translate_stage1_leaf_virtual_address(
            leaf,
            leaf_sample_virtual_address,
            base_virtual_address,
        );
        let slots = read_stage1_vector_slot_instructions(
            leaf,
            leaf_sample_virtual_address,
            base_virtual_address,
            memory,
        );
        let populated_slot_count = slots.populated_slot_count();
        if populated_slot_count > 0 {
            candidates.push(WindowsArmUefiVectorBaseCandidate {
                base_virtual_address,
                base_physical_address,
                current_el_sp0_sync_instruction_word: slots.current_el_sp0_sync_instruction_word,
                current_el_spx_sync_instruction_word: slots.current_el_spx_sync_instruction_word,
                lower_aarch64_sync_instruction_word: slots.lower_aarch64_sync_instruction_word,
                lower_aarch32_sync_instruction_word: slots.lower_aarch32_sync_instruction_word,
                current_el_spx_sync_instruction_hint: slots.current_el_spx_sync_instruction_hint(),
                populated_slot_count,
            });
            if candidates.len() >= WINDOWS_ARM_VECTOR_BASE_SCAN_MAX_PER_LEAF {
                limit_reached = true;
                break;
            }
        } else {
            suppressed_count = suppressed_count.saturating_add(1);
        }
        base_virtual_address =
            match base_virtual_address.checked_add(WINDOWS_ARM_VECTOR_BASE_SCAN_ALIGNMENT) {
                Some(next) => next,
                None => break,
            };
    }
    WindowsArmUefiVectorBaseCandidateScan {
        scanned_count,
        suppressed_count,
        limit_reached,
        candidates,
    }
}

pub(crate) fn recommend_stage1_vector_base_candidate(
    candidates: &[WindowsArmUefiVectorBaseCandidate],
) -> Option<WindowsArmUefiVectorBaseRecommendation> {
    if let Some(candidate) = candidates.iter().find(|candidate| {
        vector_slot_instruction_is_non_diagnostic_populated(
            candidate.current_el_spx_sync_instruction_word,
        )
    }) {
        return Some(vector_base_recommendation(
            candidate,
            "current-el-spx-populated-non-diagnostic",
        ));
    }

    if let Some(candidate) = candidates
        .iter()
        .find(|candidate| vector_base_candidate_has_non_diagnostic_populated_slot(candidate))
    {
        return Some(vector_base_recommendation(
            candidate,
            "any-vector-slot-populated-non-diagnostic",
        ));
    }

    candidates
        .iter()
        .find(|candidate| candidate.populated_slot_count > 0)
        .map(|candidate| {
            vector_base_recommendation(candidate, "fallback-first-populated-vector-base")
        })
}

pub(crate) fn vector_base_recommendation(
    candidate: &WindowsArmUefiVectorBaseCandidate,
    reason: &'static str,
) -> WindowsArmUefiVectorBaseRecommendation {
    WindowsArmUefiVectorBaseRecommendation {
        base_virtual_address: candidate.base_virtual_address,
        base_physical_address: candidate.base_physical_address,
        current_el_spx_sync_instruction_word: candidate.current_el_spx_sync_instruction_word,
        current_el_spx_sync_instruction_hint: candidate.current_el_spx_sync_instruction_hint,
        reason,
    }
}

pub(crate) fn vector_base_candidate_has_non_diagnostic_populated_slot(
    candidate: &WindowsArmUefiVectorBaseCandidate,
) -> bool {
    [
        candidate.current_el_sp0_sync_instruction_word,
        candidate.current_el_spx_sync_instruction_word,
        candidate.lower_aarch64_sync_instruction_word,
        candidate.lower_aarch32_sync_instruction_word,
    ]
    .into_iter()
    .any(vector_slot_instruction_is_non_diagnostic_populated)
}

pub(crate) fn vector_slot_instruction_is_non_diagnostic_populated(word: Option<u32>) -> bool {
    crate::windows_arm_vector_slot_instruction_is_non_diagnostic_populated(word)
}

pub(crate) fn read_stage1_vector_slot_instructions(
    leaf: Stage1LeafDescriptor,
    leaf_sample_virtual_address: u64,
    base_virtual_address: u64,
    memory: WindowsArmKnownGuestMemory,
) -> Stage1VectorSlotInstructions {
    Stage1VectorSlotInstructions {
        current_el_sp0_sync_instruction_word: read_stage1_vector_slot_instruction_word(
            leaf,
            leaf_sample_virtual_address,
            base_virtual_address,
            0x000,
            memory,
        ),
        current_el_spx_sync_instruction_word: read_stage1_vector_slot_instruction_word(
            leaf,
            leaf_sample_virtual_address,
            base_virtual_address,
            0x200,
            memory,
        ),
        lower_aarch64_sync_instruction_word: read_stage1_vector_slot_instruction_word(
            leaf,
            leaf_sample_virtual_address,
            base_virtual_address,
            0x400,
            memory,
        ),
        lower_aarch32_sync_instruction_word: read_stage1_vector_slot_instruction_word(
            leaf,
            leaf_sample_virtual_address,
            base_virtual_address,
            0x600,
            memory,
        ),
    }
}

pub(crate) fn read_stage1_vector_slot_instruction_word(
    leaf: Stage1LeafDescriptor,
    leaf_sample_virtual_address: u64,
    base_virtual_address: u64,
    slot_offset: u64,
    memory: WindowsArmKnownGuestMemory,
) -> Option<u32> {
    let slot_virtual_address = base_virtual_address.checked_add(slot_offset)?;
    let slot_physical_address = translate_stage1_leaf_virtual_address(
        leaf,
        leaf_sample_virtual_address,
        slot_virtual_address,
    )?;
    memory.read_u32(slot_physical_address)
}

pub(crate) fn vector_slot_instruction_is_populated(word: Option<u32>) -> bool {
    crate::windows_arm_vector_slot_instruction_is_populated(word)
}

#[cfg(test)]
mod stage1_vector_base_candidate_tests {
    use super::*;

    fn write_pflash_word(firmware_memory: &mut [u8], ipa: u64, word: u32) {
        let offset = ipa
            .checked_sub(WINDOWS_ARM_UEFI_LOW_CODE_ALIAS_IPA)
            .expect("test IPA is in low pflash alias") as usize;
        firmware_memory[offset..offset + 4].copy_from_slice(&word.to_le_bytes());
    }

    #[test]
    fn vector_base_scan_filters_erased_slots_and_caps_reported_candidates() {
        let mut firmware_memory = vec![0; WINDOWS_ARM_UEFI_SLOT_BYTES as usize];
        let leaf_sample_virtual_address = WINDOWS_ARM_EXECUTABLE_DIAGNOSTIC_VECTOR_IPA;
        let leaf = Stage1LeafDescriptor {
            level: 2,
            descriptor: 0x200f8d,
            kind: "block",
            output_address: Some(WINDOWS_ARM_EXECUTABLE_DIAGNOSTIC_VECTOR_IPA),
            attr_index: 3,
            access_permissions: 0,
            shareability: 3,
            access_flag: true,
            pxn: false,
            uxn: false,
        };

        write_pflash_word(
            &mut firmware_memory,
            leaf_sample_virtual_address
                + WINDOWS_ARM_DIAGNOSTIC_VECTOR_CURRENT_EL_SPX_SYNC_OFFSET as u64,
            0xffff_ffff,
        );
        for index in 1..=12_u64 {
            write_pflash_word(
                &mut firmware_memory,
                leaf_sample_virtual_address
                    + index * WINDOWS_ARM_VECTOR_BASE_SCAN_ALIGNMENT
                    + WINDOWS_ARM_DIAGNOSTIC_VECTOR_CURRENT_EL_SPX_SYNC_OFFSET as u64,
                AARCH64_HVC_0,
            );
        }
        write_pflash_word(
            &mut firmware_memory,
            leaf_sample_virtual_address
                + 2 * WINDOWS_ARM_VECTOR_BASE_SCAN_ALIGNMENT
                + WINDOWS_ARM_DIAGNOSTIC_VECTOR_CURRENT_EL_SPX_SYNC_OFFSET as u64,
            AARCH64_WFI,
        );

        let scan = collect_stage1_vector_base_candidates_for_leaf(
            leaf,
            leaf_sample_virtual_address,
            WindowsArmKnownGuestMemory {
                firmware_memory: firmware_memory.as_ptr().cast(),
                vars_memory: ptr::null(),
                guest_ram_memory: ptr::null(),
                guest_ram_bytes: 0,
            },
        );

        assert_eq!(
            scan.candidates.len(),
            WINDOWS_ARM_VECTOR_BASE_SCAN_MAX_PER_LEAF
        );
        assert_eq!(scan.scanned_count, 9);
        assert_eq!(scan.suppressed_count, 1);
        assert!(scan.limit_reached);

        let first = &scan.candidates[0];
        assert_eq!(
            first.base_virtual_address,
            leaf_sample_virtual_address + WINDOWS_ARM_VECTOR_BASE_SCAN_ALIGNMENT
        );
        assert_eq!(
            first.base_physical_address,
            Some(first.base_virtual_address)
        );
        assert_eq!(
            first.current_el_spx_sync_instruction_word,
            Some(AARCH64_HVC_0)
        );
        assert_eq!(first.current_el_spx_sync_instruction_hint, "hvc-0");
        assert_eq!(first.populated_slot_count, 1);
        assert!(scan.candidates.iter().all(|candidate| {
            candidate.base_virtual_address % WINDOWS_ARM_VECTOR_BASE_SCAN_ALIGNMENT == 0
                && candidate.current_el_spx_sync_instruction_word != Some(0xffff_ffff)
        }));

        let recommendation = recommend_stage1_vector_base_candidate(&scan.candidates)
            .expect("non-diagnostic vector candidate should be recommended");
        assert_eq!(
            recommendation.base_virtual_address,
            leaf_sample_virtual_address + 2 * WINDOWS_ARM_VECTOR_BASE_SCAN_ALIGNMENT
        );
        assert_eq!(
            recommendation.base_physical_address,
            Some(recommendation.base_virtual_address)
        );
        assert_eq!(
            recommendation.current_el_spx_sync_instruction_word,
            Some(AARCH64_WFI)
        );
        assert_eq!(recommendation.current_el_spx_sync_instruction_hint, "wfi");
        assert_eq!(
            recommendation.reason,
            "current-el-spx-populated-non-diagnostic"
        );
    }
}

pub(crate) fn stage1_descriptor_kind(descriptor: u64, level: u8) -> &'static str {
    match (descriptor & 0x3, level) {
        (0, _) => "invalid",
        (1, 0..=2) => "block",
        (1, _) => "reserved",
        (3, 0..=2) => "table",
        (3, _) => "page",
        _ => "reserved",
    }
}

pub(crate) fn stage1_descriptor_output_address(
    descriptor: u64,
    level: u8,
    kind: &'static str,
) -> Option<u64> {
    let shift = match (kind, level) {
        ("block", 0) => 39,
        ("block", 1) => 30,
        ("block", 2) => 21,
        ("page", 3) => 12,
        _ => return None,
    };
    let address_bits_mask = 0x0000_ffff_ffff_ffffu64;
    Some(descriptor & address_bits_mask & !((1u64 << shift) - 1))
}

pub(crate) fn stage1_descriptor_span_bytes(level: u8, kind: &'static str) -> Option<u64> {
    let shift = match (kind, level) {
        ("block", 0) => 39,
        ("block", 1) => 30,
        ("block", 2) => 21,
        ("page", 3) => 12,
        _ => return None,
    };
    Some(1u64 << shift)
}

pub(crate) fn translate_stage1_leaf_virtual_address(
    leaf: Stage1LeafDescriptor,
    leaf_sample_virtual_address: u64,
    virtual_address: u64,
) -> Option<u64> {
    let output_address = leaf.output_address?;
    let span_bytes = stage1_descriptor_span_bytes(leaf.level, leaf.kind)?;
    if span_bytes == 0 || !span_bytes.is_power_of_two() {
        return None;
    }
    let leaf_base_virtual_address = leaf_sample_virtual_address & !(span_bytes - 1);
    let leaf_end_virtual_address = leaf_base_virtual_address.checked_add(span_bytes)?;
    if virtual_address < leaf_base_virtual_address || virtual_address >= leaf_end_virtual_address {
        return None;
    }
    output_address.checked_add(virtual_address - leaf_base_virtual_address)
}
