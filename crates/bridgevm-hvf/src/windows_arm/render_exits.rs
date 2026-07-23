//! Split out of run_loop_render.rs: per-exit report and nested telemetry.

use super::*;

impl WindowsArmUefiFirmwareRunLoopProbe {
    pub(crate) fn render_exits(&self, output: &mut String) {
        if self.exits.is_empty() {
            output.push_str("- none\n");
        } else {
            for exit in &self.exits {
                output.push_str(&format!(
                    "- Exit {}: run={}, reason={}, exception_class={}, exception_class_name={}, syndrome={}, abort_iss={}, abort_fault_status={}, abort_fault_status_name={}, va={}, va_region={}, pa={}, pa_region={}, pc={}, instruction={}, instruction_hint={}, pc_stage1_leaf_level={}, pc_stage1_leaf_descriptor={}, pc_stage1_leaf_kind={}, pc_stage1_leaf_pxn={}, pc_stage1_leaf_uxn={}, x0={}, x1={}, x2={}, x3={}, x4={}, cpsr={}, vbar_el1={}, elr_el1={}, esr_el1={}, esr_el1_class_name={}, esr_el1_fault_status_name={}, far_el1={}, spsr_el1={}, sctlr_el1={}, sctlr_el1_mmu_enabled={}, tcr_el1={}, ttbr0_el1={}, ttbr1_el1={}, mair_el1={}, sp_el1={}, diagnosis={}, watchdog={}, vtimer_auto_mask={}, vtimer_auto_mask_get={}, vtimer_rearm_cval={}, vtimer_rearm_cval_set={}, vtimer_ppi_pending_recorded={}, vtimer_irq_line_assertable={}, vtimer_gic_group1_enabled={}, vtimer_gic_priority_mask={}, vtimer_gic_running_priority={}, vtimer_gic_priority_threshold={}, vtimer_gic_pending_intid={}, vtimer_pending_irq={}, vtimer_unmask={}, handled={}\n",
                    exit.index,
                    render_optional_status_name(exit.run_status),
                    render_optional_exit_reason_name(exit.exit_reason),
                    render_optional_u64(exit.exit_exception_class),
                    render_optional_exception_class_name(exit.exit_exception_class),
                    render_optional_u64(exit.exit_syndrome),
                    render_optional_abort_iss(exit.exit_syndrome),
                    render_optional_abort_fault_status(exit.exit_syndrome),
                    render_optional_abort_fault_status_name(exit.exit_syndrome),
                    render_optional_u64(exit.exit_virtual_address),
                    windows_arm_guest_region_name(exit.exit_virtual_address, self.guest_ram_bytes),
                    render_optional_u64(exit.exit_physical_address),
                    windows_arm_guest_region_name(exit.exit_physical_address, self.guest_ram_bytes),
                    render_optional_u64(exit.pc_after_exit),
                    render_optional_instruction_word(exit.instruction_word_after_exit),
                    exit.instruction_hint_after_exit,
                    render_optional_u8(exit.pc_stage1_leaf_level_after_exit),
                    render_optional_u64(exit.pc_stage1_leaf_descriptor_after_exit),
                    exit.pc_stage1_leaf_descriptor_kind_after_exit,
                    render_optional_bool(exit.pc_stage1_leaf_pxn_after_exit),
                    render_optional_bool(exit.pc_stage1_leaf_uxn_after_exit),
                    render_optional_u64(exit.x0_after_exit),
                    render_optional_u64(exit.x1_after_exit),
                    render_optional_u64(exit.x2_after_exit),
                    render_optional_u64(exit.x3_after_exit),
                    render_optional_u64(exit.x4_after_exit),
                    render_optional_u64(exit.cpsr_after_exit),
                    render_optional_u64(exit.vbar_el1_after_exit),
                    render_optional_u64(exit.elr_el1_after_exit),
                    render_optional_u64(exit.esr_el1_after_exit),
                    render_optional_esr_exception_class_name(exit.esr_el1_after_exit),
                    render_optional_abort_fault_status_name(exit.esr_el1_after_exit),
                    render_optional_u64(exit.far_el1_after_exit),
                    render_optional_u64(exit.spsr_el1_after_exit),
                    render_optional_u64(exit.sctlr_el1_after_exit),
                    render_optional_sctlr_mmu_enabled(exit.sctlr_el1_after_exit),
                    render_optional_u64(exit.tcr_el1_after_exit),
                    render_optional_u64(exit.ttbr0_el1_after_exit),
                    render_optional_u64(exit.ttbr1_el1_after_exit),
                    render_optional_u64(exit.mair_el1_after_exit),
                    render_optional_u64(exit.sp_el1_after_exit),
                    windows_arm_firmware_run_loop_exit_diagnosis(exit),
                    render_optional_status_name(exit.watchdog_cancel_status),
                    render_optional_bool(exit.vtimer_auto_mask_after_exit),
                    render_optional_status_name(exit.vtimer_auto_mask_get_status),
                    render_optional_u64(exit.vtimer_rearm_cval_value),
                    render_optional_status_name(exit.vtimer_rearm_cval_set_status),
                    render_optional_bool(exit.vtimer_ppi_pending_recorded),
                    render_optional_bool(exit.vtimer_irq_line_assertable),
                    render_optional_bool(exit.vtimer_gic_group1_enabled),
                    render_optional_u8(exit.vtimer_gic_priority_mask),
                    render_optional_u8(exit.vtimer_gic_running_priority),
                    render_optional_u8(exit.vtimer_gic_priority_threshold),
                    render_optional_gic_intid(exit.vtimer_gic_pending_intid),
                    render_optional_status_name(exit.vtimer_pending_irq_set_status),
                    render_optional_status_name(exit.vtimer_unmask_status),
                    exit.handled
                ));
                if exit.stage1_descriptor_samples_after_exit.is_empty() {
                    output.push_str("  Stage-1 descriptor samples: none\n");
                } else {
                    output.push_str("  Stage-1 descriptor samples:\n");
                    for sample in &exit.stage1_descriptor_samples_after_exit {
                        output.push_str(&format!(
                            "  - label={}, va={:#x}, region={}, level={}, descriptor={}, kind={}, output={}, attr_index={}, ap={}, sh={}, af={}, pxn={}, uxn={}\n",
                            sample.label,
                            sample.virtual_address,
                            sample.region,
                            render_optional_u8(sample.level),
                            render_optional_u64(sample.descriptor),
                            sample.descriptor_kind,
                            render_optional_u64(sample.output_address),
                            render_optional_u8(sample.attr_index),
                            render_optional_u8(sample.access_permissions),
                            render_optional_u8(sample.shareability),
                            render_optional_bool(sample.access_flag),
                            render_optional_bool(sample.pxn),
                            render_optional_bool(sample.uxn),
                        ));
                    }
                }
                if exit.stage1_walk_entries_after_exit.is_empty() {
                    output.push_str("  Stage-1 walk entries: none\n");
                } else {
                    output.push_str("  Stage-1 walk entries:\n");
                    for entry in &exit.stage1_walk_entries_after_exit {
                        output.push_str(&format!(
                            "  - label={}, va={:#x}, region={}, level={}, table_ipa={:#x}, index={:#x}, entry_ipa={:#x}, descriptor={}, kind={}, next_table={}, output={}, attr_index={}, ap={}, sh={}, af={}, pxn={}, uxn={}\n",
                            entry.label,
                            entry.virtual_address,
                            entry.region,
                            entry.level,
                            entry.table_ipa,
                            entry.index,
                            entry.entry_ipa,
                            render_optional_u64(entry.descriptor),
                            entry.descriptor_kind,
                            render_optional_u64(entry.next_table_ipa),
                            render_optional_u64(entry.output_address),
                            render_optional_u8(entry.attr_index),
                            render_optional_u8(entry.access_permissions),
                            render_optional_u8(entry.shareability),
                            render_optional_bool(entry.access_flag),
                            render_optional_bool(entry.pxn),
                            render_optional_bool(entry.uxn),
                        ));
                    }
                }
                if exit.stage1_executable_candidates_after_exit.is_empty() {
                    output.push_str("  Stage-1 EL1-executable leaf candidates: none\n");
                } else {
                    output.push_str("  Stage-1 EL1-executable leaf candidates:\n");
                    for candidate in &exit.stage1_executable_candidates_after_exit {
                        output.push_str(&format!(
                            "  - va={:#x}, region={}, level={}, descriptor={:#x}, kind={}, output={}, span={}, vector_sync_va={}, vector_sync_pa={}, vector_sync_instruction={}, vector_sync_hint={}, vector_base_scan_scanned={}, vector_base_scan_suppressed={}, vector_base_scan_limit_reached={}, attr_index={}, ap={}, sh={}, af={}, pxn={}, uxn={}\n",
                            candidate.virtual_address,
                            candidate.region,
                            candidate.level,
                            candidate.descriptor,
                            candidate.descriptor_kind,
                            render_optional_u64(candidate.output_address),
                            render_optional_u64(candidate.span_bytes),
                            render_optional_u64(candidate.vector_sync_virtual_address),
                            render_optional_u64(candidate.vector_sync_physical_address),
                            render_optional_instruction_word(candidate.vector_sync_instruction_word),
                            candidate.vector_sync_instruction_hint,
                            candidate.vector_base_scan_scanned_count,
                            candidate.vector_base_scan_suppressed_count,
                            candidate.vector_base_scan_limit_reached,
                            candidate.attr_index,
                            candidate.access_permissions,
                            candidate.shareability,
                            candidate.access_flag,
                            candidate.pxn,
                            candidate.uxn,
                        ));
                        if let Some(recommendation) = &candidate.recommended_vector_base_candidate {
                            output.push_str(&format!(
                                "    Recommended vector base: base_va={:#x}, base_pa={}, current_el_spx_sync={}, current_el_spx_hint={}, reason={}\n",
                                recommendation.base_virtual_address,
                                render_optional_u64(recommendation.base_physical_address),
                                render_optional_instruction_word(
                                    recommendation.current_el_spx_sync_instruction_word,
                                ),
                                recommendation.current_el_spx_sync_instruction_hint,
                                recommendation.reason,
                            ));
                        } else {
                            output.push_str("    Recommended vector base: none\n");
                        }
                        if candidate.vector_base_candidates.is_empty() {
                            output.push_str("    Vector base candidates: none\n");
                        } else {
                            output.push_str("    Vector base candidates:\n");
                            for vector_candidate in &candidate.vector_base_candidates {
                                output.push_str(&format!(
                                    "    - base_va={:#x}, base_pa={}, current_el_sp0_sync={}, current_el_spx_sync={}, current_el_spx_hint={}, lower_aarch64_sync={}, lower_aarch32_sync={}, populated_slots={}\n",
                                    vector_candidate.base_virtual_address,
                                    render_optional_u64(vector_candidate.base_physical_address),
                                    render_optional_instruction_word(
                                        vector_candidate.current_el_sp0_sync_instruction_word,
                                    ),
                                    render_optional_instruction_word(
                                        vector_candidate.current_el_spx_sync_instruction_word,
                                    ),
                                    vector_candidate.current_el_spx_sync_instruction_hint,
                                    render_optional_instruction_word(
                                        vector_candidate.lower_aarch64_sync_instruction_word,
                                    ),
                                    render_optional_instruction_word(
                                        vector_candidate.lower_aarch32_sync_instruction_word,
                                    ),
                                    vector_candidate.populated_slot_count,
                                ));
                            }
                        }
                    }
                }
            }
        }
        output.push_str(&format!(
            "vCPU destroy status name: {}\n",
            render_optional_status_name(self.vcpu_destroy_status)
        ));
        output.push_str(&format!(
            "Firmware unmap status name: {}\n",
            render_optional_status_name(self.firmware_unmap_status)
        ));
        output.push_str(&format!(
            "Vars unmap status name: {}\n",
            render_optional_status_name(self.vars_unmap_status)
        ));
        output.push_str(&format!(
            "Low firmware alias unmap status name: {}\n",
            render_optional_status_name(self.low_firmware_alias_unmap_status)
        ));
        output.push_str(&format!(
            "Low vars alias unmap status name: {}\n",
            render_optional_status_name(self.low_vars_alias_unmap_status)
        ));
        output.push_str(&format!(
            "Guest RAM unmap status name: {}\n",
            render_optional_status_name(self.guest_ram_unmap_status)
        ));
        output.push_str(&format!(
            "Firmware deallocate status name: {}\n",
            render_optional_status_name(self.firmware_deallocate_status)
        ));
        output.push_str(&format!(
            "Vars deallocate status name: {}\n",
            render_optional_status_name(self.vars_deallocate_status)
        ));
        output.push_str(&format!(
            "Guest RAM deallocate status name: {}\n",
            render_optional_status_name(self.guest_ram_deallocate_status)
        ));
        output.push_str(&format!(
            "VM destroy status name: {}\n",
            render_optional_status_name(self.vm_destroy_status)
        ));
    }
}
