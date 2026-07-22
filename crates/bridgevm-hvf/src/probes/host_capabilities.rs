//! Host-capability probe: what the local Hypervisor.framework reports.
//!
//! Moved verbatim out of the legacy probe monolith. Items keep the visibility
//! they had at the crate root and are re-exported there, so the public API is
//! unchanged. The live backends live in `crate::platform`.

use crate::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HvfHostCapabilities {
    pub available: bool,
    pub host: &'static str,
    pub default_ipa_bits: Option<u32>,
    pub max_ipa_bits: Option<u32>,
    pub el2_supported: Option<bool>,
    pub blockers: Vec<String>,
}

impl HvfHostCapabilities {
    pub fn render_text(&self) -> String {
        let mut output = String::new();
        output.push_str("HVF host capabilities\n");
        output.push_str(&format!("Available: {}\n", self.available));
        output.push_str(&format!("Host: {}\n", self.host));
        output.push_str(&format!(
            "Default IPA bits: {}\n",
            self.default_ipa_bits
                .map_or_else(|| "unknown".to_string(), |bits| bits.to_string())
        ));
        output.push_str(&format!(
            "Max IPA bits: {}\n",
            self.max_ipa_bits
                .map_or_else(|| "unknown".to_string(), |bits| bits.to_string())
        ));
        output.push_str(&format!(
            "EL2 supported: {}\n",
            self.el2_supported
                .map_or_else(|| "unknown".to_string(), |supported| supported.to_string())
        ));
        if self.blockers.is_empty() {
            output.push_str("Blockers: none\n");
        } else {
            output.push_str("Blockers:\n");
            for blocker in &self.blockers {
                output.push_str(&format!("- {blocker}\n"));
            }
        }
        output
    }
}

pub fn query_hvf_host_capabilities() -> HvfHostCapabilities {
    platform::query_hvf_host_capabilities()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_capabilities_render_without_percentages() {
        let capabilities = HvfHostCapabilities {
            available: false,
            host: "unsupported",
            default_ipa_bits: None,
            max_ipa_bits: None,
            el2_supported: None,
            blockers: vec!["unsupported host".to_string()],
        };
        let output = capabilities.render_text();

        assert!(output.contains("HVF host capabilities"));
        assert!(output.contains("Available: false"));
        assert!(output.contains("Default IPA bits: unknown"));
        assert!(output.contains("Blockers:"));
        assert!(!output.contains('%'));
    }

    #[test]
    fn vm_create_probe_render_records_successful_empty_vm_boundary() {
        let probe = HvfVmCreateProbe {
            allowed: true,
            attempted: true,
            created: true,
            destroyed: true,
            host: HvfHostCapabilities {
                available: true,
                host: "macos-aarch64",
                default_ipa_bits: Some(36),
                max_ipa_bits: Some(40),
                el2_supported: Some(true),
                blockers: Vec::new(),
            },
            create_status: Some(0),
            destroy_status: Some(0),
            blockers: Vec::new(),
        };
        let output = probe.render_text();

        assert!(output.contains("Allowed: true"));
        assert!(output.contains("Attempted: true"));
        assert!(output.contains("Created: true"));
        assert!(output.contains("Destroyed: true"));
        assert!(output.contains("Create status: 0x0"));
        assert!(output.contains("Create status name: HV_SUCCESS"));
        assert!(output.contains("Destroy status: 0x0"));
        assert!(output.contains("Destroy status name: HV_SUCCESS"));
        assert!(output.contains("Blockers: none"));
        assert!(!output.contains('%'));
    }

    #[test]
    fn vcpu_create_probe_render_records_successful_lifecycle_boundary() {
        let probe = HvfVcpuCreateProbe {
            allowed: true,
            attempted: true,
            vm_created: true,
            vcpu_created: true,
            vcpu_destroyed: true,
            vm_destroyed: true,
            host: HvfHostCapabilities {
                available: true,
                host: "macos-aarch64",
                default_ipa_bits: Some(36),
                max_ipa_bits: Some(40),
                el2_supported: Some(true),
                blockers: Vec::new(),
            },
            vm_create_status: Some(0),
            vcpu_create_status: Some(0),
            vcpu_destroy_status: Some(0),
            vm_destroy_status: Some(0),
            blockers: Vec::new(),
        };
        let output = probe.render_text();

        assert!(output.contains("Allowed: true"));
        assert!(output.contains("Attempted: true"));
        assert!(output.contains("VM created: true"));
        assert!(output.contains("vCPU created: true"));
        assert!(output.contains("vCPU destroyed: true"));
        assert!(output.contains("VM destroyed: true"));
        assert!(output.contains("VM create status name: HV_SUCCESS"));
        assert!(output.contains("vCPU create status name: HV_SUCCESS"));
        assert!(output.contains("vCPU destroy status name: HV_SUCCESS"));
        assert!(output.contains("VM destroy status name: HV_SUCCESS"));
        assert!(output.contains("Blockers: none"));
        assert!(!output.contains('%'));
    }

    #[test]
    fn vcpu_run_probe_render_records_canceled_run_boundary() {
        let probe = HvfVcpuRunProbe {
            allowed: true,
            attempted: true,
            vm_created: true,
            vcpu_created: true,
            cancel_requested: true,
            run_attempted: true,
            run_boundary_observed: true,
            vcpu_destroyed: true,
            vm_destroyed: true,
            host: HvfHostCapabilities {
                available: true,
                host: "macos-aarch64",
                default_ipa_bits: Some(36),
                max_ipa_bits: Some(40),
                el2_supported: Some(true),
                blockers: Vec::new(),
            },
            vm_create_status: Some(0),
            vcpu_create_status: Some(0),
            cancel_status: Some(0),
            run_status: Some(0),
            exit_reason: Some(0),
            vcpu_destroy_status: Some(0),
            vm_destroy_status: Some(0),
            blockers: Vec::new(),
        };
        let output = probe.render_text();

        assert!(output.contains("Allowed: true"));
        assert!(output.contains("Attempted: true"));
        assert!(output.contains("VM created: true"));
        assert!(output.contains("vCPU created: true"));
        assert!(output.contains("Cancel requested: true"));
        assert!(output.contains("Run attempted: true"));
        assert!(output.contains("Run boundary observed: true"));
        assert!(output.contains("Cancel status name: HV_SUCCESS"));
        assert!(output.contains("Run status name: HV_SUCCESS"));
        assert!(output.contains("Exit reason name: HV_EXIT_REASON_CANCELED"));
        assert!(output.contains("vCPU destroy status name: HV_SUCCESS"));
        assert!(output.contains("VM destroy status name: HV_SUCCESS"));
        assert!(output.contains("Blockers: none"));
        assert!(!output.contains('%'));
    }

    #[test]
    fn interrupt_timer_probe_render_records_successful_boundary() {
        let probe = HvfInterruptTimerProbe {
            allowed: true,
            attempted: true,
            vm_created: true,
            vcpu_created: true,
            pending_irq_set: true,
            pending_irq_cleared: true,
            vtimer_masked: true,
            vtimer_unmasked: true,
            vtimer_offset_set: true,
            boundary_observed: true,
            vcpu_destroyed: true,
            vm_destroyed: true,
            host: HvfHostCapabilities {
                available: true,
                host: "macos-aarch64",
                default_ipa_bits: Some(36),
                max_ipa_bits: Some(40),
                el2_supported: Some(true),
                blockers: Vec::new(),
            },
            vtimer_offset_value: 0x1000,
            vm_create_status: Some(0),
            vcpu_create_status: Some(0),
            irq_set_status: Some(0),
            irq_get_after_set_status: Some(0),
            irq_pending_after_set: Some(true),
            irq_clear_status: Some(0),
            irq_get_after_clear_status: Some(0),
            irq_pending_after_clear: Some(false),
            vtimer_mask_set_status: Some(0),
            vtimer_mask_get_status: Some(0),
            vtimer_mask_after_set: Some(true),
            vtimer_unmask_status: Some(0),
            vtimer_unmask_get_status: Some(0),
            vtimer_mask_after_clear: Some(false),
            vtimer_offset_set_status: Some(0),
            vtimer_offset_get_status: Some(0),
            vtimer_offset_after_set: Some(0x1000),
            vcpu_destroy_status: Some(0),
            vm_destroy_status: Some(0),
            blockers: Vec::new(),
        };
        let output = probe.render_text();

        assert!(output.contains("Allowed: true"));
        assert!(output.contains("Attempted: true"));
        assert!(output.contains("VM created: true"));
        assert!(output.contains("vCPU created: true"));
        assert!(output.contains("Pending IRQ set: true"));
        assert!(output.contains("Pending IRQ after set: true"));
        assert!(output.contains("Pending IRQ cleared: true"));
        assert!(output.contains("Pending IRQ after clear: false"));
        assert!(output.contains("VTimer masked: true"));
        assert!(output.contains("VTimer mask after set: true"));
        assert!(output.contains("VTimer unmasked: true"));
        assert!(output.contains("VTimer mask after clear: false"));
        assert!(output.contains("VTimer offset set: true"));
        assert!(output.contains("VTimer offset requested: 0x1000"));
        assert!(output.contains("VTimer offset after set: 0x1000"));
        assert!(output.contains("Interrupt/timer boundary observed: true"));
        assert!(output.contains("IRQ set status name: HV_SUCCESS"));
        assert!(output.contains("VTimer offset get status name: HV_SUCCESS"));
        assert!(output.contains("Blockers: none"));
        assert!(!output.contains('%'));
    }

    #[test]
    fn vtimer_exit_probe_render_records_successful_timer_boundary() {
        let probe = HvfVtimerExitProbe {
            allowed: true,
            attempted: true,
            vm_created: true,
            memory_allocated: true,
            memory_mapped: true,
            vcpu_created: true,
            pc_set: true,
            cpsr_set: true,
            vtimer_offset_set: true,
            cntv_cval_set: true,
            cntv_ctl_set: true,
            vtimer_unmasked: true,
            run_attempted: true,
            vtimer_exit_observed: true,
            pending_irq_injected: true,
            vtimer_mask_observed_after_exit: Some(true),
            vtimer_unmasked_after_exit: true,
            watchdog_cancel_fired: false,
            vcpu_destroyed: true,
            memory_unmapped: true,
            vm_destroyed: true,
            memory_deallocated: true,
            host: HvfHostCapabilities {
                available: true,
                host: "macos-aarch64",
                default_ipa_bits: Some(36),
                max_ipa_bits: Some(40),
                el2_supported: Some(true),
                blockers: Vec::new(),
            },
            ipa_start: 0x4000_0000,
            bytes: 16 * 1024,
            instructions: "WFI; HVC #0",
            vtimer_offset_value: 0,
            cntv_cval_value: 0,
            cntv_ctl_value: 1,
            vm_create_status: Some(0),
            allocate_status: Some(0),
            map_status: Some(0),
            vcpu_create_status: Some(0),
            pc_set_status: Some(0),
            cpsr_set_status: Some(0),
            vtimer_offset_set_status: Some(0),
            cntv_cval_set_status: Some(0),
            cntv_ctl_set_status: Some(0),
            vtimer_unmask_status: Some(0),
            run_status: Some(0),
            exit_reason: Some(2),
            exit_syndrome: Some(0),
            exit_virtual_address: Some(0),
            exit_physical_address: Some(0),
            watchdog_cancel_status: None,
            pending_irq_set_status: Some(0),
            vtimer_mask_get_after_exit_status: Some(0),
            vtimer_unmask_after_exit_status: Some(0),
            vcpu_destroy_status: Some(0),
            unmap_status: Some(0),
            vm_destroy_status: Some(0),
            deallocate_status: Some(0),
            blockers: Vec::new(),
        };
        let output = probe.render_text();

        assert!(output.contains("Allowed: true"));
        assert!(output.contains("Attempted: true"));
        assert!(output.contains("VM created: true"));
        assert!(output.contains("Memory allocated: true"));
        assert!(output.contains("Memory mapped: true"));
        assert!(output.contains("vCPU created: true"));
        assert!(output.contains("VTimer offset set: true"));
        assert!(output.contains("CNTV_CVAL_EL0 set: true"));
        assert!(output.contains("CNTV_CTL_EL0 set: true"));
        assert!(output.contains("VTimer unmasked: true"));
        assert!(output.contains("Run attempted: true"));
        assert!(output.contains("VTimer exit observed: true"));
        assert!(output.contains("Pending IRQ injected: true"));
        assert!(output.contains("VTimer mask observed after exit: true"));
        assert!(output.contains("VTimer unmasked after exit: true"));
        assert!(output.contains("Watchdog cancel fired: false"));
        assert!(output.contains("VTimer offset requested: 0x0"));
        assert!(output.contains("CNTV_CVAL_EL0 requested: 0x0"));
        assert!(output.contains("CNTV_CTL_EL0 requested: 0x1"));
        assert!(output.contains("Run status name: HV_SUCCESS"));
        assert!(output.contains("Exit reason name: HV_EXIT_REASON_VTIMER_ACTIVATED"));
        assert!(output.contains("VTimer mask get after exit status name: HV_SUCCESS"));
        assert!(output.contains("Pending IRQ set status name: HV_SUCCESS"));
        assert!(output.contains("VTimer unmask after exit status name: HV_SUCCESS"));
        assert!(output.contains("Blockers: none"));
        assert!(!output.contains("qemu-system"));
        assert!(!output.contains('%'));
    }

    #[test]
    fn memory_map_probe_render_records_successful_map_boundary() {
        let probe = HvfMemoryMapProbe {
            allowed: true,
            attempted: true,
            vm_created: true,
            memory_allocated: true,
            memory_mapped: true,
            memory_unmapped: true,
            memory_deallocated: true,
            vm_destroyed: true,
            host: HvfHostCapabilities {
                available: true,
                host: "macos-aarch64",
                default_ipa_bits: Some(36),
                max_ipa_bits: Some(40),
                el2_supported: Some(true),
                blockers: Vec::new(),
            },
            ipa_start: 0x4000_0000,
            bytes: 16 * 1024,
            vm_create_status: Some(0),
            allocate_status: Some(0),
            map_status: Some(0),
            unmap_status: Some(0),
            deallocate_status: Some(0),
            vm_destroy_status: Some(0),
            blockers: Vec::new(),
        };
        let output = probe.render_text();

        assert!(output.contains("Allowed: true"));
        assert!(output.contains("Attempted: true"));
        assert!(output.contains("VM created: true"));
        assert!(output.contains("Memory allocated: true"));
        assert!(output.contains("Memory mapped: true"));
        assert!(output.contains("Memory unmapped: true"));
        assert!(output.contains("Memory deallocated: true"));
        assert!(output.contains("VM destroyed: true"));
        assert!(output.contains("VM create status name: HV_SUCCESS"));
        assert!(output.contains("Allocate status name: HV_SUCCESS"));
        assert!(output.contains("Map status name: HV_SUCCESS"));
        assert!(output.contains("Unmap status name: HV_SUCCESS"));
        assert!(output.contains("Deallocate status name: HV_SUCCESS"));
        assert!(output.contains("VM destroy status name: HV_SUCCESS"));
        assert!(output.contains("Blockers: none"));
        assert!(!output.contains('%'));
    }

    #[test]
    fn guest_entry_probe_render_records_exception_boundary() {
        let probe = HvfGuestEntryProbe {
            allowed: true,
            attempted: true,
            vm_created: true,
            memory_allocated: true,
            memory_mapped: true,
            vcpu_created: true,
            pc_set: true,
            cpsr_set: true,
            run_attempted: true,
            entry_boundary_observed: true,
            watchdog_cancel_fired: false,
            vcpu_destroyed: true,
            memory_unmapped: true,
            vm_destroyed: true,
            memory_deallocated: true,
            host: HvfHostCapabilities {
                available: true,
                host: "macos-aarch64",
                default_ipa_bits: Some(36),
                max_ipa_bits: Some(40),
                el2_supported: Some(true),
                blockers: Vec::new(),
            },
            ipa_start: 0x4000_0000,
            bytes: 16 * 1024,
            instruction: "HVC #0",
            vm_create_status: Some(0),
            allocate_status: Some(0),
            map_status: Some(0),
            vcpu_create_status: Some(0),
            pc_set_status: Some(0),
            cpsr_set_status: Some(0),
            run_status: Some(0),
            exit_reason: Some(1),
            exit_syndrome: Some(0x5a00_0000),
            exit_virtual_address: Some(0),
            exit_physical_address: Some(0),
            watchdog_cancel_status: None,
            vcpu_destroy_status: Some(0),
            unmap_status: Some(0),
            vm_destroy_status: Some(0),
            deallocate_status: Some(0),
            blockers: Vec::new(),
        };
        let output = probe.render_text();

        assert!(output.contains("Allowed: true"));
        assert!(output.contains("Attempted: true"));
        assert!(output.contains("PC set: true"));
        assert!(output.contains("CPSR set: true"));
        assert!(output.contains("Run attempted: true"));
        assert!(output.contains("Entry boundary observed: true"));
        assert!(output.contains("Watchdog cancel fired: false"));
        assert!(output.contains("Run status name: HV_SUCCESS"));
        assert!(output.contains("Exit reason name: HV_EXIT_REASON_EXCEPTION"));
        assert!(output.contains("Exit syndrome: 0x5a000000"));
        assert!(output.contains("Blockers: none"));
        assert!(!output.contains('%'));
    }

    #[test]
    fn guest_exit_loop_probe_render_records_two_exception_boundaries() {
        let probe = HvfGuestExitLoopProbe {
            allowed: true,
            attempted: true,
            vm_created: true,
            memory_allocated: true,
            memory_mapped: true,
            vcpu_created: true,
            initial_pc_set: true,
            cpsr_set: true,
            first_run_attempted: true,
            first_exit_observed: true,
            pc_read_after_first_exit: true,
            pc_advanced: true,
            second_run_attempted: true,
            second_exit_observed: true,
            exit_loop_observed: true,
            watchdog_cancel_fired: false,
            vcpu_destroyed: true,
            memory_unmapped: true,
            vm_destroyed: true,
            memory_deallocated: true,
            host: HvfHostCapabilities {
                available: true,
                host: "macos-aarch64",
                default_ipa_bits: Some(36),
                max_ipa_bits: Some(40),
                el2_supported: Some(true),
                blockers: Vec::new(),
            },
            ipa_start: 0x4000_0000,
            bytes: 16 * 1024,
            instructions: "HVC #0; HVC #1",
            vm_create_status: Some(0),
            allocate_status: Some(0),
            map_status: Some(0),
            vcpu_create_status: Some(0),
            initial_pc_set_status: Some(0),
            cpsr_set_status: Some(0),
            first_run_status: Some(0),
            first_exit_reason: Some(1),
            first_exit_syndrome: Some(0x5a00_0000),
            first_exit_virtual_address: Some(0),
            first_exit_physical_address: Some(0),
            first_watchdog_cancel_status: None,
            pc_read_status: Some(0),
            pc_after_first_exit: Some(0x4000_0004),
            pc_advance_status: Some(0),
            second_run_status: Some(0),
            second_exit_reason: Some(1),
            second_exit_syndrome: Some(0x5a00_0001),
            second_exit_virtual_address: Some(0),
            second_exit_physical_address: Some(0),
            second_watchdog_cancel_status: None,
            vcpu_destroy_status: Some(0),
            unmap_status: Some(0),
            vm_destroy_status: Some(0),
            deallocate_status: Some(0),
            blockers: Vec::new(),
        };
        let output = probe.render_text();

        assert!(output.contains("Allowed: true"));
        assert!(output.contains("Attempted: true"));
        assert!(output.contains("Initial PC set: true"));
        assert!(output.contains("CPSR set: true"));
        assert!(output.contains("First run attempted: true"));
        assert!(output.contains("First exit observed: true"));
        assert!(output.contains("PC read after first exit: true"));
        assert!(output.contains("PC advanced: true"));
        assert!(output.contains("Second run attempted: true"));
        assert!(output.contains("Second exit observed: true"));
        assert!(output.contains("Exit loop observed: true"));
        assert!(output.contains("Watchdog cancel fired: false"));
        assert!(output.contains("First run status name: HV_SUCCESS"));
        assert!(output.contains("First exit reason name: HV_EXIT_REASON_EXCEPTION"));
        assert!(output.contains("First exit syndrome: 0x5a000000"));
        assert!(output.contains("PC after first exit: 0x40000004"));
        assert!(output.contains("PC advance status name: HV_SUCCESS"));
        assert!(output.contains("Second run status name: HV_SUCCESS"));
        assert!(output.contains("Second exit reason name: HV_EXIT_REASON_EXCEPTION"));
        assert!(output.contains("Second exit syndrome: 0x5a000001"));
        assert!(output.contains("Blockers: none"));
        assert!(!output.contains('%'));
    }

    #[test]
    fn mmio_read_probe_render_records_data_abort_boundary() {
        let probe = HvfMmioReadExitProbe {
            allowed: true,
            attempted: true,
            vm_created: true,
            memory_allocated: true,
            memory_mapped: true,
            vcpu_created: true,
            pc_set: true,
            cpsr_set: true,
            address_register_set: true,
            run_attempted: true,
            mmio_exit_observed: true,
            watchdog_cancel_fired: false,
            vcpu_destroyed: true,
            memory_unmapped: true,
            vm_destroyed: true,
            memory_deallocated: true,
            host: HvfHostCapabilities {
                available: true,
                host: "macos-aarch64",
                default_ipa_bits: Some(36),
                max_ipa_bits: Some(40),
                el2_supported: Some(true),
                blockers: Vec::new(),
            },
            code_ipa_start: 0x4000_0000,
            mmio_ipa: 0x5000_0000,
            bytes: 16 * 1024,
            instruction: "LDR X0, [X1]",
            vm_create_status: Some(0),
            allocate_status: Some(0),
            map_status: Some(0),
            vcpu_create_status: Some(0),
            pc_set_status: Some(0),
            cpsr_set_status: Some(0),
            address_register_set_status: Some(0),
            run_status: Some(0),
            exit_reason: Some(1),
            exit_syndrome: Some(0x93c0_8006),
            exit_virtual_address: Some(0x5000_0000),
            exit_physical_address: Some(0x5000_0000),
            watchdog_cancel_status: None,
            vcpu_destroy_status: Some(0),
            unmap_status: Some(0),
            vm_destroy_status: Some(0),
            deallocate_status: Some(0),
            blockers: Vec::new(),
        };
        let output = probe.render_text();

        assert!(output.contains("Allowed: true"));
        assert!(output.contains("Attempted: true"));
        assert!(output.contains("Address register set: true"));
        assert!(output.contains("Run attempted: true"));
        assert!(output.contains("MMIO exit observed: true"));
        assert!(output.contains("Watchdog cancel fired: false"));
        assert!(output.contains("Code IPA start: 0x40000000"));
        assert!(output.contains("MMIO IPA: 0x50000000"));
        assert!(output.contains("Run status name: HV_SUCCESS"));
        assert!(output.contains("Exit reason name: HV_EXIT_REASON_EXCEPTION"));
        assert!(output.contains("Exit syndrome: 0x93c08006"));
        assert!(output.contains("Exit virtual address: 0x50000000"));
        assert!(output.contains("Exit physical address: 0x50000000"));
        assert!(output.contains("Blockers: none"));
        assert!(!output.contains('%'));
    }

    #[test]
    fn mmio_read_emulation_probe_render_records_continuation_boundary() {
        let probe = HvfMmioReadEmulationProbe {
            allowed: true,
            attempted: true,
            vm_created: true,
            memory_allocated: true,
            memory_mapped: true,
            vcpu_created: true,
            pc_set: true,
            cpsr_set: true,
            address_register_set: true,
            first_run_attempted: true,
            mmio_exit_observed: true,
            pc_read_after_mmio_exit: true,
            emulated_value_injected: true,
            pc_advanced: true,
            second_run_attempted: true,
            continuation_exit_observed: true,
            emulated_value_preserved: true,
            watchdog_cancel_fired: false,
            vcpu_destroyed: true,
            memory_unmapped: true,
            vm_destroyed: true,
            memory_deallocated: true,
            host: HvfHostCapabilities {
                available: true,
                host: "macos-aarch64",
                default_ipa_bits: Some(36),
                max_ipa_bits: Some(40),
                el2_supported: Some(true),
                blockers: Vec::new(),
            },
            code_ipa_start: 0x4000_0000,
            mmio_ipa: 0x5000_0000,
            bytes: 16 * 1024,
            instructions: "LDR X0, [X1]; HVC #0",
            emulated_value: 0x1234_5678_9abc_def0,
            vm_create_status: Some(0),
            allocate_status: Some(0),
            map_status: Some(0),
            vcpu_create_status: Some(0),
            pc_set_status: Some(0),
            cpsr_set_status: Some(0),
            address_register_set_status: Some(0),
            first_run_status: Some(0),
            mmio_exit_reason: Some(1),
            mmio_exit_syndrome: Some(0x93c0_8006),
            mmio_exit_virtual_address: Some(0x5000_0000),
            mmio_exit_physical_address: Some(0x5000_0000),
            first_watchdog_cancel_status: None,
            pc_read_status: Some(0),
            pc_after_mmio_exit: Some(0x4000_0000),
            emulated_value_set_status: Some(0),
            pc_advance_status: Some(0),
            second_run_status: Some(0),
            continuation_exit_reason: Some(1),
            continuation_exit_syndrome: Some(0x5a00_0000),
            continuation_exit_virtual_address: Some(0),
            continuation_exit_physical_address: Some(0),
            second_watchdog_cancel_status: None,
            emulated_value_read_status: Some(0),
            emulated_value_after_continue: Some(0x1234_5678_9abc_def0),
            vcpu_destroy_status: Some(0),
            unmap_status: Some(0),
            vm_destroy_status: Some(0),
            deallocate_status: Some(0),
            blockers: Vec::new(),
        };
        let output = probe.render_text();

        assert!(output.contains("Allowed: true"));
        assert!(output.contains("Attempted: true"));
        assert!(output.contains("MMIO exit observed: true"));
        assert!(output.contains("PC read after MMIO exit: true"));
        assert!(output.contains("Emulated value injected: true"));
        assert!(output.contains("PC advanced: true"));
        assert!(output.contains("Continuation exit observed: true"));
        assert!(output.contains("Emulated value preserved: true"));
        assert!(output.contains("MMIO exit syndrome: 0x93c08006"));
        assert!(output.contains("MMIO exit virtual address: 0x50000000"));
        assert!(output.contains("PC after MMIO exit: 0x40000000"));
        assert!(output.contains("Continuation exit syndrome: 0x5a000000"));
        assert!(output.contains("Emulated value after continue: 0x123456789abcdef0"));
        assert!(output.contains("Blockers: none"));
        assert!(!output.contains('%'));
    }

    #[test]
    fn mmio_write_emulation_probe_render_records_continuation_boundary() {
        let probe = HvfMmioWriteEmulationProbe {
            allowed: true,
            attempted: true,
            vm_created: true,
            memory_allocated: true,
            memory_mapped: true,
            vcpu_created: true,
            pc_set: true,
            cpsr_set: true,
            write_value_register_set: true,
            address_register_set: true,
            first_run_attempted: true,
            mmio_exit_observed: true,
            pc_read_after_mmio_exit: true,
            write_value_captured: true,
            pc_advanced: true,
            second_run_attempted: true,
            continuation_exit_observed: true,
            write_value_preserved: true,
            watchdog_cancel_fired: false,
            vcpu_destroyed: true,
            memory_unmapped: true,
            vm_destroyed: true,
            memory_deallocated: true,
            host: HvfHostCapabilities {
                available: true,
                host: "macos-aarch64",
                default_ipa_bits: Some(36),
                max_ipa_bits: Some(40),
                el2_supported: Some(true),
                blockers: Vec::new(),
            },
            code_ipa_start: 0x4000_0000,
            mmio_ipa: 0x5000_0000,
            bytes: 16 * 1024,
            instructions: "STR X0, [X1]; HVC #0",
            write_value: 0x0fed_cba9_8765_4321,
            vm_create_status: Some(0),
            allocate_status: Some(0),
            map_status: Some(0),
            vcpu_create_status: Some(0),
            pc_set_status: Some(0),
            cpsr_set_status: Some(0),
            write_value_register_set_status: Some(0),
            address_register_set_status: Some(0),
            first_run_status: Some(0),
            mmio_exit_reason: Some(1),
            mmio_exit_syndrome: Some(0x93c0_8046),
            mmio_exit_virtual_address: Some(0x5000_0000),
            mmio_exit_physical_address: Some(0x5000_0000),
            first_watchdog_cancel_status: None,
            pc_read_status: Some(0),
            pc_after_mmio_exit: Some(0x4000_0000),
            write_value_capture_status: Some(0),
            captured_write_value: Some(0x0fed_cba9_8765_4321),
            pc_advance_status: Some(0),
            second_run_status: Some(0),
            continuation_exit_reason: Some(1),
            continuation_exit_syndrome: Some(0x5a00_0000),
            continuation_exit_virtual_address: Some(0),
            continuation_exit_physical_address: Some(0),
            second_watchdog_cancel_status: None,
            write_value_after_continue_status: Some(0),
            write_value_after_continue: Some(0x0fed_cba9_8765_4321),
            vcpu_destroy_status: Some(0),
            unmap_status: Some(0),
            vm_destroy_status: Some(0),
            deallocate_status: Some(0),
            blockers: Vec::new(),
        };
        let output = probe.render_text();

        assert!(output.contains("Allowed: true"));
        assert!(output.contains("Attempted: true"));
        assert!(output.contains("Write value register set: true"));
        assert!(output.contains("MMIO exit observed: true"));
        assert!(output.contains("PC read after MMIO exit: true"));
        assert!(output.contains("Write value captured: true"));
        assert!(output.contains("PC advanced: true"));
        assert!(output.contains("Continuation exit observed: true"));
        assert!(output.contains("Write value preserved: true"));
        assert!(output.contains("MMIO exit syndrome: 0x93c08046"));
        assert!(output.contains("MMIO exit virtual address: 0x50000000"));
        assert!(output.contains("PC after MMIO exit: 0x40000000"));
        assert!(output.contains("Continuation exit syndrome: 0x5a000000"));
        assert!(output.contains("Captured write value: 0xfedcba987654321"));
        assert!(output.contains("Write value after continue: 0xfedcba987654321"));
        assert!(output.contains("Blockers: none"));
        assert!(!output.contains('%'));
    }
}
