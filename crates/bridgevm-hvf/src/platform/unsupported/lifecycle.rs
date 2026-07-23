//! Split out of unsupported.rs by responsibility.

use super::super::*;
use super::*;
use crate::*;

pub fn probe_hvf_vm_create(allow_create: bool, host: HvfHostCapabilities) -> HvfVmCreateProbe {
    HvfVmCreateProbe {
        allowed: allow_create,
        attempted: false,
        created: false,
        destroyed: false,
        host,
        create_status: None,
        destroy_status: None,
        blockers: vec![
            "Apple Hypervisor.framework VM create/destroy probe is only available on Apple Silicon macOS".to_string(),
        ],
    }
}

pub fn probe_hvf_vcpu_create(allow_create: bool, host: HvfHostCapabilities) -> HvfVcpuCreateProbe {
    HvfVcpuCreateProbe {
        allowed: allow_create,
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
        blockers: vec![
            "Apple Hypervisor.framework vCPU create/destroy probe is only available on Apple Silicon macOS".to_string(),
        ],
    }
}

pub fn probe_hvf_vcpu_run(allow_run: bool, host: HvfHostCapabilities) -> HvfVcpuRunProbe {
    HvfVcpuRunProbe {
        allowed: allow_run,
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
        blockers: vec![
            "Apple Hypervisor.framework vCPU run/cancel probe is only available on Apple Silicon macOS".to_string(),
        ],
    }
}

pub fn probe_hvf_interrupt_timer(
    allow_probe: bool,
    host: HvfHostCapabilities,
) -> HvfInterruptTimerProbe {
    HvfInterruptTimerProbe {
        allowed: allow_probe,
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
        vtimer_offset_value: 0x1000,
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
        blockers: vec![
            "Apple Hypervisor.framework interrupt/timer probe is only available on Apple Silicon macOS".to_string(),
        ],
    }
}

pub fn probe_hvf_vtimer_exit(allow_probe: bool, host: HvfHostCapabilities) -> HvfVtimerExitProbe {
    HvfVtimerExitProbe {
        allowed: allow_probe,
        attempted: false,
        vm_created: false,
        memory_allocated: false,
        memory_mapped: false,
        vcpu_created: false,
        pc_set: false,
        cpsr_set: false,
        vtimer_offset_set: false,
        cntv_cval_set: false,
        cntv_ctl_set: false,
        vtimer_unmasked: false,
        run_attempted: false,
        vtimer_exit_observed: false,
        pending_irq_injected: false,
        vtimer_mask_observed_after_exit: None,
        vtimer_unmasked_after_exit: false,
        watchdog_cancel_fired: false,
        vcpu_destroyed: false,
        memory_unmapped: false,
        vm_destroyed: false,
        memory_deallocated: false,
        host,
        ipa_start: 0x4000_0000,
        bytes: 16 * 1024,
        instructions: "WFI; HVC #0",
        vtimer_offset_value: 0,
        cntv_cval_value: 0,
        cntv_ctl_value: 1,
        vm_create_status: None,
        allocate_status: None,
        map_status: None,
        vcpu_create_status: None,
        pc_set_status: None,
        cpsr_set_status: None,
        vtimer_offset_set_status: None,
        cntv_cval_set_status: None,
        cntv_ctl_set_status: None,
        vtimer_unmask_status: None,
        run_status: None,
        exit_reason: None,
        exit_syndrome: None,
        exit_virtual_address: None,
        exit_physical_address: None,
        watchdog_cancel_status: None,
        pending_irq_set_status: None,
        vtimer_mask_get_after_exit_status: None,
        vtimer_unmask_after_exit_status: None,
        vcpu_destroy_status: None,
        unmap_status: None,
        vm_destroy_status: None,
        deallocate_status: None,
        blockers: vec![
            "Apple Hypervisor.framework VTimer exit probe is only available on Apple Silicon macOS"
                .to_string(),
        ],
    }
}
