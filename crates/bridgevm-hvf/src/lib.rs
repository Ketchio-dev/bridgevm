use std::{
    any::Any,
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

// Modules of the "QEMU virt contract" path (Path A). New platform code lands in
// dedicated files like these rather than growing the legacy probe monolith
// below. See docs/hvf-windows-engine-strategy.md and
// docs/hvf-windows-platform-contract-gap.md.
pub mod acpi;
pub mod checkpoint;
pub mod dtb;
pub mod fwcfg;
pub mod hda;
pub mod machine;
pub mod media;
pub mod msix;
pub mod net_nat;
pub mod nvme;
pub mod pcie;
pub mod pflash;
pub mod pl011;
pub mod pl031;
pub mod platform_virt;
pub mod ramfb;
pub mod smbios;
pub mod stage1;
pub mod tpm_ppi;
pub mod tpm_tis;
#[cfg(feature = "venus")]
pub mod venus_backend;
pub mod virtio_blk;
pub mod virtio_console;
pub mod virtio_gpu;
pub mod virtio_gpu_3d;
pub mod virtio_gpu_3d_preflight;
mod virtio_gpu_trace;
pub mod virtio_net;
mod windows_arm_xhci_hid_boot_key_probe;
pub mod xhci;

// Extracted from the legacy probe monolith below (see
// docs/hvf-lib-refactor-extraction-plan.md). These stay private and are
// re-exported explicitly so the crate-root public surface is unchanged.
mod machine_plan;
mod no_qemu_plan;
mod probe_mmio;
mod support;

// Crate-internal synthetic MMIO harness. Glob import keeps the (formerly
// file-local) names resolving unqualified; every item is pub(crate), so this
// does not widen the public API.
use probe_mmio::*;

pub use machine_plan::{
    build_windows_11_arm_hvf_machine_plan, plan_windows_11_arm_hvf_machine,
    windows_11_arm_hvf_machine_devices, HvfDevicePlan, HvfMachinePlan, HvfMachinePlanOptions,
    HvfMemoryRegionPlan, HvfVcpuLifecyclePlan,
};
pub use no_qemu_plan::{
    plan_windows_11_arm_no_qemu, windows_11_arm_no_qemu_vmm_gates, WindowsArmNoQemuPlan,
    WindowsArmVmmGate,
};
pub use support::{detect_hvf_support, HvfSupport, WindowsArmVmmGateStatus};
pub(crate) use support::{
    hv_exit_reason_name, hv_return_name, HV_EXIT_REASON_CANCELED_VALUE,
    HV_EXIT_REASON_EXCEPTION_VALUE, HV_EXIT_REASON_VTIMER_ACTIVATED_VALUE, HV_SUCCESS_VALUE,
};

pub use virtio_gpu_3d_preflight::{
    probe_virtio_gpu_3d_host_preflight, probe_virtio_gpu_3d_host_preflight_for,
    VirtioGpu3dHostPreflight, VirtioGpu3dHostPreflightProtocol,
};
pub use windows_arm_xhci_hid_boot_key_probe::{
    probe_windows_11_arm_xhci_hid_boot_key_report, WindowsArmXhciHidBootKeyReportProbe,
};

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HvfVmCreateProbe {
    pub allowed: bool,
    pub attempted: bool,
    pub created: bool,
    pub destroyed: bool,
    pub host: HvfHostCapabilities,
    pub create_status: Option<i32>,
    pub destroy_status: Option<i32>,
    pub blockers: Vec<String>,
}

impl HvfVmCreateProbe {
    pub fn render_text(&self) -> String {
        let mut output = String::new();
        output.push_str("HVF VM create/destroy probe\n");
        output.push_str("QEMU: not used\n");
        output.push_str("Apple VZ: not used\n");
        output.push_str(&format!("Host: {}\n", self.host.host));
        output.push_str(&format!("Host HVF available: {}\n", self.host.available));
        output.push_str(&format!("Allowed: {}\n", self.allowed));
        output.push_str(&format!("Attempted: {}\n", self.attempted));
        output.push_str(&format!("Created: {}\n", self.created));
        output.push_str(&format!("Destroyed: {}\n", self.destroyed));
        output.push_str(&format!(
            "Create status: {}\n",
            render_optional_status(self.create_status)
        ));
        output.push_str(&format!(
            "Create status name: {}\n",
            render_optional_status_name(self.create_status)
        ));
        output.push_str(&format!(
            "Destroy status: {}\n",
            render_optional_status(self.destroy_status)
        ));
        output.push_str(&format!(
            "Destroy status name: {}\n",
            render_optional_status_name(self.destroy_status)
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

pub fn probe_hvf_vm_create(allow_create: bool) -> HvfVmCreateProbe {
    let host = query_hvf_host_capabilities();
    platform::probe_hvf_vm_create(allow_create, host)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HvfVcpuCreateProbe {
    pub allowed: bool,
    pub attempted: bool,
    pub vm_created: bool,
    pub vcpu_created: bool,
    pub vcpu_destroyed: bool,
    pub vm_destroyed: bool,
    pub host: HvfHostCapabilities,
    pub vm_create_status: Option<i32>,
    pub vcpu_create_status: Option<i32>,
    pub vcpu_destroy_status: Option<i32>,
    pub vm_destroy_status: Option<i32>,
    pub blockers: Vec<String>,
}

impl HvfVcpuCreateProbe {
    pub fn render_text(&self) -> String {
        let mut output = String::new();
        output.push_str("HVF vCPU create/destroy probe\n");
        output.push_str("QEMU: not used\n");
        output.push_str("Apple VZ: not used\n");
        output.push_str(&format!("Host: {}\n", self.host.host));
        output.push_str(&format!("Host HVF available: {}\n", self.host.available));
        output.push_str(&format!("Allowed: {}\n", self.allowed));
        output.push_str(&format!("Attempted: {}\n", self.attempted));
        output.push_str(&format!("VM created: {}\n", self.vm_created));
        output.push_str(&format!("vCPU created: {}\n", self.vcpu_created));
        output.push_str(&format!("vCPU destroyed: {}\n", self.vcpu_destroyed));
        output.push_str(&format!("VM destroyed: {}\n", self.vm_destroyed));
        output.push_str(&format!(
            "VM create status: {}\n",
            render_optional_status(self.vm_create_status)
        ));
        output.push_str(&format!(
            "VM create status name: {}\n",
            render_optional_status_name(self.vm_create_status)
        ));
        output.push_str(&format!(
            "vCPU create status: {}\n",
            render_optional_status(self.vcpu_create_status)
        ));
        output.push_str(&format!(
            "vCPU create status name: {}\n",
            render_optional_status_name(self.vcpu_create_status)
        ));
        output.push_str(&format!(
            "vCPU destroy status: {}\n",
            render_optional_status(self.vcpu_destroy_status)
        ));
        output.push_str(&format!(
            "vCPU destroy status name: {}\n",
            render_optional_status_name(self.vcpu_destroy_status)
        ));
        output.push_str(&format!(
            "VM destroy status: {}\n",
            render_optional_status(self.vm_destroy_status)
        ));
        output.push_str(&format!(
            "VM destroy status name: {}\n",
            render_optional_status_name(self.vm_destroy_status)
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

pub fn probe_hvf_vcpu_create(allow_create: bool) -> HvfVcpuCreateProbe {
    let host = query_hvf_host_capabilities();
    platform::probe_hvf_vcpu_create(allow_create, host)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HvfVcpuRunProbe {
    pub allowed: bool,
    pub attempted: bool,
    pub vm_created: bool,
    pub vcpu_created: bool,
    pub cancel_requested: bool,
    pub run_attempted: bool,
    pub run_boundary_observed: bool,
    pub vcpu_destroyed: bool,
    pub vm_destroyed: bool,
    pub host: HvfHostCapabilities,
    pub vm_create_status: Option<i32>,
    pub vcpu_create_status: Option<i32>,
    pub cancel_status: Option<i32>,
    pub run_status: Option<i32>,
    pub exit_reason: Option<u32>,
    pub vcpu_destroy_status: Option<i32>,
    pub vm_destroy_status: Option<i32>,
    pub blockers: Vec<String>,
}

impl HvfVcpuRunProbe {
    pub fn render_text(&self) -> String {
        let mut output = String::new();
        output.push_str("HVF vCPU run/cancel probe\n");
        output.push_str("QEMU: not used\n");
        output.push_str("Apple VZ: not used\n");
        output.push_str("Guest execution: pre-canceled before entry\n");
        output.push_str(&format!("Host: {}\n", self.host.host));
        output.push_str(&format!("Host HVF available: {}\n", self.host.available));
        output.push_str(&format!("Allowed: {}\n", self.allowed));
        output.push_str(&format!("Attempted: {}\n", self.attempted));
        output.push_str(&format!("VM created: {}\n", self.vm_created));
        output.push_str(&format!("vCPU created: {}\n", self.vcpu_created));
        output.push_str(&format!("Cancel requested: {}\n", self.cancel_requested));
        output.push_str(&format!("Run attempted: {}\n", self.run_attempted));
        output.push_str(&format!(
            "Run boundary observed: {}\n",
            self.run_boundary_observed
        ));
        output.push_str(&format!("vCPU destroyed: {}\n", self.vcpu_destroyed));
        output.push_str(&format!("VM destroyed: {}\n", self.vm_destroyed));
        output.push_str(&format!(
            "VM create status: {}\n",
            render_optional_status(self.vm_create_status)
        ));
        output.push_str(&format!(
            "VM create status name: {}\n",
            render_optional_status_name(self.vm_create_status)
        ));
        output.push_str(&format!(
            "vCPU create status: {}\n",
            render_optional_status(self.vcpu_create_status)
        ));
        output.push_str(&format!(
            "vCPU create status name: {}\n",
            render_optional_status_name(self.vcpu_create_status)
        ));
        output.push_str(&format!(
            "Cancel status: {}\n",
            render_optional_status(self.cancel_status)
        ));
        output.push_str(&format!(
            "Cancel status name: {}\n",
            render_optional_status_name(self.cancel_status)
        ));
        output.push_str(&format!(
            "Run status: {}\n",
            render_optional_status(self.run_status)
        ));
        output.push_str(&format!(
            "Run status name: {}\n",
            render_optional_status_name(self.run_status)
        ));
        output.push_str(&format!(
            "Exit reason: {}\n",
            render_optional_exit_reason(self.exit_reason)
        ));
        output.push_str(&format!(
            "Exit reason name: {}\n",
            render_optional_exit_reason_name(self.exit_reason)
        ));
        output.push_str(&format!(
            "vCPU destroy status: {}\n",
            render_optional_status(self.vcpu_destroy_status)
        ));
        output.push_str(&format!(
            "vCPU destroy status name: {}\n",
            render_optional_status_name(self.vcpu_destroy_status)
        ));
        output.push_str(&format!(
            "VM destroy status: {}\n",
            render_optional_status(self.vm_destroy_status)
        ));
        output.push_str(&format!(
            "VM destroy status name: {}\n",
            render_optional_status_name(self.vm_destroy_status)
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

pub fn probe_hvf_vcpu_run(allow_run: bool) -> HvfVcpuRunProbe {
    let host = query_hvf_host_capabilities();
    platform::probe_hvf_vcpu_run(allow_run, host)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HvfInterruptTimerProbe {
    pub allowed: bool,
    pub attempted: bool,
    pub vm_created: bool,
    pub vcpu_created: bool,
    pub pending_irq_set: bool,
    pub pending_irq_cleared: bool,
    pub vtimer_masked: bool,
    pub vtimer_unmasked: bool,
    pub vtimer_offset_set: bool,
    pub boundary_observed: bool,
    pub vcpu_destroyed: bool,
    pub vm_destroyed: bool,
    pub host: HvfHostCapabilities,
    pub vtimer_offset_value: u64,
    pub vm_create_status: Option<i32>,
    pub vcpu_create_status: Option<i32>,
    pub irq_set_status: Option<i32>,
    pub irq_get_after_set_status: Option<i32>,
    pub irq_pending_after_set: Option<bool>,
    pub irq_clear_status: Option<i32>,
    pub irq_get_after_clear_status: Option<i32>,
    pub irq_pending_after_clear: Option<bool>,
    pub vtimer_mask_set_status: Option<i32>,
    pub vtimer_mask_get_status: Option<i32>,
    pub vtimer_mask_after_set: Option<bool>,
    pub vtimer_unmask_status: Option<i32>,
    pub vtimer_unmask_get_status: Option<i32>,
    pub vtimer_mask_after_clear: Option<bool>,
    pub vtimer_offset_set_status: Option<i32>,
    pub vtimer_offset_get_status: Option<i32>,
    pub vtimer_offset_after_set: Option<u64>,
    pub vcpu_destroy_status: Option<i32>,
    pub vm_destroy_status: Option<i32>,
    pub blockers: Vec<String>,
}

impl HvfInterruptTimerProbe {
    pub fn render_text(&self) -> String {
        let mut output = String::new();
        output.push_str("HVF interrupt/timer probe\n");
        output.push_str("QEMU: not used\n");
        output.push_str("Apple VZ: not used\n");
        output.push_str("Guest execution: not entered\n");
        output.push_str(&format!("Host: {}\n", self.host.host));
        output.push_str(&format!("Host HVF available: {}\n", self.host.available));
        output.push_str(&format!("Allowed: {}\n", self.allowed));
        output.push_str(&format!("Attempted: {}\n", self.attempted));
        output.push_str(&format!("VM created: {}\n", self.vm_created));
        output.push_str(&format!("vCPU created: {}\n", self.vcpu_created));
        output.push_str(&format!("Pending IRQ set: {}\n", self.pending_irq_set));
        output.push_str(&format!(
            "Pending IRQ after set: {}\n",
            self.irq_pending_after_set
                .map_or_else(|| "not observed".to_string(), |value| value.to_string())
        ));
        output.push_str(&format!(
            "Pending IRQ cleared: {}\n",
            self.pending_irq_cleared
        ));
        output.push_str(&format!(
            "Pending IRQ after clear: {}\n",
            self.irq_pending_after_clear
                .map_or_else(|| "not observed".to_string(), |value| value.to_string())
        ));
        output.push_str(&format!("VTimer masked: {}\n", self.vtimer_masked));
        output.push_str(&format!(
            "VTimer mask after set: {}\n",
            self.vtimer_mask_after_set
                .map_or_else(|| "not observed".to_string(), |value| value.to_string())
        ));
        output.push_str(&format!("VTimer unmasked: {}\n", self.vtimer_unmasked));
        output.push_str(&format!(
            "VTimer mask after clear: {}\n",
            self.vtimer_mask_after_clear
                .map_or_else(|| "not observed".to_string(), |value| value.to_string())
        ));
        output.push_str(&format!("VTimer offset set: {}\n", self.vtimer_offset_set));
        output.push_str(&format!(
            "VTimer offset requested: {:#x}\n",
            self.vtimer_offset_value
        ));
        output.push_str(&format!(
            "VTimer offset after set: {}\n",
            self.vtimer_offset_after_set
                .map_or_else(|| "not observed".to_string(), |value| format!("{value:#x}"))
        ));
        output.push_str(&format!(
            "Interrupt/timer boundary observed: {}\n",
            self.boundary_observed
        ));
        output.push_str(&format!("vCPU destroyed: {}\n", self.vcpu_destroyed));
        output.push_str(&format!("VM destroyed: {}\n", self.vm_destroyed));
        output.push_str(&format!(
            "VM create status name: {}\n",
            render_optional_status_name(self.vm_create_status)
        ));
        output.push_str(&format!(
            "vCPU create status name: {}\n",
            render_optional_status_name(self.vcpu_create_status)
        ));
        output.push_str(&format!(
            "IRQ set status name: {}\n",
            render_optional_status_name(self.irq_set_status)
        ));
        output.push_str(&format!(
            "IRQ get after set status name: {}\n",
            render_optional_status_name(self.irq_get_after_set_status)
        ));
        output.push_str(&format!(
            "IRQ clear status name: {}\n",
            render_optional_status_name(self.irq_clear_status)
        ));
        output.push_str(&format!(
            "IRQ get after clear status name: {}\n",
            render_optional_status_name(self.irq_get_after_clear_status)
        ));
        output.push_str(&format!(
            "VTimer mask set status name: {}\n",
            render_optional_status_name(self.vtimer_mask_set_status)
        ));
        output.push_str(&format!(
            "VTimer mask get status name: {}\n",
            render_optional_status_name(self.vtimer_mask_get_status)
        ));
        output.push_str(&format!(
            "VTimer unmask status name: {}\n",
            render_optional_status_name(self.vtimer_unmask_status)
        ));
        output.push_str(&format!(
            "VTimer unmask get status name: {}\n",
            render_optional_status_name(self.vtimer_unmask_get_status)
        ));
        output.push_str(&format!(
            "VTimer offset set status name: {}\n",
            render_optional_status_name(self.vtimer_offset_set_status)
        ));
        output.push_str(&format!(
            "VTimer offset get status name: {}\n",
            render_optional_status_name(self.vtimer_offset_get_status)
        ));
        output.push_str(&format!(
            "vCPU destroy status name: {}\n",
            render_optional_status_name(self.vcpu_destroy_status)
        ));
        output.push_str(&format!(
            "VM destroy status name: {}\n",
            render_optional_status_name(self.vm_destroy_status)
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

pub fn probe_hvf_interrupt_timer(allow_probe: bool) -> HvfInterruptTimerProbe {
    let host = query_hvf_host_capabilities();
    platform::probe_hvf_interrupt_timer(allow_probe, host)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HvfVtimerExitProbe {
    pub allowed: bool,
    pub attempted: bool,
    pub vm_created: bool,
    pub memory_allocated: bool,
    pub memory_mapped: bool,
    pub vcpu_created: bool,
    pub pc_set: bool,
    pub cpsr_set: bool,
    pub vtimer_offset_set: bool,
    pub cntv_cval_set: bool,
    pub cntv_ctl_set: bool,
    pub vtimer_unmasked: bool,
    pub run_attempted: bool,
    pub vtimer_exit_observed: bool,
    pub pending_irq_injected: bool,
    pub vtimer_mask_observed_after_exit: Option<bool>,
    pub vtimer_unmasked_after_exit: bool,
    pub watchdog_cancel_fired: bool,
    pub vcpu_destroyed: bool,
    pub memory_unmapped: bool,
    pub vm_destroyed: bool,
    pub memory_deallocated: bool,
    pub host: HvfHostCapabilities,
    pub ipa_start: u64,
    pub bytes: usize,
    pub instructions: &'static str,
    pub vtimer_offset_value: u64,
    pub cntv_cval_value: u64,
    pub cntv_ctl_value: u64,
    pub vm_create_status: Option<i32>,
    pub allocate_status: Option<i32>,
    pub map_status: Option<i32>,
    pub vcpu_create_status: Option<i32>,
    pub pc_set_status: Option<i32>,
    pub cpsr_set_status: Option<i32>,
    pub vtimer_offset_set_status: Option<i32>,
    pub cntv_cval_set_status: Option<i32>,
    pub cntv_ctl_set_status: Option<i32>,
    pub vtimer_unmask_status: Option<i32>,
    pub run_status: Option<i32>,
    pub exit_reason: Option<u32>,
    pub exit_syndrome: Option<u64>,
    pub exit_virtual_address: Option<u64>,
    pub exit_physical_address: Option<u64>,
    pub watchdog_cancel_status: Option<i32>,
    pub pending_irq_set_status: Option<i32>,
    pub vtimer_mask_get_after_exit_status: Option<i32>,
    pub vtimer_unmask_after_exit_status: Option<i32>,
    pub vcpu_destroy_status: Option<i32>,
    pub unmap_status: Option<i32>,
    pub vm_destroy_status: Option<i32>,
    pub deallocate_status: Option<i32>,
    pub blockers: Vec<String>,
}

impl HvfVtimerExitProbe {
    pub fn render_text(&self) -> String {
        let mut output = String::new();
        output.push_str("HVF VTimer exit probe\n");
        output.push_str("QEMU: not used\n");
        output.push_str("Apple VZ: not used\n");
        output.push_str("Guest execution: WFI wait loop with host-programmed virtual timer\n");
        output.push_str(&format!("Host: {}\n", self.host.host));
        output.push_str(&format!("Host HVF available: {}\n", self.host.available));
        output.push_str(&format!("Allowed: {}\n", self.allowed));
        output.push_str(&format!("Attempted: {}\n", self.attempted));
        output.push_str(&format!("VM created: {}\n", self.vm_created));
        output.push_str(&format!("Memory allocated: {}\n", self.memory_allocated));
        output.push_str(&format!("Memory mapped: {}\n", self.memory_mapped));
        output.push_str(&format!("vCPU created: {}\n", self.vcpu_created));
        output.push_str(&format!("PC set: {}\n", self.pc_set));
        output.push_str(&format!("CPSR set: {}\n", self.cpsr_set));
        output.push_str(&format!("VTimer offset set: {}\n", self.vtimer_offset_set));
        output.push_str(&format!("CNTV_CVAL_EL0 set: {}\n", self.cntv_cval_set));
        output.push_str(&format!("CNTV_CTL_EL0 set: {}\n", self.cntv_ctl_set));
        output.push_str(&format!("VTimer unmasked: {}\n", self.vtimer_unmasked));
        output.push_str(&format!("Run attempted: {}\n", self.run_attempted));
        output.push_str(&format!(
            "VTimer exit observed: {}\n",
            self.vtimer_exit_observed
        ));
        output.push_str(&format!(
            "Pending IRQ injected: {}\n",
            self.pending_irq_injected
        ));
        output.push_str(&format!(
            "VTimer mask observed after exit: {}\n",
            render_optional_bool(self.vtimer_mask_observed_after_exit)
        ));
        output.push_str(&format!(
            "VTimer unmasked after exit: {}\n",
            self.vtimer_unmasked_after_exit
        ));
        output.push_str(&format!(
            "Watchdog cancel fired: {}\n",
            self.watchdog_cancel_fired
        ));
        output.push_str(&format!("vCPU destroyed: {}\n", self.vcpu_destroyed));
        output.push_str(&format!("Memory unmapped: {}\n", self.memory_unmapped));
        output.push_str(&format!("VM destroyed: {}\n", self.vm_destroyed));
        output.push_str(&format!(
            "Memory deallocated: {}\n",
            self.memory_deallocated
        ));
        output.push_str(&format!("IPA start: {:#x}\n", self.ipa_start));
        output.push_str(&format!("Bytes: {:#x}\n", self.bytes));
        output.push_str(&format!("Instructions: {}\n", self.instructions));
        output.push_str(&format!(
            "VTimer offset requested: {:#x}\n",
            self.vtimer_offset_value
        ));
        output.push_str(&format!(
            "CNTV_CVAL_EL0 requested: {:#x}\n",
            self.cntv_cval_value
        ));
        output.push_str(&format!(
            "CNTV_CTL_EL0 requested: {:#x}\n",
            self.cntv_ctl_value
        ));
        output.push_str(&format!(
            "Run status name: {}\n",
            render_optional_status_name(self.run_status)
        ));
        output.push_str(&format!(
            "Exit reason name: {}\n",
            render_optional_exit_reason_name(self.exit_reason)
        ));
        output.push_str(&format!(
            "Exit syndrome: {}\n",
            render_optional_u64(self.exit_syndrome)
        ));
        output.push_str(&format!(
            "Exit virtual address: {}\n",
            render_optional_u64(self.exit_virtual_address)
        ));
        output.push_str(&format!(
            "Exit physical address: {}\n",
            render_optional_u64(self.exit_physical_address)
        ));
        output.push_str(&format!(
            "VM create status name: {}\n",
            render_optional_status_name(self.vm_create_status)
        ));
        output.push_str(&format!(
            "Allocate status name: {}\n",
            render_optional_status_name(self.allocate_status)
        ));
        output.push_str(&format!(
            "Map status name: {}\n",
            render_optional_status_name(self.map_status)
        ));
        output.push_str(&format!(
            "vCPU create status name: {}\n",
            render_optional_status_name(self.vcpu_create_status)
        ));
        output.push_str(&format!(
            "PC set status name: {}\n",
            render_optional_status_name(self.pc_set_status)
        ));
        output.push_str(&format!(
            "CPSR set status name: {}\n",
            render_optional_status_name(self.cpsr_set_status)
        ));
        output.push_str(&format!(
            "VTimer offset set status name: {}\n",
            render_optional_status_name(self.vtimer_offset_set_status)
        ));
        output.push_str(&format!(
            "CNTV_CVAL_EL0 set status name: {}\n",
            render_optional_status_name(self.cntv_cval_set_status)
        ));
        output.push_str(&format!(
            "CNTV_CTL_EL0 set status name: {}\n",
            render_optional_status_name(self.cntv_ctl_set_status)
        ));
        output.push_str(&format!(
            "VTimer unmask status name: {}\n",
            render_optional_status_name(self.vtimer_unmask_status)
        ));
        output.push_str(&format!(
            "Watchdog cancel status name: {}\n",
            render_optional_status_name(self.watchdog_cancel_status)
        ));
        output.push_str(&format!(
            "Pending IRQ set status name: {}\n",
            render_optional_status_name(self.pending_irq_set_status)
        ));
        output.push_str(&format!(
            "VTimer mask get after exit status name: {}\n",
            render_optional_status_name(self.vtimer_mask_get_after_exit_status)
        ));
        output.push_str(&format!(
            "VTimer unmask after exit status name: {}\n",
            render_optional_status_name(self.vtimer_unmask_after_exit_status)
        ));
        output.push_str(&format!(
            "vCPU destroy status name: {}\n",
            render_optional_status_name(self.vcpu_destroy_status)
        ));
        output.push_str(&format!(
            "Unmap status name: {}\n",
            render_optional_status_name(self.unmap_status)
        ));
        output.push_str(&format!(
            "VM destroy status name: {}\n",
            render_optional_status_name(self.vm_destroy_status)
        ));
        output.push_str(&format!(
            "Deallocate status name: {}\n",
            render_optional_status_name(self.deallocate_status)
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

pub fn probe_hvf_vtimer_exit(allow_probe: bool) -> HvfVtimerExitProbe {
    let host = query_hvf_host_capabilities();
    platform::probe_hvf_vtimer_exit(allow_probe, host)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HvfMemoryMapProbe {
    pub allowed: bool,
    pub attempted: bool,
    pub vm_created: bool,
    pub memory_allocated: bool,
    pub memory_mapped: bool,
    pub memory_unmapped: bool,
    pub memory_deallocated: bool,
    pub vm_destroyed: bool,
    pub host: HvfHostCapabilities,
    pub ipa_start: u64,
    pub bytes: usize,
    pub vm_create_status: Option<i32>,
    pub allocate_status: Option<i32>,
    pub map_status: Option<i32>,
    pub unmap_status: Option<i32>,
    pub deallocate_status: Option<i32>,
    pub vm_destroy_status: Option<i32>,
    pub blockers: Vec<String>,
}

impl HvfMemoryMapProbe {
    pub fn render_text(&self) -> String {
        let mut output = String::new();
        output.push_str("HVF memory map/unmap probe\n");
        output.push_str("QEMU: not used\n");
        output.push_str("Apple VZ: not used\n");
        output.push_str("Guest execution: not entered\n");
        output.push_str(&format!("Host: {}\n", self.host.host));
        output.push_str(&format!("Host HVF available: {}\n", self.host.available));
        output.push_str(&format!("Allowed: {}\n", self.allowed));
        output.push_str(&format!("Attempted: {}\n", self.attempted));
        output.push_str(&format!("VM created: {}\n", self.vm_created));
        output.push_str(&format!("Memory allocated: {}\n", self.memory_allocated));
        output.push_str(&format!("Memory mapped: {}\n", self.memory_mapped));
        output.push_str(&format!("Memory unmapped: {}\n", self.memory_unmapped));
        output.push_str(&format!(
            "Memory deallocated: {}\n",
            self.memory_deallocated
        ));
        output.push_str(&format!("VM destroyed: {}\n", self.vm_destroyed));
        output.push_str(&format!("Guest IPA start: {:#x}\n", self.ipa_start));
        output.push_str(&format!("Bytes: {}\n", self.bytes));
        output.push_str(&format!(
            "VM create status: {}\n",
            render_optional_status(self.vm_create_status)
        ));
        output.push_str(&format!(
            "VM create status name: {}\n",
            render_optional_status_name(self.vm_create_status)
        ));
        output.push_str(&format!(
            "Allocate status: {}\n",
            render_optional_status(self.allocate_status)
        ));
        output.push_str(&format!(
            "Allocate status name: {}\n",
            render_optional_status_name(self.allocate_status)
        ));
        output.push_str(&format!(
            "Map status: {}\n",
            render_optional_status(self.map_status)
        ));
        output.push_str(&format!(
            "Map status name: {}\n",
            render_optional_status_name(self.map_status)
        ));
        output.push_str(&format!(
            "Unmap status: {}\n",
            render_optional_status(self.unmap_status)
        ));
        output.push_str(&format!(
            "Unmap status name: {}\n",
            render_optional_status_name(self.unmap_status)
        ));
        output.push_str(&format!(
            "Deallocate status: {}\n",
            render_optional_status(self.deallocate_status)
        ));
        output.push_str(&format!(
            "Deallocate status name: {}\n",
            render_optional_status_name(self.deallocate_status)
        ));
        output.push_str(&format!(
            "VM destroy status: {}\n",
            render_optional_status(self.vm_destroy_status)
        ));
        output.push_str(&format!(
            "VM destroy status name: {}\n",
            render_optional_status_name(self.vm_destroy_status)
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

pub fn probe_hvf_memory_map(allow_map: bool) -> HvfMemoryMapProbe {
    let host = query_hvf_host_capabilities();
    platform::probe_hvf_memory_map(allow_map, host)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HvfGuestEntryProbe {
    pub allowed: bool,
    pub attempted: bool,
    pub vm_created: bool,
    pub memory_allocated: bool,
    pub memory_mapped: bool,
    pub vcpu_created: bool,
    pub pc_set: bool,
    pub cpsr_set: bool,
    pub run_attempted: bool,
    pub entry_boundary_observed: bool,
    pub watchdog_cancel_fired: bool,
    pub vcpu_destroyed: bool,
    pub memory_unmapped: bool,
    pub vm_destroyed: bool,
    pub memory_deallocated: bool,
    pub host: HvfHostCapabilities,
    pub ipa_start: u64,
    pub bytes: usize,
    pub instruction: &'static str,
    pub vm_create_status: Option<i32>,
    pub allocate_status: Option<i32>,
    pub map_status: Option<i32>,
    pub vcpu_create_status: Option<i32>,
    pub pc_set_status: Option<i32>,
    pub cpsr_set_status: Option<i32>,
    pub run_status: Option<i32>,
    pub exit_reason: Option<u32>,
    pub exit_syndrome: Option<u64>,
    pub exit_virtual_address: Option<u64>,
    pub exit_physical_address: Option<u64>,
    pub watchdog_cancel_status: Option<i32>,
    pub vcpu_destroy_status: Option<i32>,
    pub unmap_status: Option<i32>,
    pub vm_destroy_status: Option<i32>,
    pub deallocate_status: Option<i32>,
    pub blockers: Vec<String>,
}

impl HvfGuestEntryProbe {
    pub fn render_text(&self) -> String {
        let mut output = String::new();
        output.push_str("HVF guest entry probe\n");
        output.push_str("QEMU: not used\n");
        output.push_str("Apple VZ: not used\n");
        output.push_str("Guest execution: one HVC instruction with watchdog\n");
        output.push_str(&format!("Host: {}\n", self.host.host));
        output.push_str(&format!("Host HVF available: {}\n", self.host.available));
        output.push_str(&format!("Allowed: {}\n", self.allowed));
        output.push_str(&format!("Attempted: {}\n", self.attempted));
        output.push_str(&format!("VM created: {}\n", self.vm_created));
        output.push_str(&format!("Memory allocated: {}\n", self.memory_allocated));
        output.push_str(&format!("Memory mapped: {}\n", self.memory_mapped));
        output.push_str(&format!("vCPU created: {}\n", self.vcpu_created));
        output.push_str(&format!("PC set: {}\n", self.pc_set));
        output.push_str(&format!("CPSR set: {}\n", self.cpsr_set));
        output.push_str(&format!("Run attempted: {}\n", self.run_attempted));
        output.push_str(&format!(
            "Entry boundary observed: {}\n",
            self.entry_boundary_observed
        ));
        output.push_str(&format!(
            "Watchdog cancel fired: {}\n",
            self.watchdog_cancel_fired
        ));
        output.push_str(&format!("vCPU destroyed: {}\n", self.vcpu_destroyed));
        output.push_str(&format!("Memory unmapped: {}\n", self.memory_unmapped));
        output.push_str(&format!("VM destroyed: {}\n", self.vm_destroyed));
        output.push_str(&format!(
            "Memory deallocated: {}\n",
            self.memory_deallocated
        ));
        output.push_str(&format!("Guest IPA start: {:#x}\n", self.ipa_start));
        output.push_str(&format!("Bytes: {}\n", self.bytes));
        output.push_str(&format!("Instruction: {}\n", self.instruction));
        output.push_str(&format!(
            "VM create status name: {}\n",
            render_optional_status_name(self.vm_create_status)
        ));
        output.push_str(&format!(
            "Allocate status name: {}\n",
            render_optional_status_name(self.allocate_status)
        ));
        output.push_str(&format!(
            "Map status name: {}\n",
            render_optional_status_name(self.map_status)
        ));
        output.push_str(&format!(
            "vCPU create status name: {}\n",
            render_optional_status_name(self.vcpu_create_status)
        ));
        output.push_str(&format!(
            "PC set status name: {}\n",
            render_optional_status_name(self.pc_set_status)
        ));
        output.push_str(&format!(
            "CPSR set status name: {}\n",
            render_optional_status_name(self.cpsr_set_status)
        ));
        output.push_str(&format!(
            "Run status name: {}\n",
            render_optional_status_name(self.run_status)
        ));
        output.push_str(&format!(
            "Exit reason name: {}\n",
            render_optional_exit_reason_name(self.exit_reason)
        ));
        output.push_str(&format!(
            "Exit syndrome: {}\n",
            render_optional_u64(self.exit_syndrome)
        ));
        output.push_str(&format!(
            "Exit virtual address: {}\n",
            render_optional_u64(self.exit_virtual_address)
        ));
        output.push_str(&format!(
            "Exit physical address: {}\n",
            render_optional_u64(self.exit_physical_address)
        ));
        output.push_str(&format!(
            "Watchdog cancel status name: {}\n",
            render_optional_status_name(self.watchdog_cancel_status)
        ));
        output.push_str(&format!(
            "vCPU destroy status name: {}\n",
            render_optional_status_name(self.vcpu_destroy_status)
        ));
        output.push_str(&format!(
            "Unmap status name: {}\n",
            render_optional_status_name(self.unmap_status)
        ));
        output.push_str(&format!(
            "VM destroy status name: {}\n",
            render_optional_status_name(self.vm_destroy_status)
        ));
        output.push_str(&format!(
            "Deallocate status name: {}\n",
            render_optional_status_name(self.deallocate_status)
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

pub fn probe_hvf_guest_entry(allow_entry: bool) -> HvfGuestEntryProbe {
    let host = query_hvf_host_capabilities();
    platform::probe_hvf_guest_entry(allow_entry, host)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HvfGuestExitLoopProbe {
    pub allowed: bool,
    pub attempted: bool,
    pub vm_created: bool,
    pub memory_allocated: bool,
    pub memory_mapped: bool,
    pub vcpu_created: bool,
    pub initial_pc_set: bool,
    pub cpsr_set: bool,
    pub first_run_attempted: bool,
    pub first_exit_observed: bool,
    pub pc_read_after_first_exit: bool,
    pub pc_advanced: bool,
    pub second_run_attempted: bool,
    pub second_exit_observed: bool,
    pub exit_loop_observed: bool,
    pub watchdog_cancel_fired: bool,
    pub vcpu_destroyed: bool,
    pub memory_unmapped: bool,
    pub vm_destroyed: bool,
    pub memory_deallocated: bool,
    pub host: HvfHostCapabilities,
    pub ipa_start: u64,
    pub bytes: usize,
    pub instructions: &'static str,
    pub vm_create_status: Option<i32>,
    pub allocate_status: Option<i32>,
    pub map_status: Option<i32>,
    pub vcpu_create_status: Option<i32>,
    pub initial_pc_set_status: Option<i32>,
    pub cpsr_set_status: Option<i32>,
    pub first_run_status: Option<i32>,
    pub first_exit_reason: Option<u32>,
    pub first_exit_syndrome: Option<u64>,
    pub first_exit_virtual_address: Option<u64>,
    pub first_exit_physical_address: Option<u64>,
    pub first_watchdog_cancel_status: Option<i32>,
    pub pc_read_status: Option<i32>,
    pub pc_after_first_exit: Option<u64>,
    pub pc_advance_status: Option<i32>,
    pub second_run_status: Option<i32>,
    pub second_exit_reason: Option<u32>,
    pub second_exit_syndrome: Option<u64>,
    pub second_exit_virtual_address: Option<u64>,
    pub second_exit_physical_address: Option<u64>,
    pub second_watchdog_cancel_status: Option<i32>,
    pub vcpu_destroy_status: Option<i32>,
    pub unmap_status: Option<i32>,
    pub vm_destroy_status: Option<i32>,
    pub deallocate_status: Option<i32>,
    pub blockers: Vec<String>,
}

impl HvfGuestExitLoopProbe {
    pub fn render_text(&self) -> String {
        let mut output = String::new();
        output.push_str("HVF guest exit loop probe\n");
        output.push_str("QEMU: not used\n");
        output.push_str("Apple VZ: not used\n");
        output.push_str("Guest execution: two HVC instructions with PC advance watchdog\n");
        output.push_str(&format!("Host: {}\n", self.host.host));
        output.push_str(&format!("Host HVF available: {}\n", self.host.available));
        output.push_str(&format!("Allowed: {}\n", self.allowed));
        output.push_str(&format!("Attempted: {}\n", self.attempted));
        output.push_str(&format!("VM created: {}\n", self.vm_created));
        output.push_str(&format!("Memory allocated: {}\n", self.memory_allocated));
        output.push_str(&format!("Memory mapped: {}\n", self.memory_mapped));
        output.push_str(&format!("vCPU created: {}\n", self.vcpu_created));
        output.push_str(&format!("Initial PC set: {}\n", self.initial_pc_set));
        output.push_str(&format!("CPSR set: {}\n", self.cpsr_set));
        output.push_str(&format!(
            "First run attempted: {}\n",
            self.first_run_attempted
        ));
        output.push_str(&format!(
            "First exit observed: {}\n",
            self.first_exit_observed
        ));
        output.push_str(&format!(
            "PC read after first exit: {}\n",
            self.pc_read_after_first_exit
        ));
        output.push_str(&format!("PC advanced: {}\n", self.pc_advanced));
        output.push_str(&format!(
            "Second run attempted: {}\n",
            self.second_run_attempted
        ));
        output.push_str(&format!(
            "Second exit observed: {}\n",
            self.second_exit_observed
        ));
        output.push_str(&format!(
            "Exit loop observed: {}\n",
            self.exit_loop_observed
        ));
        output.push_str(&format!(
            "Watchdog cancel fired: {}\n",
            self.watchdog_cancel_fired
        ));
        output.push_str(&format!("vCPU destroyed: {}\n", self.vcpu_destroyed));
        output.push_str(&format!("Memory unmapped: {}\n", self.memory_unmapped));
        output.push_str(&format!("VM destroyed: {}\n", self.vm_destroyed));
        output.push_str(&format!(
            "Memory deallocated: {}\n",
            self.memory_deallocated
        ));
        output.push_str(&format!("Guest IPA start: {:#x}\n", self.ipa_start));
        output.push_str(&format!("Bytes: {}\n", self.bytes));
        output.push_str(&format!("Instructions: {}\n", self.instructions));
        output.push_str(&format!(
            "VM create status name: {}\n",
            render_optional_status_name(self.vm_create_status)
        ));
        output.push_str(&format!(
            "Allocate status name: {}\n",
            render_optional_status_name(self.allocate_status)
        ));
        output.push_str(&format!(
            "Map status name: {}\n",
            render_optional_status_name(self.map_status)
        ));
        output.push_str(&format!(
            "vCPU create status name: {}\n",
            render_optional_status_name(self.vcpu_create_status)
        ));
        output.push_str(&format!(
            "Initial PC set status name: {}\n",
            render_optional_status_name(self.initial_pc_set_status)
        ));
        output.push_str(&format!(
            "CPSR set status name: {}\n",
            render_optional_status_name(self.cpsr_set_status)
        ));
        output.push_str(&format!(
            "First run status name: {}\n",
            render_optional_status_name(self.first_run_status)
        ));
        output.push_str(&format!(
            "First exit reason name: {}\n",
            render_optional_exit_reason_name(self.first_exit_reason)
        ));
        output.push_str(&format!(
            "First exit syndrome: {}\n",
            render_optional_u64(self.first_exit_syndrome)
        ));
        output.push_str(&format!(
            "First exit virtual address: {}\n",
            render_optional_u64(self.first_exit_virtual_address)
        ));
        output.push_str(&format!(
            "First exit physical address: {}\n",
            render_optional_u64(self.first_exit_physical_address)
        ));
        output.push_str(&format!(
            "First watchdog cancel status name: {}\n",
            render_optional_status_name(self.first_watchdog_cancel_status)
        ));
        output.push_str(&format!(
            "PC read status name: {}\n",
            render_optional_status_name(self.pc_read_status)
        ));
        output.push_str(&format!(
            "PC after first exit: {}\n",
            render_optional_u64(self.pc_after_first_exit)
        ));
        output.push_str(&format!(
            "PC advance status name: {}\n",
            render_optional_status_name(self.pc_advance_status)
        ));
        output.push_str(&format!(
            "Second run status name: {}\n",
            render_optional_status_name(self.second_run_status)
        ));
        output.push_str(&format!(
            "Second exit reason name: {}\n",
            render_optional_exit_reason_name(self.second_exit_reason)
        ));
        output.push_str(&format!(
            "Second exit syndrome: {}\n",
            render_optional_u64(self.second_exit_syndrome)
        ));
        output.push_str(&format!(
            "Second exit virtual address: {}\n",
            render_optional_u64(self.second_exit_virtual_address)
        ));
        output.push_str(&format!(
            "Second exit physical address: {}\n",
            render_optional_u64(self.second_exit_physical_address)
        ));
        output.push_str(&format!(
            "Second watchdog cancel status name: {}\n",
            render_optional_status_name(self.second_watchdog_cancel_status)
        ));
        output.push_str(&format!(
            "vCPU destroy status name: {}\n",
            render_optional_status_name(self.vcpu_destroy_status)
        ));
        output.push_str(&format!(
            "Unmap status name: {}\n",
            render_optional_status_name(self.unmap_status)
        ));
        output.push_str(&format!(
            "VM destroy status name: {}\n",
            render_optional_status_name(self.vm_destroy_status)
        ));
        output.push_str(&format!(
            "Deallocate status name: {}\n",
            render_optional_status_name(self.deallocate_status)
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

pub fn probe_hvf_guest_exit_loop(allow_loop: bool) -> HvfGuestExitLoopProbe {
    let host = query_hvf_host_capabilities();
    platform::probe_hvf_guest_exit_loop(allow_loop, host)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HvfMmioReadExitProbe {
    pub allowed: bool,
    pub attempted: bool,
    pub vm_created: bool,
    pub memory_allocated: bool,
    pub memory_mapped: bool,
    pub vcpu_created: bool,
    pub pc_set: bool,
    pub cpsr_set: bool,
    pub address_register_set: bool,
    pub run_attempted: bool,
    pub mmio_exit_observed: bool,
    pub watchdog_cancel_fired: bool,
    pub vcpu_destroyed: bool,
    pub memory_unmapped: bool,
    pub vm_destroyed: bool,
    pub memory_deallocated: bool,
    pub host: HvfHostCapabilities,
    pub code_ipa_start: u64,
    pub mmio_ipa: u64,
    pub bytes: usize,
    pub instruction: &'static str,
    pub vm_create_status: Option<i32>,
    pub allocate_status: Option<i32>,
    pub map_status: Option<i32>,
    pub vcpu_create_status: Option<i32>,
    pub pc_set_status: Option<i32>,
    pub cpsr_set_status: Option<i32>,
    pub address_register_set_status: Option<i32>,
    pub run_status: Option<i32>,
    pub exit_reason: Option<u32>,
    pub exit_syndrome: Option<u64>,
    pub exit_virtual_address: Option<u64>,
    pub exit_physical_address: Option<u64>,
    pub watchdog_cancel_status: Option<i32>,
    pub vcpu_destroy_status: Option<i32>,
    pub unmap_status: Option<i32>,
    pub vm_destroy_status: Option<i32>,
    pub deallocate_status: Option<i32>,
    pub blockers: Vec<String>,
}

impl HvfMmioReadExitProbe {
    pub fn render_text(&self) -> String {
        let mut output = String::new();
        output.push_str("HVF MMIO read exit probe\n");
        output.push_str("QEMU: not used\n");
        output.push_str("Apple VZ: not used\n");
        output.push_str("Guest execution: one unmapped LDR read with watchdog\n");
        output.push_str(&format!("Host: {}\n", self.host.host));
        output.push_str(&format!("Host HVF available: {}\n", self.host.available));
        output.push_str(&format!("Allowed: {}\n", self.allowed));
        output.push_str(&format!("Attempted: {}\n", self.attempted));
        output.push_str(&format!("VM created: {}\n", self.vm_created));
        output.push_str(&format!("Memory allocated: {}\n", self.memory_allocated));
        output.push_str(&format!("Memory mapped: {}\n", self.memory_mapped));
        output.push_str(&format!("vCPU created: {}\n", self.vcpu_created));
        output.push_str(&format!("PC set: {}\n", self.pc_set));
        output.push_str(&format!("CPSR set: {}\n", self.cpsr_set));
        output.push_str(&format!(
            "Address register set: {}\n",
            self.address_register_set
        ));
        output.push_str(&format!("Run attempted: {}\n", self.run_attempted));
        output.push_str(&format!(
            "MMIO exit observed: {}\n",
            self.mmio_exit_observed
        ));
        output.push_str(&format!(
            "Watchdog cancel fired: {}\n",
            self.watchdog_cancel_fired
        ));
        output.push_str(&format!("vCPU destroyed: {}\n", self.vcpu_destroyed));
        output.push_str(&format!("Memory unmapped: {}\n", self.memory_unmapped));
        output.push_str(&format!("VM destroyed: {}\n", self.vm_destroyed));
        output.push_str(&format!(
            "Memory deallocated: {}\n",
            self.memory_deallocated
        ));
        output.push_str(&format!("Code IPA start: {:#x}\n", self.code_ipa_start));
        output.push_str(&format!("MMIO IPA: {:#x}\n", self.mmio_ipa));
        output.push_str(&format!("Bytes: {}\n", self.bytes));
        output.push_str(&format!("Instruction: {}\n", self.instruction));
        output.push_str(&format!(
            "VM create status name: {}\n",
            render_optional_status_name(self.vm_create_status)
        ));
        output.push_str(&format!(
            "Allocate status name: {}\n",
            render_optional_status_name(self.allocate_status)
        ));
        output.push_str(&format!(
            "Map status name: {}\n",
            render_optional_status_name(self.map_status)
        ));
        output.push_str(&format!(
            "vCPU create status name: {}\n",
            render_optional_status_name(self.vcpu_create_status)
        ));
        output.push_str(&format!(
            "PC set status name: {}\n",
            render_optional_status_name(self.pc_set_status)
        ));
        output.push_str(&format!(
            "CPSR set status name: {}\n",
            render_optional_status_name(self.cpsr_set_status)
        ));
        output.push_str(&format!(
            "Address register set status name: {}\n",
            render_optional_status_name(self.address_register_set_status)
        ));
        output.push_str(&format!(
            "Run status name: {}\n",
            render_optional_status_name(self.run_status)
        ));
        output.push_str(&format!(
            "Exit reason name: {}\n",
            render_optional_exit_reason_name(self.exit_reason)
        ));
        output.push_str(&format!(
            "Exit syndrome: {}\n",
            render_optional_u64(self.exit_syndrome)
        ));
        output.push_str(&format!(
            "Exit virtual address: {}\n",
            render_optional_u64(self.exit_virtual_address)
        ));
        output.push_str(&format!(
            "Exit physical address: {}\n",
            render_optional_u64(self.exit_physical_address)
        ));
        output.push_str(&format!(
            "Watchdog cancel status name: {}\n",
            render_optional_status_name(self.watchdog_cancel_status)
        ));
        output.push_str(&format!(
            "vCPU destroy status name: {}\n",
            render_optional_status_name(self.vcpu_destroy_status)
        ));
        output.push_str(&format!(
            "Unmap status name: {}\n",
            render_optional_status_name(self.unmap_status)
        ));
        output.push_str(&format!(
            "VM destroy status name: {}\n",
            render_optional_status_name(self.vm_destroy_status)
        ));
        output.push_str(&format!(
            "Deallocate status name: {}\n",
            render_optional_status_name(self.deallocate_status)
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

pub fn probe_hvf_mmio_read_exit(allow_mmio: bool) -> HvfMmioReadExitProbe {
    let host = query_hvf_host_capabilities();
    platform::probe_hvf_mmio_read_exit(allow_mmio, host)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HvfMmioReadEmulationProbe {
    pub allowed: bool,
    pub attempted: bool,
    pub vm_created: bool,
    pub memory_allocated: bool,
    pub memory_mapped: bool,
    pub vcpu_created: bool,
    pub pc_set: bool,
    pub cpsr_set: bool,
    pub address_register_set: bool,
    pub first_run_attempted: bool,
    pub mmio_exit_observed: bool,
    pub pc_read_after_mmio_exit: bool,
    pub emulated_value_injected: bool,
    pub pc_advanced: bool,
    pub second_run_attempted: bool,
    pub continuation_exit_observed: bool,
    pub emulated_value_preserved: bool,
    pub watchdog_cancel_fired: bool,
    pub vcpu_destroyed: bool,
    pub memory_unmapped: bool,
    pub vm_destroyed: bool,
    pub memory_deallocated: bool,
    pub host: HvfHostCapabilities,
    pub code_ipa_start: u64,
    pub mmio_ipa: u64,
    pub bytes: usize,
    pub instructions: &'static str,
    pub emulated_value: u64,
    pub vm_create_status: Option<i32>,
    pub allocate_status: Option<i32>,
    pub map_status: Option<i32>,
    pub vcpu_create_status: Option<i32>,
    pub pc_set_status: Option<i32>,
    pub cpsr_set_status: Option<i32>,
    pub address_register_set_status: Option<i32>,
    pub first_run_status: Option<i32>,
    pub mmio_exit_reason: Option<u32>,
    pub mmio_exit_syndrome: Option<u64>,
    pub mmio_exit_virtual_address: Option<u64>,
    pub mmio_exit_physical_address: Option<u64>,
    pub first_watchdog_cancel_status: Option<i32>,
    pub pc_read_status: Option<i32>,
    pub pc_after_mmio_exit: Option<u64>,
    pub emulated_value_set_status: Option<i32>,
    pub pc_advance_status: Option<i32>,
    pub second_run_status: Option<i32>,
    pub continuation_exit_reason: Option<u32>,
    pub continuation_exit_syndrome: Option<u64>,
    pub continuation_exit_virtual_address: Option<u64>,
    pub continuation_exit_physical_address: Option<u64>,
    pub second_watchdog_cancel_status: Option<i32>,
    pub emulated_value_read_status: Option<i32>,
    pub emulated_value_after_continue: Option<u64>,
    pub vcpu_destroy_status: Option<i32>,
    pub unmap_status: Option<i32>,
    pub vm_destroy_status: Option<i32>,
    pub deallocate_status: Option<i32>,
    pub blockers: Vec<String>,
}

impl HvfMmioReadEmulationProbe {
    pub fn render_text(&self) -> String {
        let mut output = String::new();
        output.push_str("HVF MMIO read emulation probe\n");
        output.push_str("QEMU: not used\n");
        output.push_str("Apple VZ: not used\n");
        output.push_str("Guest execution: unmapped LDR, injected read value, then HVC\n");
        output.push_str(&format!("Host: {}\n", self.host.host));
        output.push_str(&format!("Host HVF available: {}\n", self.host.available));
        output.push_str(&format!("Allowed: {}\n", self.allowed));
        output.push_str(&format!("Attempted: {}\n", self.attempted));
        output.push_str(&format!("VM created: {}\n", self.vm_created));
        output.push_str(&format!("Memory allocated: {}\n", self.memory_allocated));
        output.push_str(&format!("Memory mapped: {}\n", self.memory_mapped));
        output.push_str(&format!("vCPU created: {}\n", self.vcpu_created));
        output.push_str(&format!("PC set: {}\n", self.pc_set));
        output.push_str(&format!("CPSR set: {}\n", self.cpsr_set));
        output.push_str(&format!(
            "Address register set: {}\n",
            self.address_register_set
        ));
        output.push_str(&format!(
            "First run attempted: {}\n",
            self.first_run_attempted
        ));
        output.push_str(&format!(
            "MMIO exit observed: {}\n",
            self.mmio_exit_observed
        ));
        output.push_str(&format!(
            "PC read after MMIO exit: {}\n",
            self.pc_read_after_mmio_exit
        ));
        output.push_str(&format!(
            "Emulated value injected: {}\n",
            self.emulated_value_injected
        ));
        output.push_str(&format!("PC advanced: {}\n", self.pc_advanced));
        output.push_str(&format!(
            "Second run attempted: {}\n",
            self.second_run_attempted
        ));
        output.push_str(&format!(
            "Continuation exit observed: {}\n",
            self.continuation_exit_observed
        ));
        output.push_str(&format!(
            "Emulated value preserved: {}\n",
            self.emulated_value_preserved
        ));
        output.push_str(&format!(
            "Watchdog cancel fired: {}\n",
            self.watchdog_cancel_fired
        ));
        output.push_str(&format!("vCPU destroyed: {}\n", self.vcpu_destroyed));
        output.push_str(&format!("Memory unmapped: {}\n", self.memory_unmapped));
        output.push_str(&format!("VM destroyed: {}\n", self.vm_destroyed));
        output.push_str(&format!(
            "Memory deallocated: {}\n",
            self.memory_deallocated
        ));
        output.push_str(&format!("Code IPA start: {:#x}\n", self.code_ipa_start));
        output.push_str(&format!("MMIO IPA: {:#x}\n", self.mmio_ipa));
        output.push_str(&format!("Bytes: {}\n", self.bytes));
        output.push_str(&format!("Instructions: {}\n", self.instructions));
        output.push_str(&format!("Emulated value: {:#x}\n", self.emulated_value));
        output.push_str(&format!(
            "VM create status name: {}\n",
            render_optional_status_name(self.vm_create_status)
        ));
        output.push_str(&format!(
            "Allocate status name: {}\n",
            render_optional_status_name(self.allocate_status)
        ));
        output.push_str(&format!(
            "Map status name: {}\n",
            render_optional_status_name(self.map_status)
        ));
        output.push_str(&format!(
            "vCPU create status name: {}\n",
            render_optional_status_name(self.vcpu_create_status)
        ));
        output.push_str(&format!(
            "PC set status name: {}\n",
            render_optional_status_name(self.pc_set_status)
        ));
        output.push_str(&format!(
            "CPSR set status name: {}\n",
            render_optional_status_name(self.cpsr_set_status)
        ));
        output.push_str(&format!(
            "Address register set status name: {}\n",
            render_optional_status_name(self.address_register_set_status)
        ));
        output.push_str(&format!(
            "First run status name: {}\n",
            render_optional_status_name(self.first_run_status)
        ));
        output.push_str(&format!(
            "MMIO exit reason name: {}\n",
            render_optional_exit_reason_name(self.mmio_exit_reason)
        ));
        output.push_str(&format!(
            "MMIO exit syndrome: {}\n",
            render_optional_u64(self.mmio_exit_syndrome)
        ));
        output.push_str(&format!(
            "MMIO exit virtual address: {}\n",
            render_optional_u64(self.mmio_exit_virtual_address)
        ));
        output.push_str(&format!(
            "MMIO exit physical address: {}\n",
            render_optional_u64(self.mmio_exit_physical_address)
        ));
        output.push_str(&format!(
            "First watchdog cancel status name: {}\n",
            render_optional_status_name(self.first_watchdog_cancel_status)
        ));
        output.push_str(&format!(
            "PC read status name: {}\n",
            render_optional_status_name(self.pc_read_status)
        ));
        output.push_str(&format!(
            "PC after MMIO exit: {}\n",
            render_optional_u64(self.pc_after_mmio_exit)
        ));
        output.push_str(&format!(
            "Emulated value set status name: {}\n",
            render_optional_status_name(self.emulated_value_set_status)
        ));
        output.push_str(&format!(
            "PC advance status name: {}\n",
            render_optional_status_name(self.pc_advance_status)
        ));
        output.push_str(&format!(
            "Second run status name: {}\n",
            render_optional_status_name(self.second_run_status)
        ));
        output.push_str(&format!(
            "Continuation exit reason name: {}\n",
            render_optional_exit_reason_name(self.continuation_exit_reason)
        ));
        output.push_str(&format!(
            "Continuation exit syndrome: {}\n",
            render_optional_u64(self.continuation_exit_syndrome)
        ));
        output.push_str(&format!(
            "Continuation exit virtual address: {}\n",
            render_optional_u64(self.continuation_exit_virtual_address)
        ));
        output.push_str(&format!(
            "Continuation exit physical address: {}\n",
            render_optional_u64(self.continuation_exit_physical_address)
        ));
        output.push_str(&format!(
            "Second watchdog cancel status name: {}\n",
            render_optional_status_name(self.second_watchdog_cancel_status)
        ));
        output.push_str(&format!(
            "Emulated value read status name: {}\n",
            render_optional_status_name(self.emulated_value_read_status)
        ));
        output.push_str(&format!(
            "Emulated value after continue: {}\n",
            render_optional_u64(self.emulated_value_after_continue)
        ));
        output.push_str(&format!(
            "vCPU destroy status name: {}\n",
            render_optional_status_name(self.vcpu_destroy_status)
        ));
        output.push_str(&format!(
            "Unmap status name: {}\n",
            render_optional_status_name(self.unmap_status)
        ));
        output.push_str(&format!(
            "VM destroy status name: {}\n",
            render_optional_status_name(self.vm_destroy_status)
        ));
        output.push_str(&format!(
            "Deallocate status name: {}\n",
            render_optional_status_name(self.deallocate_status)
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

pub fn probe_hvf_mmio_read_emulation(allow_emulate: bool) -> HvfMmioReadEmulationProbe {
    let host = query_hvf_host_capabilities();
    platform::probe_hvf_mmio_read_emulation(allow_emulate, host)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HvfMmioWriteEmulationProbe {
    pub allowed: bool,
    pub attempted: bool,
    pub vm_created: bool,
    pub memory_allocated: bool,
    pub memory_mapped: bool,
    pub vcpu_created: bool,
    pub pc_set: bool,
    pub cpsr_set: bool,
    pub write_value_register_set: bool,
    pub address_register_set: bool,
    pub first_run_attempted: bool,
    pub mmio_exit_observed: bool,
    pub pc_read_after_mmio_exit: bool,
    pub write_value_captured: bool,
    pub pc_advanced: bool,
    pub second_run_attempted: bool,
    pub continuation_exit_observed: bool,
    pub write_value_preserved: bool,
    pub watchdog_cancel_fired: bool,
    pub vcpu_destroyed: bool,
    pub memory_unmapped: bool,
    pub vm_destroyed: bool,
    pub memory_deallocated: bool,
    pub host: HvfHostCapabilities,
    pub code_ipa_start: u64,
    pub mmio_ipa: u64,
    pub bytes: usize,
    pub instructions: &'static str,
    pub write_value: u64,
    pub vm_create_status: Option<i32>,
    pub allocate_status: Option<i32>,
    pub map_status: Option<i32>,
    pub vcpu_create_status: Option<i32>,
    pub pc_set_status: Option<i32>,
    pub cpsr_set_status: Option<i32>,
    pub write_value_register_set_status: Option<i32>,
    pub address_register_set_status: Option<i32>,
    pub first_run_status: Option<i32>,
    pub mmio_exit_reason: Option<u32>,
    pub mmio_exit_syndrome: Option<u64>,
    pub mmio_exit_virtual_address: Option<u64>,
    pub mmio_exit_physical_address: Option<u64>,
    pub first_watchdog_cancel_status: Option<i32>,
    pub pc_read_status: Option<i32>,
    pub pc_after_mmio_exit: Option<u64>,
    pub write_value_capture_status: Option<i32>,
    pub captured_write_value: Option<u64>,
    pub pc_advance_status: Option<i32>,
    pub second_run_status: Option<i32>,
    pub continuation_exit_reason: Option<u32>,
    pub continuation_exit_syndrome: Option<u64>,
    pub continuation_exit_virtual_address: Option<u64>,
    pub continuation_exit_physical_address: Option<u64>,
    pub second_watchdog_cancel_status: Option<i32>,
    pub write_value_after_continue_status: Option<i32>,
    pub write_value_after_continue: Option<u64>,
    pub vcpu_destroy_status: Option<i32>,
    pub unmap_status: Option<i32>,
    pub vm_destroy_status: Option<i32>,
    pub deallocate_status: Option<i32>,
    pub blockers: Vec<String>,
}

impl HvfMmioWriteEmulationProbe {
    pub fn render_text(&self) -> String {
        let mut output = String::new();
        output.push_str("HVF MMIO write emulation probe\n");
        output.push_str("QEMU: not used\n");
        output.push_str("Apple VZ: not used\n");
        output.push_str("Guest execution: unmapped STR, captured write value, then HVC\n");
        output.push_str(&format!("Host: {}\n", self.host.host));
        output.push_str(&format!("Host HVF available: {}\n", self.host.available));
        output.push_str(&format!("Allowed: {}\n", self.allowed));
        output.push_str(&format!("Attempted: {}\n", self.attempted));
        output.push_str(&format!("VM created: {}\n", self.vm_created));
        output.push_str(&format!("Memory allocated: {}\n", self.memory_allocated));
        output.push_str(&format!("Memory mapped: {}\n", self.memory_mapped));
        output.push_str(&format!("vCPU created: {}\n", self.vcpu_created));
        output.push_str(&format!("PC set: {}\n", self.pc_set));
        output.push_str(&format!("CPSR set: {}\n", self.cpsr_set));
        output.push_str(&format!(
            "Write value register set: {}\n",
            self.write_value_register_set
        ));
        output.push_str(&format!(
            "Address register set: {}\n",
            self.address_register_set
        ));
        output.push_str(&format!(
            "First run attempted: {}\n",
            self.first_run_attempted
        ));
        output.push_str(&format!(
            "MMIO exit observed: {}\n",
            self.mmio_exit_observed
        ));
        output.push_str(&format!(
            "PC read after MMIO exit: {}\n",
            self.pc_read_after_mmio_exit
        ));
        output.push_str(&format!(
            "Write value captured: {}\n",
            self.write_value_captured
        ));
        output.push_str(&format!("PC advanced: {}\n", self.pc_advanced));
        output.push_str(&format!(
            "Second run attempted: {}\n",
            self.second_run_attempted
        ));
        output.push_str(&format!(
            "Continuation exit observed: {}\n",
            self.continuation_exit_observed
        ));
        output.push_str(&format!(
            "Write value preserved: {}\n",
            self.write_value_preserved
        ));
        output.push_str(&format!(
            "Watchdog cancel fired: {}\n",
            self.watchdog_cancel_fired
        ));
        output.push_str(&format!("vCPU destroyed: {}\n", self.vcpu_destroyed));
        output.push_str(&format!("Memory unmapped: {}\n", self.memory_unmapped));
        output.push_str(&format!("VM destroyed: {}\n", self.vm_destroyed));
        output.push_str(&format!(
            "Memory deallocated: {}\n",
            self.memory_deallocated
        ));
        output.push_str(&format!("Code IPA start: {:#x}\n", self.code_ipa_start));
        output.push_str(&format!("MMIO IPA: {:#x}\n", self.mmio_ipa));
        output.push_str(&format!("Bytes: {}\n", self.bytes));
        output.push_str(&format!("Instructions: {}\n", self.instructions));
        output.push_str(&format!("Write value: {:#x}\n", self.write_value));
        output.push_str(&format!(
            "VM create status name: {}\n",
            render_optional_status_name(self.vm_create_status)
        ));
        output.push_str(&format!(
            "Allocate status name: {}\n",
            render_optional_status_name(self.allocate_status)
        ));
        output.push_str(&format!(
            "Map status name: {}\n",
            render_optional_status_name(self.map_status)
        ));
        output.push_str(&format!(
            "vCPU create status name: {}\n",
            render_optional_status_name(self.vcpu_create_status)
        ));
        output.push_str(&format!(
            "PC set status name: {}\n",
            render_optional_status_name(self.pc_set_status)
        ));
        output.push_str(&format!(
            "CPSR set status name: {}\n",
            render_optional_status_name(self.cpsr_set_status)
        ));
        output.push_str(&format!(
            "Write value register set status name: {}\n",
            render_optional_status_name(self.write_value_register_set_status)
        ));
        output.push_str(&format!(
            "Address register set status name: {}\n",
            render_optional_status_name(self.address_register_set_status)
        ));
        output.push_str(&format!(
            "First run status name: {}\n",
            render_optional_status_name(self.first_run_status)
        ));
        output.push_str(&format!(
            "MMIO exit reason name: {}\n",
            render_optional_exit_reason_name(self.mmio_exit_reason)
        ));
        output.push_str(&format!(
            "MMIO exit syndrome: {}\n",
            render_optional_u64(self.mmio_exit_syndrome)
        ));
        output.push_str(&format!(
            "MMIO exit virtual address: {}\n",
            render_optional_u64(self.mmio_exit_virtual_address)
        ));
        output.push_str(&format!(
            "MMIO exit physical address: {}\n",
            render_optional_u64(self.mmio_exit_physical_address)
        ));
        output.push_str(&format!(
            "First watchdog cancel status name: {}\n",
            render_optional_status_name(self.first_watchdog_cancel_status)
        ));
        output.push_str(&format!(
            "PC read status name: {}\n",
            render_optional_status_name(self.pc_read_status)
        ));
        output.push_str(&format!(
            "PC after MMIO exit: {}\n",
            render_optional_u64(self.pc_after_mmio_exit)
        ));
        output.push_str(&format!(
            "Write value capture status name: {}\n",
            render_optional_status_name(self.write_value_capture_status)
        ));
        output.push_str(&format!(
            "Captured write value: {}\n",
            render_optional_u64(self.captured_write_value)
        ));
        output.push_str(&format!(
            "PC advance status name: {}\n",
            render_optional_status_name(self.pc_advance_status)
        ));
        output.push_str(&format!(
            "Second run status name: {}\n",
            render_optional_status_name(self.second_run_status)
        ));
        output.push_str(&format!(
            "Continuation exit reason name: {}\n",
            render_optional_exit_reason_name(self.continuation_exit_reason)
        ));
        output.push_str(&format!(
            "Continuation exit syndrome: {}\n",
            render_optional_u64(self.continuation_exit_syndrome)
        ));
        output.push_str(&format!(
            "Continuation exit virtual address: {}\n",
            render_optional_u64(self.continuation_exit_virtual_address)
        ));
        output.push_str(&format!(
            "Continuation exit physical address: {}\n",
            render_optional_u64(self.continuation_exit_physical_address)
        ));
        output.push_str(&format!(
            "Second watchdog cancel status name: {}\n",
            render_optional_status_name(self.second_watchdog_cancel_status)
        ));
        output.push_str(&format!(
            "Write value after continue status name: {}\n",
            render_optional_status_name(self.write_value_after_continue_status)
        ));
        output.push_str(&format!(
            "Write value after continue: {}\n",
            render_optional_u64(self.write_value_after_continue)
        ));
        output.push_str(&format!(
            "vCPU destroy status name: {}\n",
            render_optional_status_name(self.vcpu_destroy_status)
        ));
        output.push_str(&format!(
            "Unmap status name: {}\n",
            render_optional_status_name(self.unmap_status)
        ));
        output.push_str(&format!(
            "VM destroy status name: {}\n",
            render_optional_status_name(self.vm_destroy_status)
        ));
        output.push_str(&format!(
            "Deallocate status name: {}\n",
            render_optional_status_name(self.deallocate_status)
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

pub fn probe_hvf_mmio_write_emulation(allow_emulate: bool) -> HvfMmioWriteEmulationProbe {
    let host = query_hvf_host_capabilities();
    platform::probe_hvf_mmio_write_emulation(allow_emulate, host)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HvfMmioSerialDeviceProbe {
    pub allowed: bool,
    pub attempted: bool,
    pub vm_created: bool,
    pub memory_allocated: bool,
    pub memory_mapped: bool,
    pub vcpu_created: bool,
    pub pc_set: bool,
    pub cpsr_set: bool,
    pub write_value_register_set: bool,
    pub data_address_register_set: bool,
    pub status_address_register_set: bool,
    pub device_bus_created: bool,
    pub device_bus_device_count: usize,
    pub write_run_attempted: bool,
    pub write_exit_observed: bool,
    pub write_handled_by_device: bool,
    pub write_value_captured: bool,
    pub pc_advanced_after_write: bool,
    pub status_run_attempted: bool,
    pub status_exit_observed: bool,
    pub status_handled_by_device: bool,
    pub status_value_injected: bool,
    pub pc_advanced_after_status: bool,
    pub continuation_run_attempted: bool,
    pub continuation_exit_observed: bool,
    pub status_value_preserved: bool,
    pub watchdog_cancel_fired: bool,
    pub vcpu_destroyed: bool,
    pub memory_unmapped: bool,
    pub vm_destroyed: bool,
    pub memory_deallocated: bool,
    pub host: HvfHostCapabilities,
    pub device_model: &'static str,
    pub code_ipa_start: u64,
    pub data_ipa: u64,
    pub status_ipa: u64,
    pub bytes: usize,
    pub instructions: &'static str,
    pub serial_write_value: u64,
    pub serial_status_value: u64,
    pub captured_write_value: Option<u64>,
    pub captured_byte: Option<u8>,
    pub vm_create_status: Option<i32>,
    pub allocate_status: Option<i32>,
    pub map_status: Option<i32>,
    pub vcpu_create_status: Option<i32>,
    pub pc_set_status: Option<i32>,
    pub cpsr_set_status: Option<i32>,
    pub write_value_register_set_status: Option<i32>,
    pub data_address_register_set_status: Option<i32>,
    pub status_address_register_set_status: Option<i32>,
    pub write_run_status: Option<i32>,
    pub write_exit_reason: Option<u32>,
    pub write_exit_syndrome: Option<u64>,
    pub write_exit_virtual_address: Option<u64>,
    pub write_exit_physical_address: Option<u64>,
    pub write_watchdog_cancel_status: Option<i32>,
    pub write_value_capture_status: Option<i32>,
    pub pc_read_after_write_status: Option<i32>,
    pub pc_after_write_exit: Option<u64>,
    pub pc_advance_after_write_status: Option<i32>,
    pub status_run_status: Option<i32>,
    pub status_exit_reason: Option<u32>,
    pub status_exit_syndrome: Option<u64>,
    pub status_exit_virtual_address: Option<u64>,
    pub status_exit_physical_address: Option<u64>,
    pub status_watchdog_cancel_status: Option<i32>,
    pub status_value_set_status: Option<i32>,
    pub pc_read_after_status_status: Option<i32>,
    pub pc_after_status_exit: Option<u64>,
    pub pc_advance_after_status_status: Option<i32>,
    pub continuation_run_status: Option<i32>,
    pub continuation_exit_reason: Option<u32>,
    pub continuation_exit_syndrome: Option<u64>,
    pub continuation_exit_virtual_address: Option<u64>,
    pub continuation_exit_physical_address: Option<u64>,
    pub continuation_watchdog_cancel_status: Option<i32>,
    pub status_value_after_continue_status: Option<i32>,
    pub status_value_after_continue: Option<u64>,
    pub vcpu_destroy_status: Option<i32>,
    pub unmap_status: Option<i32>,
    pub vm_destroy_status: Option<i32>,
    pub deallocate_status: Option<i32>,
    pub blockers: Vec<String>,
}

impl HvfMmioSerialDeviceProbe {
    pub fn render_text(&self) -> String {
        let mut output = String::new();
        output.push_str("HVF MMIO serial device probe\n");
        output.push_str("QEMU: not used\n");
        output.push_str("Apple VZ: not used\n");
        output.push_str("Guest execution: STR data register, LDR status register, then HVC\n");
        output.push_str(&format!("Device model: {}\n", self.device_model));
        output.push_str(&format!("Host: {}\n", self.host.host));
        output.push_str(&format!("Host HVF available: {}\n", self.host.available));
        output.push_str(&format!("Allowed: {}\n", self.allowed));
        output.push_str(&format!("Attempted: {}\n", self.attempted));
        output.push_str(&format!("VM created: {}\n", self.vm_created));
        output.push_str(&format!("Memory allocated: {}\n", self.memory_allocated));
        output.push_str(&format!("Memory mapped: {}\n", self.memory_mapped));
        output.push_str(&format!("vCPU created: {}\n", self.vcpu_created));
        output.push_str(&format!("PC set: {}\n", self.pc_set));
        output.push_str(&format!("CPSR set: {}\n", self.cpsr_set));
        output.push_str(&format!(
            "Write value register set: {}\n",
            self.write_value_register_set
        ));
        output.push_str(&format!(
            "Data address register set: {}\n",
            self.data_address_register_set
        ));
        output.push_str(&format!(
            "Status address register set: {}\n",
            self.status_address_register_set
        ));
        output.push_str(&format!(
            "Device bus created: {}\n",
            self.device_bus_created
        ));
        output.push_str(&format!(
            "Device bus device count: {}\n",
            self.device_bus_device_count
        ));
        output.push_str(&format!(
            "Write run attempted: {}\n",
            self.write_run_attempted
        ));
        output.push_str(&format!(
            "Write exit observed: {}\n",
            self.write_exit_observed
        ));
        output.push_str(&format!(
            "Write handled by device: {}\n",
            self.write_handled_by_device
        ));
        output.push_str(&format!(
            "Write value captured: {}\n",
            self.write_value_captured
        ));
        output.push_str(&format!(
            "PC advanced after write: {}\n",
            self.pc_advanced_after_write
        ));
        output.push_str(&format!(
            "Status run attempted: {}\n",
            self.status_run_attempted
        ));
        output.push_str(&format!(
            "Status exit observed: {}\n",
            self.status_exit_observed
        ));
        output.push_str(&format!(
            "Status handled by device: {}\n",
            self.status_handled_by_device
        ));
        output.push_str(&format!(
            "Status value injected: {}\n",
            self.status_value_injected
        ));
        output.push_str(&format!(
            "PC advanced after status: {}\n",
            self.pc_advanced_after_status
        ));
        output.push_str(&format!(
            "Continuation run attempted: {}\n",
            self.continuation_run_attempted
        ));
        output.push_str(&format!(
            "Continuation exit observed: {}\n",
            self.continuation_exit_observed
        ));
        output.push_str(&format!(
            "Status value preserved: {}\n",
            self.status_value_preserved
        ));
        output.push_str(&format!(
            "Watchdog cancel fired: {}\n",
            self.watchdog_cancel_fired
        ));
        output.push_str(&format!("vCPU destroyed: {}\n", self.vcpu_destroyed));
        output.push_str(&format!("Memory unmapped: {}\n", self.memory_unmapped));
        output.push_str(&format!("VM destroyed: {}\n", self.vm_destroyed));
        output.push_str(&format!(
            "Memory deallocated: {}\n",
            self.memory_deallocated
        ));
        output.push_str(&format!("Code IPA start: {:#x}\n", self.code_ipa_start));
        output.push_str(&format!("Serial data IPA: {:#x}\n", self.data_ipa));
        output.push_str(&format!("Serial status IPA: {:#x}\n", self.status_ipa));
        output.push_str(&format!("Bytes: {}\n", self.bytes));
        output.push_str(&format!("Instructions: {}\n", self.instructions));
        output.push_str(&format!(
            "Serial write value: {:#x}\n",
            self.serial_write_value
        ));
        output.push_str(&format!(
            "Serial status value: {:#x}\n",
            self.serial_status_value
        ));
        output.push_str(&format!(
            "Captured write value: {}\n",
            render_optional_u64(self.captured_write_value)
        ));
        output.push_str(&format!(
            "Captured byte: {}\n",
            self.captured_byte
                .map_or_else(|| "not observed".to_string(), |value| format!("{value:#x}"))
        ));
        output.push_str(&format!(
            "VM create status name: {}\n",
            render_optional_status_name(self.vm_create_status)
        ));
        output.push_str(&format!(
            "Allocate status name: {}\n",
            render_optional_status_name(self.allocate_status)
        ));
        output.push_str(&format!(
            "Map status name: {}\n",
            render_optional_status_name(self.map_status)
        ));
        output.push_str(&format!(
            "vCPU create status name: {}\n",
            render_optional_status_name(self.vcpu_create_status)
        ));
        output.push_str(&format!(
            "PC set status name: {}\n",
            render_optional_status_name(self.pc_set_status)
        ));
        output.push_str(&format!(
            "CPSR set status name: {}\n",
            render_optional_status_name(self.cpsr_set_status)
        ));
        output.push_str(&format!(
            "Write value register set status name: {}\n",
            render_optional_status_name(self.write_value_register_set_status)
        ));
        output.push_str(&format!(
            "Data address register set status name: {}\n",
            render_optional_status_name(self.data_address_register_set_status)
        ));
        output.push_str(&format!(
            "Status address register set status name: {}\n",
            render_optional_status_name(self.status_address_register_set_status)
        ));
        output.push_str(&format!(
            "Write run status name: {}\n",
            render_optional_status_name(self.write_run_status)
        ));
        output.push_str(&format!(
            "Write exit reason name: {}\n",
            render_optional_exit_reason_name(self.write_exit_reason)
        ));
        output.push_str(&format!(
            "Write exit syndrome: {}\n",
            render_optional_u64(self.write_exit_syndrome)
        ));
        output.push_str(&format!(
            "Write exit virtual address: {}\n",
            render_optional_u64(self.write_exit_virtual_address)
        ));
        output.push_str(&format!(
            "Write exit physical address: {}\n",
            render_optional_u64(self.write_exit_physical_address)
        ));
        output.push_str(&format!(
            "Write watchdog cancel status name: {}\n",
            render_optional_status_name(self.write_watchdog_cancel_status)
        ));
        output.push_str(&format!(
            "Write value capture status name: {}\n",
            render_optional_status_name(self.write_value_capture_status)
        ));
        output.push_str(&format!(
            "PC read after write status name: {}\n",
            render_optional_status_name(self.pc_read_after_write_status)
        ));
        output.push_str(&format!(
            "PC after write exit: {}\n",
            render_optional_u64(self.pc_after_write_exit)
        ));
        output.push_str(&format!(
            "PC advance after write status name: {}\n",
            render_optional_status_name(self.pc_advance_after_write_status)
        ));
        output.push_str(&format!(
            "Status run status name: {}\n",
            render_optional_status_name(self.status_run_status)
        ));
        output.push_str(&format!(
            "Status exit reason name: {}\n",
            render_optional_exit_reason_name(self.status_exit_reason)
        ));
        output.push_str(&format!(
            "Status exit syndrome: {}\n",
            render_optional_u64(self.status_exit_syndrome)
        ));
        output.push_str(&format!(
            "Status exit virtual address: {}\n",
            render_optional_u64(self.status_exit_virtual_address)
        ));
        output.push_str(&format!(
            "Status exit physical address: {}\n",
            render_optional_u64(self.status_exit_physical_address)
        ));
        output.push_str(&format!(
            "Status watchdog cancel status name: {}\n",
            render_optional_status_name(self.status_watchdog_cancel_status)
        ));
        output.push_str(&format!(
            "Status value set status name: {}\n",
            render_optional_status_name(self.status_value_set_status)
        ));
        output.push_str(&format!(
            "PC read after status status name: {}\n",
            render_optional_status_name(self.pc_read_after_status_status)
        ));
        output.push_str(&format!(
            "PC after status exit: {}\n",
            render_optional_u64(self.pc_after_status_exit)
        ));
        output.push_str(&format!(
            "PC advance after status status name: {}\n",
            render_optional_status_name(self.pc_advance_after_status_status)
        ));
        output.push_str(&format!(
            "Continuation run status name: {}\n",
            render_optional_status_name(self.continuation_run_status)
        ));
        output.push_str(&format!(
            "Continuation exit reason name: {}\n",
            render_optional_exit_reason_name(self.continuation_exit_reason)
        ));
        output.push_str(&format!(
            "Continuation exit syndrome: {}\n",
            render_optional_u64(self.continuation_exit_syndrome)
        ));
        output.push_str(&format!(
            "Continuation exit virtual address: {}\n",
            render_optional_u64(self.continuation_exit_virtual_address)
        ));
        output.push_str(&format!(
            "Continuation exit physical address: {}\n",
            render_optional_u64(self.continuation_exit_physical_address)
        ));
        output.push_str(&format!(
            "Continuation watchdog cancel status name: {}\n",
            render_optional_status_name(self.continuation_watchdog_cancel_status)
        ));
        output.push_str(&format!(
            "Status value after continue status name: {}\n",
            render_optional_status_name(self.status_value_after_continue_status)
        ));
        output.push_str(&format!(
            "Status value after continue: {}\n",
            render_optional_u64(self.status_value_after_continue)
        ));
        output.push_str(&format!(
            "vCPU destroy status name: {}\n",
            render_optional_status_name(self.vcpu_destroy_status)
        ));
        output.push_str(&format!(
            "Unmap status name: {}\n",
            render_optional_status_name(self.unmap_status)
        ));
        output.push_str(&format!(
            "VM destroy status name: {}\n",
            render_optional_status_name(self.vm_destroy_status)
        ));
        output.push_str(&format!(
            "Deallocate status name: {}\n",
            render_optional_status_name(self.deallocate_status)
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

pub fn probe_hvf_mmio_serial_device(allow_device: bool) -> HvfMmioSerialDeviceProbe {
    let host = query_hvf_host_capabilities();
    platform::probe_hvf_mmio_serial_device(allow_device, host)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HvfMmioRtcDeviceProbe {
    pub allowed: bool,
    pub attempted: bool,
    pub vm_created: bool,
    pub memory_allocated: bool,
    pub memory_mapped: bool,
    pub vcpu_created: bool,
    pub pc_set: bool,
    pub cpsr_set: bool,
    pub rtc_address_register_set: bool,
    pub device_bus_created: bool,
    pub device_bus_device_count: usize,
    pub first_run_attempted: bool,
    pub rtc_exit_observed: bool,
    pub rtc_handled_by_device: bool,
    pub rtc_value_injected: bool,
    pub pc_read_after_rtc_exit: bool,
    pub pc_advanced: bool,
    pub second_run_attempted: bool,
    pub continuation_exit_observed: bool,
    pub rtc_value_preserved: bool,
    pub watchdog_cancel_fired: bool,
    pub vcpu_destroyed: bool,
    pub memory_unmapped: bool,
    pub vm_destroyed: bool,
    pub memory_deallocated: bool,
    pub host: HvfHostCapabilities,
    pub device_models: &'static str,
    pub code_ipa_start: u64,
    pub uart_ipa: u64,
    pub rtc_ipa: u64,
    pub bytes: usize,
    pub instructions: &'static str,
    pub rtc_value: u64,
    pub vm_create_status: Option<i32>,
    pub allocate_status: Option<i32>,
    pub map_status: Option<i32>,
    pub vcpu_create_status: Option<i32>,
    pub pc_set_status: Option<i32>,
    pub cpsr_set_status: Option<i32>,
    pub rtc_address_register_set_status: Option<i32>,
    pub first_run_status: Option<i32>,
    pub rtc_exit_reason: Option<u32>,
    pub rtc_exit_syndrome: Option<u64>,
    pub rtc_exit_virtual_address: Option<u64>,
    pub rtc_exit_physical_address: Option<u64>,
    pub first_watchdog_cancel_status: Option<i32>,
    pub rtc_value_set_status: Option<i32>,
    pub pc_read_status: Option<i32>,
    pub pc_after_rtc_exit: Option<u64>,
    pub pc_advance_status: Option<i32>,
    pub second_run_status: Option<i32>,
    pub continuation_exit_reason: Option<u32>,
    pub continuation_exit_syndrome: Option<u64>,
    pub continuation_exit_virtual_address: Option<u64>,
    pub continuation_exit_physical_address: Option<u64>,
    pub second_watchdog_cancel_status: Option<i32>,
    pub rtc_value_after_continue_status: Option<i32>,
    pub rtc_value_after_continue: Option<u64>,
    pub vcpu_destroy_status: Option<i32>,
    pub unmap_status: Option<i32>,
    pub vm_destroy_status: Option<i32>,
    pub deallocate_status: Option<i32>,
    pub blockers: Vec<String>,
}

impl HvfMmioRtcDeviceProbe {
    pub fn render_text(&self) -> String {
        let mut output = String::new();
        output.push_str("HVF MMIO RTC device probe\n");
        output.push_str("QEMU: not used\n");
        output.push_str("Apple VZ: not used\n");
        output.push_str("Guest execution: LDR RTC data register, then HVC\n");
        output.push_str(&format!("Device models: {}\n", self.device_models));
        output.push_str(&format!("Host: {}\n", self.host.host));
        output.push_str(&format!("Host HVF available: {}\n", self.host.available));
        output.push_str(&format!("Allowed: {}\n", self.allowed));
        output.push_str(&format!("Attempted: {}\n", self.attempted));
        output.push_str(&format!("VM created: {}\n", self.vm_created));
        output.push_str(&format!("Memory allocated: {}\n", self.memory_allocated));
        output.push_str(&format!("Memory mapped: {}\n", self.memory_mapped));
        output.push_str(&format!("vCPU created: {}\n", self.vcpu_created));
        output.push_str(&format!("PC set: {}\n", self.pc_set));
        output.push_str(&format!("CPSR set: {}\n", self.cpsr_set));
        output.push_str(&format!(
            "RTC address register set: {}\n",
            self.rtc_address_register_set
        ));
        output.push_str(&format!(
            "Device bus created: {}\n",
            self.device_bus_created
        ));
        output.push_str(&format!(
            "Device bus device count: {}\n",
            self.device_bus_device_count
        ));
        output.push_str(&format!(
            "First run attempted: {}\n",
            self.first_run_attempted
        ));
        output.push_str(&format!("RTC exit observed: {}\n", self.rtc_exit_observed));
        output.push_str(&format!(
            "RTC handled by device: {}\n",
            self.rtc_handled_by_device
        ));
        output.push_str(&format!(
            "RTC value injected: {}\n",
            self.rtc_value_injected
        ));
        output.push_str(&format!(
            "PC read after RTC exit: {}\n",
            self.pc_read_after_rtc_exit
        ));
        output.push_str(&format!("PC advanced: {}\n", self.pc_advanced));
        output.push_str(&format!(
            "Second run attempted: {}\n",
            self.second_run_attempted
        ));
        output.push_str(&format!(
            "Continuation exit observed: {}\n",
            self.continuation_exit_observed
        ));
        output.push_str(&format!(
            "RTC value preserved: {}\n",
            self.rtc_value_preserved
        ));
        output.push_str(&format!(
            "Watchdog cancel fired: {}\n",
            self.watchdog_cancel_fired
        ));
        output.push_str(&format!("vCPU destroyed: {}\n", self.vcpu_destroyed));
        output.push_str(&format!("Memory unmapped: {}\n", self.memory_unmapped));
        output.push_str(&format!("VM destroyed: {}\n", self.vm_destroyed));
        output.push_str(&format!(
            "Memory deallocated: {}\n",
            self.memory_deallocated
        ));
        output.push_str(&format!("Code IPA start: {:#x}\n", self.code_ipa_start));
        output.push_str(&format!("UART IPA: {:#x}\n", self.uart_ipa));
        output.push_str(&format!("RTC IPA: {:#x}\n", self.rtc_ipa));
        output.push_str(&format!("Bytes: {}\n", self.bytes));
        output.push_str(&format!("Instructions: {}\n", self.instructions));
        output.push_str(&format!("RTC value: {:#x}\n", self.rtc_value));
        output.push_str(&format!(
            "VM create status name: {}\n",
            render_optional_status_name(self.vm_create_status)
        ));
        output.push_str(&format!(
            "Allocate status name: {}\n",
            render_optional_status_name(self.allocate_status)
        ));
        output.push_str(&format!(
            "Map status name: {}\n",
            render_optional_status_name(self.map_status)
        ));
        output.push_str(&format!(
            "vCPU create status name: {}\n",
            render_optional_status_name(self.vcpu_create_status)
        ));
        output.push_str(&format!(
            "PC set status name: {}\n",
            render_optional_status_name(self.pc_set_status)
        ));
        output.push_str(&format!(
            "CPSR set status name: {}\n",
            render_optional_status_name(self.cpsr_set_status)
        ));
        output.push_str(&format!(
            "RTC address register set status name: {}\n",
            render_optional_status_name(self.rtc_address_register_set_status)
        ));
        output.push_str(&format!(
            "First run status name: {}\n",
            render_optional_status_name(self.first_run_status)
        ));
        output.push_str(&format!(
            "RTC exit reason name: {}\n",
            render_optional_exit_reason_name(self.rtc_exit_reason)
        ));
        output.push_str(&format!(
            "RTC exit syndrome: {}\n",
            render_optional_u64(self.rtc_exit_syndrome)
        ));
        output.push_str(&format!(
            "RTC exit virtual address: {}\n",
            render_optional_u64(self.rtc_exit_virtual_address)
        ));
        output.push_str(&format!(
            "RTC exit physical address: {}\n",
            render_optional_u64(self.rtc_exit_physical_address)
        ));
        output.push_str(&format!(
            "First watchdog cancel status name: {}\n",
            render_optional_status_name(self.first_watchdog_cancel_status)
        ));
        output.push_str(&format!(
            "RTC value set status name: {}\n",
            render_optional_status_name(self.rtc_value_set_status)
        ));
        output.push_str(&format!(
            "PC read status name: {}\n",
            render_optional_status_name(self.pc_read_status)
        ));
        output.push_str(&format!(
            "PC after RTC exit: {}\n",
            render_optional_u64(self.pc_after_rtc_exit)
        ));
        output.push_str(&format!(
            "PC advance status name: {}\n",
            render_optional_status_name(self.pc_advance_status)
        ));
        output.push_str(&format!(
            "Second run status name: {}\n",
            render_optional_status_name(self.second_run_status)
        ));
        output.push_str(&format!(
            "Continuation exit reason name: {}\n",
            render_optional_exit_reason_name(self.continuation_exit_reason)
        ));
        output.push_str(&format!(
            "Continuation exit syndrome: {}\n",
            render_optional_u64(self.continuation_exit_syndrome)
        ));
        output.push_str(&format!(
            "Continuation exit virtual address: {}\n",
            render_optional_u64(self.continuation_exit_virtual_address)
        ));
        output.push_str(&format!(
            "Continuation exit physical address: {}\n",
            render_optional_u64(self.continuation_exit_physical_address)
        ));
        output.push_str(&format!(
            "Second watchdog cancel status name: {}\n",
            render_optional_status_name(self.second_watchdog_cancel_status)
        ));
        output.push_str(&format!(
            "RTC value after continue status name: {}\n",
            render_optional_status_name(self.rtc_value_after_continue_status)
        ));
        output.push_str(&format!(
            "RTC value after continue: {}\n",
            render_optional_u64(self.rtc_value_after_continue)
        ));
        output.push_str(&format!(
            "vCPU destroy status name: {}\n",
            render_optional_status_name(self.vcpu_destroy_status)
        ));
        output.push_str(&format!(
            "Unmap status name: {}\n",
            render_optional_status_name(self.unmap_status)
        ));
        output.push_str(&format!(
            "VM destroy status name: {}\n",
            render_optional_status_name(self.vm_destroy_status)
        ));
        output.push_str(&format!(
            "Deallocate status name: {}\n",
            render_optional_status_name(self.deallocate_status)
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

pub fn probe_hvf_mmio_rtc_device(allow_device: bool) -> HvfMmioRtcDeviceProbe {
    let host = query_hvf_host_capabilities();
    platform::probe_hvf_mmio_rtc_device(allow_device, host)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HvfMmioBlockRegisterProbe {
    pub name: &'static str,
    pub ipa: u64,
    pub expected_value: u64,
    pub run_attempted: bool,
    pub exit_observed: bool,
    pub handled_by_device: bool,
    pub value_injected: bool,
    pub pc_read_after_exit: bool,
    pub pc_advanced: bool,
    pub run_status: Option<i32>,
    pub exit_reason: Option<u32>,
    pub exit_syndrome: Option<u64>,
    pub exit_virtual_address: Option<u64>,
    pub exit_physical_address: Option<u64>,
    pub watchdog_cancel_status: Option<i32>,
    pub value_set_status: Option<i32>,
    pub pc_read_status: Option<i32>,
    pub pc_after_exit: Option<u64>,
    pub pc_advance_status: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HvfMmioBlockDeviceProbe {
    pub allowed: bool,
    pub attempted: bool,
    pub vm_created: bool,
    pub memory_allocated: bool,
    pub memory_mapped: bool,
    pub vcpu_created: bool,
    pub pc_set: bool,
    pub cpsr_set: bool,
    pub register_address_registers_set: bool,
    pub device_bus_created: bool,
    pub device_bus_device_count: usize,
    pub register_reads: Vec<HvfMmioBlockRegisterProbe>,
    pub continuation_run_attempted: bool,
    pub continuation_exit_observed: bool,
    pub vendor_value_preserved: bool,
    pub watchdog_cancel_fired: bool,
    pub vcpu_destroyed: bool,
    pub memory_unmapped: bool,
    pub vm_destroyed: bool,
    pub memory_deallocated: bool,
    pub host: HvfHostCapabilities,
    pub device_models: &'static str,
    pub code_ipa_start: u64,
    pub block_ipa: u64,
    pub bytes: usize,
    pub instructions: &'static str,
    pub magic_value: u64,
    pub version_value: u64,
    pub device_id_value: u64,
    pub vendor_id_value: u64,
    pub vm_create_status: Option<i32>,
    pub allocate_status: Option<i32>,
    pub map_status: Option<i32>,
    pub vcpu_create_status: Option<i32>,
    pub pc_set_status: Option<i32>,
    pub cpsr_set_status: Option<i32>,
    pub register_address_registers_set_status: Vec<Option<i32>>,
    pub continuation_run_status: Option<i32>,
    pub continuation_exit_reason: Option<u32>,
    pub continuation_exit_syndrome: Option<u64>,
    pub continuation_exit_virtual_address: Option<u64>,
    pub continuation_exit_physical_address: Option<u64>,
    pub continuation_watchdog_cancel_status: Option<i32>,
    pub vendor_value_after_continue_status: Option<i32>,
    pub vendor_value_after_continue: Option<u64>,
    pub vcpu_destroy_status: Option<i32>,
    pub unmap_status: Option<i32>,
    pub vm_destroy_status: Option<i32>,
    pub deallocate_status: Option<i32>,
    pub blockers: Vec<String>,
}

impl HvfMmioBlockDeviceProbe {
    pub fn render_text(&self) -> String {
        let mut output = String::new();
        output.push_str("HVF MMIO block device probe\n");
        output.push_str("QEMU: not used\n");
        output.push_str("Apple VZ: not used\n");
        output.push_str("Guest execution: LDR W0 VirtIO-MMIO identity registers, then HVC\n");
        output.push_str(&format!("Device models: {}\n", self.device_models));
        output.push_str(&format!("Host: {}\n", self.host.host));
        output.push_str(&format!("Host HVF available: {}\n", self.host.available));
        output.push_str(&format!("Allowed: {}\n", self.allowed));
        output.push_str(&format!("Attempted: {}\n", self.attempted));
        output.push_str(&format!("VM created: {}\n", self.vm_created));
        output.push_str(&format!("Memory allocated: {}\n", self.memory_allocated));
        output.push_str(&format!("Memory mapped: {}\n", self.memory_mapped));
        output.push_str(&format!("vCPU created: {}\n", self.vcpu_created));
        output.push_str(&format!("PC set: {}\n", self.pc_set));
        output.push_str(&format!("CPSR set: {}\n", self.cpsr_set));
        output.push_str(&format!(
            "Register address registers set: {}\n",
            self.register_address_registers_set
        ));
        output.push_str(&format!(
            "Device bus created: {}\n",
            self.device_bus_created
        ));
        output.push_str(&format!(
            "Device bus device count: {}\n",
            self.device_bus_device_count
        ));
        output.push_str("VirtIO-MMIO block identity reads:\n");
        for read in &self.register_reads {
            output.push_str(&format!(
                "- {} at {:#x}: expected {:#x}, run={}, exit={}, handled={}, injected={}, pc_advanced={}\n",
                read.name,
                read.ipa,
                read.expected_value,
                read.run_attempted,
                read.exit_observed,
                read.handled_by_device,
                read.value_injected,
                read.pc_advanced
            ));
            output.push_str(&format!(
                "  {} run status name: {}\n",
                read.name,
                render_optional_status_name(read.run_status)
            ));
            output.push_str(&format!(
                "  {} exit reason name: {}\n",
                read.name,
                render_optional_exit_reason_name(read.exit_reason)
            ));
            output.push_str(&format!(
                "  {} exit syndrome: {}\n",
                read.name,
                render_optional_u64(read.exit_syndrome)
            ));
            output.push_str(&format!(
                "  {} exit virtual address: {}\n",
                read.name,
                render_optional_u64(read.exit_virtual_address)
            ));
            output.push_str(&format!(
                "  {} exit physical address: {}\n",
                read.name,
                render_optional_u64(read.exit_physical_address)
            ));
            output.push_str(&format!(
                "  {} watchdog cancel status name: {}\n",
                read.name,
                render_optional_status_name(read.watchdog_cancel_status)
            ));
            output.push_str(&format!(
                "  {} value set status name: {}\n",
                read.name,
                render_optional_status_name(read.value_set_status)
            ));
            output.push_str(&format!(
                "  {} PC read status name: {}\n",
                read.name,
                render_optional_status_name(read.pc_read_status)
            ));
            output.push_str(&format!(
                "  {} PC after exit: {}\n",
                read.name,
                render_optional_u64(read.pc_after_exit)
            ));
            output.push_str(&format!(
                "  {} PC advance status name: {}\n",
                read.name,
                render_optional_status_name(read.pc_advance_status)
            ));
        }
        output.push_str(&format!(
            "Continuation run attempted: {}\n",
            self.continuation_run_attempted
        ));
        output.push_str(&format!(
            "Continuation exit observed: {}\n",
            self.continuation_exit_observed
        ));
        output.push_str(&format!(
            "Vendor value preserved: {}\n",
            self.vendor_value_preserved
        ));
        output.push_str(&format!(
            "Watchdog cancel fired: {}\n",
            self.watchdog_cancel_fired
        ));
        output.push_str(&format!("vCPU destroyed: {}\n", self.vcpu_destroyed));
        output.push_str(&format!("Memory unmapped: {}\n", self.memory_unmapped));
        output.push_str(&format!("VM destroyed: {}\n", self.vm_destroyed));
        output.push_str(&format!(
            "Memory deallocated: {}\n",
            self.memory_deallocated
        ));
        output.push_str(&format!("Code IPA start: {:#x}\n", self.code_ipa_start));
        output.push_str(&format!("Block IPA: {:#x}\n", self.block_ipa));
        output.push_str(&format!("Bytes: {}\n", self.bytes));
        output.push_str(&format!("Instructions: {}\n", self.instructions));
        output.push_str(&format!("VirtIO magic value: {:#x}\n", self.magic_value));
        output.push_str(&format!(
            "VirtIO version value: {:#x}\n",
            self.version_value
        ));
        output.push_str(&format!(
            "VirtIO block device ID value: {:#x}\n",
            self.device_id_value
        ));
        output.push_str(&format!(
            "VirtIO vendor ID value: {:#x}\n",
            self.vendor_id_value
        ));
        output.push_str(&format!(
            "VM create status name: {}\n",
            render_optional_status_name(self.vm_create_status)
        ));
        output.push_str(&format!(
            "Allocate status name: {}\n",
            render_optional_status_name(self.allocate_status)
        ));
        output.push_str(&format!(
            "Map status name: {}\n",
            render_optional_status_name(self.map_status)
        ));
        output.push_str(&format!(
            "vCPU create status name: {}\n",
            render_optional_status_name(self.vcpu_create_status)
        ));
        output.push_str(&format!(
            "PC set status name: {}\n",
            render_optional_status_name(self.pc_set_status)
        ));
        output.push_str(&format!(
            "CPSR set status name: {}\n",
            render_optional_status_name(self.cpsr_set_status)
        ));
        output.push_str("Register address set status names:\n");
        for (index, status) in self
            .register_address_registers_set_status
            .iter()
            .enumerate()
        {
            output.push_str(&format!(
                "- X{}: {}\n",
                index + 1,
                render_optional_status_name(*status)
            ));
        }
        output.push_str(&format!(
            "Continuation run status name: {}\n",
            render_optional_status_name(self.continuation_run_status)
        ));
        output.push_str(&format!(
            "Continuation exit reason name: {}\n",
            render_optional_exit_reason_name(self.continuation_exit_reason)
        ));
        output.push_str(&format!(
            "Continuation exit syndrome: {}\n",
            render_optional_u64(self.continuation_exit_syndrome)
        ));
        output.push_str(&format!(
            "Continuation exit virtual address: {}\n",
            render_optional_u64(self.continuation_exit_virtual_address)
        ));
        output.push_str(&format!(
            "Continuation exit physical address: {}\n",
            render_optional_u64(self.continuation_exit_physical_address)
        ));
        output.push_str(&format!(
            "Continuation watchdog cancel status name: {}\n",
            render_optional_status_name(self.continuation_watchdog_cancel_status)
        ));
        output.push_str(&format!(
            "Vendor value after continue status name: {}\n",
            render_optional_status_name(self.vendor_value_after_continue_status)
        ));
        output.push_str(&format!(
            "Vendor value after continue: {}\n",
            render_optional_u64(self.vendor_value_after_continue)
        ));
        output.push_str(&format!(
            "vCPU destroy status name: {}\n",
            render_optional_status_name(self.vcpu_destroy_status)
        ));
        output.push_str(&format!(
            "Unmap status name: {}\n",
            render_optional_status_name(self.unmap_status)
        ));
        output.push_str(&format!(
            "VM destroy status name: {}\n",
            render_optional_status_name(self.vm_destroy_status)
        ));
        output.push_str(&format!(
            "Deallocate status name: {}\n",
            render_optional_status_name(self.deallocate_status)
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

pub fn probe_hvf_mmio_block_device(allow_device: bool) -> HvfMmioBlockDeviceProbe {
    let host = query_hvf_host_capabilities();
    platform::probe_hvf_mmio_block_device(allow_device, host)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HvfMmioBlockQueueStepProbe {
    pub name: &'static str,
    pub access: &'static str,
    pub ipa: u64,
    pub expected_value: Option<u64>,
    pub write_value: Option<u64>,
    pub run_attempted: bool,
    pub address_register_set: bool,
    pub write_value_register_set: bool,
    pub exit_observed: bool,
    pub handled_by_device: bool,
    pub value_injected: bool,
    pub write_accepted: bool,
    pub pc_read_after_exit: bool,
    pub pc_advanced: bool,
    pub captured_write_value: Option<u64>,
    pub run_status: Option<i32>,
    pub address_register_set_status: Option<i32>,
    pub write_value_register_set_status: Option<i32>,
    pub exit_reason: Option<u32>,
    pub exit_syndrome: Option<u64>,
    pub exit_virtual_address: Option<u64>,
    pub exit_physical_address: Option<u64>,
    pub watchdog_cancel_status: Option<i32>,
    pub value_set_status: Option<i32>,
    pub pc_read_status: Option<i32>,
    pub pc_after_exit: Option<u64>,
    pub pc_advance_status: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HvfMmioBlockQueueProbe {
    pub allowed: bool,
    pub attempted: bool,
    pub vm_created: bool,
    pub memory_allocated: bool,
    pub memory_mapped: bool,
    pub vcpu_created: bool,
    pub pc_set: bool,
    pub cpsr_set: bool,
    pub device_bus_created: bool,
    pub device_bus_device_count: usize,
    pub steps: Vec<HvfMmioBlockQueueStepProbe>,
    pub continuation_run_attempted: bool,
    pub continuation_exit_observed: bool,
    pub capacity_high_value_preserved: bool,
    pub watchdog_cancel_fired: bool,
    pub vcpu_destroyed: bool,
    pub memory_unmapped: bool,
    pub vm_destroyed: bool,
    pub memory_deallocated: bool,
    pub host: HvfHostCapabilities,
    pub device_models: &'static str,
    pub code_ipa_start: u64,
    pub block_ipa: u64,
    pub bytes: usize,
    pub instructions: &'static str,
    pub device_features_value: u64,
    pub driver_features_value: u64,
    pub queue_select_value: u64,
    pub queue_num_max_value: u64,
    pub queue_num_value: u64,
    pub queue_ready_value: u64,
    pub queue_desc_address: u64,
    pub queue_driver_address: u64,
    pub queue_device_address: u64,
    pub queue_notify_value: u64,
    pub interrupt_status_value: u64,
    pub block_backing_kind: &'static str,
    pub block_backing_path: Option<PathBuf>,
    pub request_ring_seeded: bool,
    pub request_completed_after_notify: bool,
    pub request_descriptor_index: Option<u16>,
    pub request_sector: Option<u64>,
    pub request_byte_offset: Option<u64>,
    pub request_data_bytes: Option<u32>,
    pub request_data_prefix: Vec<u8>,
    pub request_status: Option<u8>,
    pub request_used_index: Option<u16>,
    pub request_used_len: Option<u32>,
    pub request_interrupt_status: Option<u64>,
    pub write_completed_after_notify: bool,
    pub write_request_type: Option<u32>,
    pub write_sector: Option<u64>,
    pub write_byte_offset: Option<u64>,
    pub write_data_bytes: Option<u32>,
    pub write_data_prefix: Vec<u8>,
    pub write_status: Option<u8>,
    pub write_used_index: Option<u16>,
    pub write_used_len: Option<u32>,
    pub flush_completed_after_notify: bool,
    pub flush_request_type: Option<u32>,
    pub flush_status: Option<u8>,
    pub flush_used_index: Option<u16>,
    pub flush_used_len: Option<u32>,
    pub persisted_data_prefix: Vec<u8>,
    pub status_value: u64,
    pub capacity_sectors: u64,
    pub vm_create_status: Option<i32>,
    pub allocate_status: Option<i32>,
    pub map_status: Option<i32>,
    pub vcpu_create_status: Option<i32>,
    pub pc_set_status: Option<i32>,
    pub cpsr_set_status: Option<i32>,
    pub continuation_run_status: Option<i32>,
    pub continuation_exit_reason: Option<u32>,
    pub continuation_exit_syndrome: Option<u64>,
    pub continuation_exit_virtual_address: Option<u64>,
    pub continuation_exit_physical_address: Option<u64>,
    pub continuation_watchdog_cancel_status: Option<i32>,
    pub capacity_high_after_continue_status: Option<i32>,
    pub capacity_high_after_continue: Option<u64>,
    pub vcpu_destroy_status: Option<i32>,
    pub unmap_status: Option<i32>,
    pub vm_destroy_status: Option<i32>,
    pub deallocate_status: Option<i32>,
    pub blockers: Vec<String>,
}

impl HvfMmioBlockQueueProbe {
    pub fn render_text(&self) -> String {
        let mut output = String::new();
        output.push_str("HVF MMIO block queue/config/address/notify probe\n");
        output.push_str("QEMU: not used\n");
        output.push_str("Apple VZ: not used\n");
        output.push_str(
            "Guest execution: VirtIO-MMIO feature, queue, ring address, notify, status, and capacity registers, then HVC\n",
        );
        output.push_str(&format!("Device models: {}\n", self.device_models));
        output.push_str(&format!("Host: {}\n", self.host.host));
        output.push_str(&format!("Host HVF available: {}\n", self.host.available));
        output.push_str(&format!("Allowed: {}\n", self.allowed));
        output.push_str(&format!("Attempted: {}\n", self.attempted));
        output.push_str(&format!("VM created: {}\n", self.vm_created));
        output.push_str(&format!("Memory allocated: {}\n", self.memory_allocated));
        output.push_str(&format!("Memory mapped: {}\n", self.memory_mapped));
        output.push_str(&format!("vCPU created: {}\n", self.vcpu_created));
        output.push_str(&format!("PC set: {}\n", self.pc_set));
        output.push_str(&format!("CPSR set: {}\n", self.cpsr_set));
        output.push_str(&format!(
            "Device bus created: {}\n",
            self.device_bus_created
        ));
        output.push_str(&format!(
            "Device bus device count: {}\n",
            self.device_bus_device_count
        ));
        output.push_str("VirtIO-MMIO block queue/config steps:\n");
        for step in &self.steps {
            output.push_str(&format!(
                "- {} {} at {:#x}: expected={}, write={}, run={}, address_set={}, write_value_set={}, exit={}, handled={}, injected={}, write_accepted={}, pc_advanced={}, captured={}\n",
                step.access,
                step.name,
                step.ipa,
                render_optional_u64(step.expected_value),
                render_optional_u64(step.write_value),
                step.run_attempted,
                step.address_register_set,
                step.write_value_register_set,
                step.exit_observed,
                step.handled_by_device,
                step.value_injected,
                step.write_accepted,
                step.pc_advanced,
                render_optional_u64(step.captured_write_value)
            ));
            output.push_str(&format!(
                "  {} run status name: {}\n",
                step.name,
                render_optional_status_name(step.run_status)
            ));
            output.push_str(&format!(
                "  {} address register set status name: {}\n",
                step.name,
                render_optional_status_name(step.address_register_set_status)
            ));
            output.push_str(&format!(
                "  {} write value register set status name: {}\n",
                step.name,
                render_optional_status_name(step.write_value_register_set_status)
            ));
            output.push_str(&format!(
                "  {} exit reason name: {}\n",
                step.name,
                render_optional_exit_reason_name(step.exit_reason)
            ));
            output.push_str(&format!(
                "  {} exit syndrome: {}\n",
                step.name,
                render_optional_u64(step.exit_syndrome)
            ));
            output.push_str(&format!(
                "  {} exit virtual address: {}\n",
                step.name,
                render_optional_u64(step.exit_virtual_address)
            ));
            output.push_str(&format!(
                "  {} exit physical address: {}\n",
                step.name,
                render_optional_u64(step.exit_physical_address)
            ));
            output.push_str(&format!(
                "  {} watchdog cancel status name: {}\n",
                step.name,
                render_optional_status_name(step.watchdog_cancel_status)
            ));
            output.push_str(&format!(
                "  {} value set status name: {}\n",
                step.name,
                render_optional_status_name(step.value_set_status)
            ));
            output.push_str(&format!(
                "  {} PC read status name: {}\n",
                step.name,
                render_optional_status_name(step.pc_read_status)
            ));
            output.push_str(&format!(
                "  {} PC after exit: {}\n",
                step.name,
                render_optional_u64(step.pc_after_exit)
            ));
            output.push_str(&format!(
                "  {} PC advance status name: {}\n",
                step.name,
                render_optional_status_name(step.pc_advance_status)
            ));
        }
        output.push_str(&format!(
            "Continuation run attempted: {}\n",
            self.continuation_run_attempted
        ));
        output.push_str(&format!(
            "Continuation exit observed: {}\n",
            self.continuation_exit_observed
        ));
        output.push_str(&format!(
            "Capacity high value preserved: {}\n",
            self.capacity_high_value_preserved
        ));
        output.push_str(&format!(
            "Watchdog cancel fired: {}\n",
            self.watchdog_cancel_fired
        ));
        output.push_str(&format!("vCPU destroyed: {}\n", self.vcpu_destroyed));
        output.push_str(&format!("Memory unmapped: {}\n", self.memory_unmapped));
        output.push_str(&format!("VM destroyed: {}\n", self.vm_destroyed));
        output.push_str(&format!(
            "Memory deallocated: {}\n",
            self.memory_deallocated
        ));
        output.push_str(&format!("Code IPA start: {:#x}\n", self.code_ipa_start));
        output.push_str(&format!("Block IPA: {:#x}\n", self.block_ipa));
        output.push_str(&format!("Bytes: {}\n", self.bytes));
        output.push_str(&format!("Instructions: {}\n", self.instructions));
        output.push_str(&format!(
            "Device features value: {:#x}\n",
            self.device_features_value
        ));
        output.push_str(&format!(
            "Driver features value: {:#x}\n",
            self.driver_features_value
        ));
        output.push_str(&format!(
            "Queue select value: {:#x}\n",
            self.queue_select_value
        ));
        output.push_str(&format!(
            "Queue num max value: {:#x}\n",
            self.queue_num_max_value
        ));
        output.push_str(&format!("Queue num value: {:#x}\n", self.queue_num_value));
        output.push_str(&format!(
            "Queue ready value: {:#x}\n",
            self.queue_ready_value
        ));
        output.push_str(&format!(
            "Queue descriptor address: {:#x}\n",
            self.queue_desc_address
        ));
        output.push_str(&format!(
            "Queue driver address: {:#x}\n",
            self.queue_driver_address
        ));
        output.push_str(&format!(
            "Queue device address: {:#x}\n",
            self.queue_device_address
        ));
        output.push_str(&format!(
            "Queue notify value: {:#x}\n",
            self.queue_notify_value
        ));
        output.push_str(&format!(
            "Interrupt status value: {:#x}\n",
            self.interrupt_status_value
        ));
        output.push_str(&format!(
            "Block backing kind: {}\n",
            self.block_backing_kind
        ));
        output.push_str(&format!(
            "Block backing path: {}\n",
            self.block_backing_path.as_ref().map_or_else(
                || "not observed".to_string(),
                |path| path.display().to_string()
            )
        ));
        output.push_str(&format!(
            "Request ring seeded: {}\n",
            self.request_ring_seeded
        ));
        output.push_str(&format!(
            "Request completed after notify: {}\n",
            self.request_completed_after_notify
        ));
        output.push_str(&format!(
            "Request descriptor index: {}\n",
            render_optional_u64(self.request_descriptor_index.map(u64::from))
        ));
        output.push_str(&format!(
            "Request sector: {}\n",
            render_optional_u64(self.request_sector)
        ));
        output.push_str(&format!(
            "Request byte offset: {}\n",
            render_optional_u64(self.request_byte_offset)
        ));
        output.push_str(&format!(
            "Request data bytes: {}\n",
            render_optional_u64(self.request_data_bytes.map(u64::from))
        ));
        output.push_str(&format!(
            "Request data prefix: {}\n",
            render_hex_bytes(&self.request_data_prefix)
        ));
        output.push_str(&format!(
            "Request status byte: {}\n",
            render_optional_u64(self.request_status.map(u64::from))
        ));
        output.push_str(&format!(
            "Request used index: {}\n",
            render_optional_u64(self.request_used_index.map(u64::from))
        ));
        output.push_str(&format!(
            "Request used length: {}\n",
            render_optional_u64(self.request_used_len.map(u64::from))
        ));
        output.push_str(&format!(
            "Request interrupt status: {}\n",
            render_optional_u64(self.request_interrupt_status)
        ));
        output.push_str(&format!(
            "Write completed after notify: {}\n",
            self.write_completed_after_notify
        ));
        output.push_str(&format!(
            "Write request type: {}\n",
            render_optional_u64(self.write_request_type.map(u64::from))
        ));
        output.push_str(&format!(
            "Write sector: {}\n",
            render_optional_u64(self.write_sector)
        ));
        output.push_str(&format!(
            "Write byte offset: {}\n",
            render_optional_u64(self.write_byte_offset)
        ));
        output.push_str(&format!(
            "Write data bytes: {}\n",
            render_optional_u64(self.write_data_bytes.map(u64::from))
        ));
        output.push_str(&format!(
            "Write data prefix: {}\n",
            render_hex_bytes(&self.write_data_prefix)
        ));
        output.push_str(&format!(
            "Write status byte: {}\n",
            render_optional_u64(self.write_status.map(u64::from))
        ));
        output.push_str(&format!(
            "Write used index: {}\n",
            render_optional_u64(self.write_used_index.map(u64::from))
        ));
        output.push_str(&format!(
            "Write used length: {}\n",
            render_optional_u64(self.write_used_len.map(u64::from))
        ));
        output.push_str(&format!(
            "Flush completed after notify: {}\n",
            self.flush_completed_after_notify
        ));
        output.push_str(&format!(
            "Flush request type: {}\n",
            render_optional_u64(self.flush_request_type.map(u64::from))
        ));
        output.push_str(&format!(
            "Flush status byte: {}\n",
            render_optional_u64(self.flush_status.map(u64::from))
        ));
        output.push_str(&format!(
            "Flush used index: {}\n",
            render_optional_u64(self.flush_used_index.map(u64::from))
        ));
        output.push_str(&format!(
            "Flush used length: {}\n",
            render_optional_u64(self.flush_used_len.map(u64::from))
        ));
        output.push_str(&format!(
            "Persisted data prefix: {}\n",
            render_hex_bytes(&self.persisted_data_prefix)
        ));
        output.push_str(&format!("Status value: {:#x}\n", self.status_value));
        output.push_str(&format!("Capacity sectors: {:#x}\n", self.capacity_sectors));
        output.push_str(&format!(
            "VM create status name: {}\n",
            render_optional_status_name(self.vm_create_status)
        ));
        output.push_str(&format!(
            "Allocate status name: {}\n",
            render_optional_status_name(self.allocate_status)
        ));
        output.push_str(&format!(
            "Map status name: {}\n",
            render_optional_status_name(self.map_status)
        ));
        output.push_str(&format!(
            "vCPU create status name: {}\n",
            render_optional_status_name(self.vcpu_create_status)
        ));
        output.push_str(&format!(
            "PC set status name: {}\n",
            render_optional_status_name(self.pc_set_status)
        ));
        output.push_str(&format!(
            "CPSR set status name: {}\n",
            render_optional_status_name(self.cpsr_set_status)
        ));
        output.push_str(&format!(
            "Continuation run status name: {}\n",
            render_optional_status_name(self.continuation_run_status)
        ));
        output.push_str(&format!(
            "Continuation exit reason name: {}\n",
            render_optional_exit_reason_name(self.continuation_exit_reason)
        ));
        output.push_str(&format!(
            "Continuation exit syndrome: {}\n",
            render_optional_u64(self.continuation_exit_syndrome)
        ));
        output.push_str(&format!(
            "Continuation exit virtual address: {}\n",
            render_optional_u64(self.continuation_exit_virtual_address)
        ));
        output.push_str(&format!(
            "Continuation exit physical address: {}\n",
            render_optional_u64(self.continuation_exit_physical_address)
        ));
        output.push_str(&format!(
            "Continuation watchdog cancel status name: {}\n",
            render_optional_status_name(self.continuation_watchdog_cancel_status)
        ));
        output.push_str(&format!(
            "Capacity high after continue status name: {}\n",
            render_optional_status_name(self.capacity_high_after_continue_status)
        ));
        output.push_str(&format!(
            "Capacity high after continue: {}\n",
            render_optional_u64(self.capacity_high_after_continue)
        ));
        output.push_str(&format!(
            "vCPU destroy status name: {}\n",
            render_optional_status_name(self.vcpu_destroy_status)
        ));
        output.push_str(&format!(
            "Unmap status name: {}\n",
            render_optional_status_name(self.unmap_status)
        ));
        output.push_str(&format!(
            "VM destroy status name: {}\n",
            render_optional_status_name(self.vm_destroy_status)
        ));
        output.push_str(&format!(
            "Deallocate status name: {}\n",
            render_optional_status_name(self.deallocate_status)
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

pub fn probe_hvf_mmio_block_queue(
    allow_device: bool,
    disk_path: Option<PathBuf>,
    iso_path: Option<PathBuf>,
    writable_disk_path: Option<PathBuf>,
) -> HvfMmioBlockQueueProbe {
    let host = query_hvf_host_capabilities();
    platform::probe_hvf_mmio_block_queue(
        allow_device,
        disk_path,
        iso_path,
        writable_disk_path,
        host,
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VirtioBlockRequestModelProbe {
    pub configured_via_mmio: bool,
    pub configured_via_mmio_bus: bool,
    pub queue_notified: bool,
    pub queue_notify_value: Option<u64>,
    pub completed_via_device_bus: bool,
    pub completed: bool,
    pub descriptor_index: Option<u16>,
    pub request_type: Option<u32>,
    pub sector: Option<u64>,
    pub data_bytes: Option<u32>,
    pub data_prefix: Vec<u8>,
    pub status: Option<u8>,
    pub used_index: Option<u16>,
    pub used_len: Option<u32>,
    pub interrupt_status: Option<u64>,
    pub blockers: Vec<String>,
}

impl VirtioBlockRequestModelProbe {
    pub fn render_text(&self) -> String {
        let mut output = String::new();
        output.push_str("VirtIO block request model probe\n");
        output.push_str("QEMU: not used\n");
        output.push_str("Apple VZ: not used\n");
        output.push_str("HVF: not entered\n");
        output.push_str("Guest execution: not entered; in-memory VirtIO block descriptor chain\n");
        output.push_str(&format!(
            "Configured via MMIO: {}\n",
            self.configured_via_mmio
        ));
        output.push_str(&format!(
            "Configured via MMIO bus: {}\n",
            self.configured_via_mmio_bus
        ));
        output.push_str(&format!("Queue notified: {}\n", self.queue_notified));
        output.push_str(&format!(
            "Queue notify value: {}\n",
            render_optional_u64(self.queue_notify_value)
        ));
        output.push_str(&format!(
            "Completed via device bus: {}\n",
            self.completed_via_device_bus
        ));
        output.push_str(&format!("Completed: {}\n", self.completed));
        output.push_str(&format!(
            "Descriptor index: {}\n",
            render_optional_u64(self.descriptor_index.map(u64::from))
        ));
        output.push_str(&format!(
            "Request type: {}\n",
            render_optional_u64(self.request_type.map(u64::from))
        ));
        output.push_str(&format!("Sector: {}\n", render_optional_u64(self.sector)));
        output.push_str(&format!(
            "Data bytes: {}\n",
            render_optional_u64(self.data_bytes.map(u64::from))
        ));
        output.push_str(&format!(
            "Data prefix: {}\n",
            render_hex_bytes(&self.data_prefix)
        ));
        output.push_str(&format!(
            "Status byte: {}\n",
            render_optional_u64(self.status.map(u64::from))
        ));
        output.push_str(&format!(
            "Used index: {}\n",
            render_optional_u64(self.used_index.map(u64::from))
        ));
        output.push_str(&format!(
            "Used length: {}\n",
            render_optional_u64(self.used_len.map(u64::from))
        ));
        output.push_str(&format!(
            "Interrupt status: {}\n",
            render_optional_u64(self.interrupt_status)
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

pub fn probe_virtio_block_request_model() -> VirtioBlockRequestModelProbe {
    match run_virtio_block_request_model() {
        Ok(probe) => probe,
        Err(error) => VirtioBlockRequestModelProbe {
            configured_via_mmio: false,
            configured_via_mmio_bus: false,
            queue_notified: false,
            queue_notify_value: None,
            completed_via_device_bus: false,
            completed: false,
            descriptor_index: None,
            request_type: None,
            sector: None,
            data_bytes: None,
            data_prefix: Vec::new(),
            status: None,
            used_index: None,
            used_len: None,
            interrupt_status: None,
            blockers: vec![error.render_blocker()],
        },
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VirtioBlockFileBackingProbe {
    pub disk_path: PathBuf,
    pub backing_kind: &'static str,
    pub configured_via_mmio: bool,
    pub configured_via_mmio_bus: bool,
    pub queue_notified: bool,
    pub queue_notify_value: Option<u64>,
    pub completed_via_device_bus: bool,
    pub completed: bool,
    pub descriptor_index: Option<u16>,
    pub request_type: Option<u32>,
    pub sector: Option<u64>,
    pub byte_offset: Option<u64>,
    pub data_bytes: Option<u32>,
    pub data_prefix: Vec<u8>,
    pub status: Option<u8>,
    pub used_index: Option<u16>,
    pub used_len: Option<u32>,
    pub interrupt_status: Option<u64>,
    pub blockers: Vec<String>,
}

impl VirtioBlockFileBackingProbe {
    pub fn render_text(&self) -> String {
        let mut output = String::new();
        output.push_str("VirtIO block file backing probe\n");
        output.push_str("QEMU: not used\n");
        output.push_str("Apple VZ: not used\n");
        output.push_str("HVF: not entered\n");
        output.push_str(
            "Guest execution: not entered; host file-backed VirtIO block descriptor chain\n",
        );
        output.push_str(&format!("Disk path: {}\n", self.disk_path.display()));
        output.push_str(&format!("Backing kind: {}\n", self.backing_kind));
        output.push_str(&format!(
            "Configured via MMIO: {}\n",
            self.configured_via_mmio
        ));
        output.push_str(&format!(
            "Configured via MMIO bus: {}\n",
            self.configured_via_mmio_bus
        ));
        output.push_str(&format!("Queue notified: {}\n", self.queue_notified));
        output.push_str(&format!(
            "Queue notify value: {}\n",
            render_optional_u64(self.queue_notify_value)
        ));
        output.push_str(&format!(
            "Completed via device bus: {}\n",
            self.completed_via_device_bus
        ));
        output.push_str(&format!("Completed: {}\n", self.completed));
        output.push_str(&format!(
            "Descriptor index: {}\n",
            render_optional_u64(self.descriptor_index.map(u64::from))
        ));
        output.push_str(&format!(
            "Request type: {}\n",
            render_optional_u64(self.request_type.map(u64::from))
        ));
        output.push_str(&format!("Sector: {}\n", render_optional_u64(self.sector)));
        output.push_str(&format!(
            "Byte offset: {}\n",
            render_optional_u64(self.byte_offset)
        ));
        output.push_str(&format!(
            "Data bytes: {}\n",
            render_optional_u64(self.data_bytes.map(u64::from))
        ));
        output.push_str(&format!(
            "Data prefix: {}\n",
            render_hex_bytes(&self.data_prefix)
        ));
        output.push_str(&format!(
            "Status byte: {}\n",
            render_optional_u64(self.status.map(u64::from))
        ));
        output.push_str(&format!(
            "Used index: {}\n",
            render_optional_u64(self.used_index.map(u64::from))
        ));
        output.push_str(&format!(
            "Used length: {}\n",
            render_optional_u64(self.used_len.map(u64::from))
        ));
        output.push_str(&format!(
            "Interrupt status: {}\n",
            render_optional_u64(self.interrupt_status)
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

pub fn probe_virtio_block_file_backing(disk_path: PathBuf) -> VirtioBlockFileBackingProbe {
    match run_virtio_block_file_backing(disk_path.clone()) {
        Ok(probe) => probe,
        Err(error) => VirtioBlockFileBackingProbe {
            disk_path,
            backing_kind: "host-file",
            configured_via_mmio: false,
            configured_via_mmio_bus: false,
            queue_notified: false,
            queue_notify_value: None,
            completed_via_device_bus: false,
            completed: false,
            descriptor_index: None,
            request_type: None,
            sector: None,
            byte_offset: None,
            data_bytes: None,
            data_prefix: Vec::new(),
            status: None,
            used_index: None,
            used_len: None,
            interrupt_status: None,
            blockers: vec![error.render_blocker()],
        },
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VirtioBlockWritableFileBackingProbe {
    pub disk_path: PathBuf,
    pub backing_kind: &'static str,
    pub configured_via_mmio: bool,
    pub configured_via_mmio_bus: bool,
    pub queue_notified: bool,
    pub queue_notify_value: Option<u64>,
    pub initial_read_prefix: Vec<u8>,
    pub write_completed: bool,
    pub write_request_type: Option<u32>,
    pub write_sector: Option<u64>,
    pub write_byte_offset: Option<u64>,
    pub write_data_bytes: Option<u32>,
    pub write_data_prefix: Vec<u8>,
    pub write_status: Option<u8>,
    pub write_used_index: Option<u16>,
    pub write_used_len: Option<u32>,
    pub flush_completed: bool,
    pub flush_request_type: Option<u32>,
    pub flush_status: Option<u8>,
    pub flush_used_index: Option<u16>,
    pub flush_used_len: Option<u32>,
    pub persisted_data_prefix: Vec<u8>,
    pub interrupt_status: Option<u64>,
    pub blockers: Vec<String>,
}

impl VirtioBlockWritableFileBackingProbe {
    pub fn render_text(&self) -> String {
        let mut output = String::new();
        output.push_str("VirtIO block writable file backing probe\n");
        output.push_str("QEMU: not used\n");
        output.push_str("Apple VZ: not used\n");
        output.push_str("HVF: not entered\n");
        output.push_str(
            "Guest execution: not entered; host file-backed VirtIO block write/flush persistence descriptor chain\n",
        );
        output.push_str(&format!("Disk path: {}\n", self.disk_path.display()));
        output.push_str(&format!("Backing kind: {}\n", self.backing_kind));
        output.push_str(&format!(
            "Configured via MMIO: {}\n",
            self.configured_via_mmio
        ));
        output.push_str(&format!(
            "Configured via MMIO bus: {}\n",
            self.configured_via_mmio_bus
        ));
        output.push_str(&format!("Queue notified: {}\n", self.queue_notified));
        output.push_str(&format!(
            "Queue notify value: {}\n",
            render_optional_u64(self.queue_notify_value)
        ));
        output.push_str(&format!(
            "Initial read data prefix: {}\n",
            render_hex_bytes(&self.initial_read_prefix)
        ));
        output.push_str(&format!("Write completed: {}\n", self.write_completed));
        output.push_str(&format!(
            "Write request type: {}\n",
            render_optional_u64(self.write_request_type.map(u64::from))
        ));
        output.push_str(&format!(
            "Write sector: {}\n",
            render_optional_u64(self.write_sector)
        ));
        output.push_str(&format!(
            "Write byte offset: {}\n",
            render_optional_u64(self.write_byte_offset)
        ));
        output.push_str(&format!(
            "Write data bytes: {}\n",
            render_optional_u64(self.write_data_bytes.map(u64::from))
        ));
        output.push_str(&format!(
            "Write data prefix: {}\n",
            render_hex_bytes(&self.write_data_prefix)
        ));
        output.push_str(&format!(
            "Write status byte: {}\n",
            render_optional_u64(self.write_status.map(u64::from))
        ));
        output.push_str(&format!(
            "Write used index: {}\n",
            render_optional_u64(self.write_used_index.map(u64::from))
        ));
        output.push_str(&format!(
            "Write used length: {}\n",
            render_optional_u64(self.write_used_len.map(u64::from))
        ));
        output.push_str(&format!("Flush completed: {}\n", self.flush_completed));
        output.push_str(&format!(
            "Flush request type: {}\n",
            render_optional_u64(self.flush_request_type.map(u64::from))
        ));
        output.push_str(&format!(
            "Flush status byte: {}\n",
            render_optional_u64(self.flush_status.map(u64::from))
        ));
        output.push_str(&format!(
            "Flush used index: {}\n",
            render_optional_u64(self.flush_used_index.map(u64::from))
        ));
        output.push_str(&format!(
            "Flush used length: {}\n",
            render_optional_u64(self.flush_used_len.map(u64::from))
        ));
        output.push_str(&format!(
            "Persisted data prefix: {}\n",
            render_hex_bytes(&self.persisted_data_prefix)
        ));
        output.push_str(&format!(
            "Interrupt status: {}\n",
            render_optional_u64(self.interrupt_status)
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

pub fn probe_virtio_block_writable_file_backing(
    disk_path: PathBuf,
) -> VirtioBlockWritableFileBackingProbe {
    match run_virtio_block_writable_file_backing(disk_path.clone()) {
        Ok(probe) => probe,
        Err(error) => VirtioBlockWritableFileBackingProbe {
            disk_path,
            backing_kind: "host-file-writable",
            configured_via_mmio: false,
            configured_via_mmio_bus: false,
            queue_notified: false,
            queue_notify_value: None,
            initial_read_prefix: Vec::new(),
            write_completed: false,
            write_request_type: None,
            write_sector: None,
            write_byte_offset: None,
            write_data_bytes: None,
            write_data_prefix: Vec::new(),
            write_status: None,
            write_used_index: None,
            write_used_len: None,
            flush_completed: false,
            flush_request_type: None,
            flush_status: None,
            flush_used_index: None,
            flush_used_len: None,
            persisted_data_prefix: Vec::new(),
            interrupt_status: None,
            blockers: vec![error.render_blocker()],
        },
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VirtioBlockIsoBackingProbe {
    pub iso_path: PathBuf,
    pub backing_kind: &'static str,
    pub media_mode: &'static str,
    pub configured_via_mmio: bool,
    pub configured_via_mmio_bus: bool,
    pub queue_notified: bool,
    pub queue_notify_value: Option<u64>,
    pub completed_via_device_bus: bool,
    pub completed: bool,
    pub descriptor_index: Option<u16>,
    pub request_type: Option<u32>,
    pub sector: Option<u64>,
    pub byte_offset: Option<u64>,
    pub data_bytes: Option<u32>,
    pub data_prefix: Vec<u8>,
    pub status: Option<u8>,
    pub used_index: Option<u16>,
    pub used_len: Option<u32>,
    pub interrupt_status: Option<u64>,
    pub readonly_write_rejected: bool,
    pub readonly_write_status: Option<u8>,
    pub readonly_write_used_index: Option<u16>,
    pub readonly_write_used_len: Option<u32>,
    pub readonly_write_interrupt_status: Option<u64>,
    pub blockers: Vec<String>,
}

impl VirtioBlockIsoBackingProbe {
    pub fn render_text(&self) -> String {
        let mut output = String::new();
        output.push_str("VirtIO block ISO backing probe\n");
        output.push_str("QEMU: not used\n");
        output.push_str("Apple VZ: not used\n");
        output.push_str("HVF: not entered\n");
        output.push_str(
            "Guest execution: not entered; read-only ISO-backed VirtIO block descriptor chain\n",
        );
        output.push_str(&format!("ISO path: {}\n", self.iso_path.display()));
        output.push_str(&format!("Backing kind: {}\n", self.backing_kind));
        output.push_str(&format!("Media mode: {}\n", self.media_mode));
        output.push_str(&format!(
            "Configured via MMIO: {}\n",
            self.configured_via_mmio
        ));
        output.push_str(&format!(
            "Configured via MMIO bus: {}\n",
            self.configured_via_mmio_bus
        ));
        output.push_str(&format!("Queue notified: {}\n", self.queue_notified));
        output.push_str(&format!(
            "Queue notify value: {}\n",
            render_optional_u64(self.queue_notify_value)
        ));
        output.push_str(&format!(
            "Completed via device bus: {}\n",
            self.completed_via_device_bus
        ));
        output.push_str(&format!("Completed: {}\n", self.completed));
        output.push_str(&format!(
            "Descriptor index: {}\n",
            render_optional_u64(self.descriptor_index.map(u64::from))
        ));
        output.push_str(&format!(
            "Request type: {}\n",
            render_optional_u64(self.request_type.map(u64::from))
        ));
        output.push_str(&format!("Sector: {}\n", render_optional_u64(self.sector)));
        output.push_str(&format!(
            "Byte offset: {}\n",
            render_optional_u64(self.byte_offset)
        ));
        output.push_str(&format!(
            "Data bytes: {}\n",
            render_optional_u64(self.data_bytes.map(u64::from))
        ));
        output.push_str(&format!(
            "Data prefix: {}\n",
            render_hex_bytes(&self.data_prefix)
        ));
        output.push_str(&format!(
            "Status byte: {}\n",
            render_optional_u64(self.status.map(u64::from))
        ));
        output.push_str(&format!(
            "Used index: {}\n",
            render_optional_u64(self.used_index.map(u64::from))
        ));
        output.push_str(&format!(
            "Used length: {}\n",
            render_optional_u64(self.used_len.map(u64::from))
        ));
        output.push_str(&format!(
            "Interrupt status: {}\n",
            render_optional_u64(self.interrupt_status)
        ));
        output.push_str(&format!(
            "Read-only write rejected: {}\n",
            self.readonly_write_rejected
        ));
        output.push_str(&format!(
            "Read-only write status byte: {}\n",
            render_optional_u64(self.readonly_write_status.map(u64::from))
        ));
        output.push_str(&format!(
            "Read-only write used index: {}\n",
            render_optional_u64(self.readonly_write_used_index.map(u64::from))
        ));
        output.push_str(&format!(
            "Read-only write used length: {}\n",
            render_optional_u64(self.readonly_write_used_len.map(u64::from))
        ));
        output.push_str(&format!(
            "Read-only write interrupt status: {}\n",
            render_optional_u64(self.readonly_write_interrupt_status)
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

pub fn probe_virtio_block_iso_backing(iso_path: PathBuf) -> VirtioBlockIsoBackingProbe {
    match run_virtio_block_iso_backing(iso_path.clone()) {
        Ok(probe) => probe,
        Err(error) => VirtioBlockIsoBackingProbe {
            iso_path,
            backing_kind: "host-iso-readonly",
            media_mode: "read-only",
            configured_via_mmio: false,
            configured_via_mmio_bus: false,
            queue_notified: false,
            queue_notify_value: None,
            completed_via_device_bus: false,
            completed: false,
            descriptor_index: None,
            request_type: None,
            sector: None,
            byte_offset: None,
            data_bytes: None,
            data_prefix: Vec::new(),
            status: None,
            used_index: None,
            used_len: None,
            interrupt_status: None,
            readonly_write_rejected: false,
            readonly_write_status: None,
            readonly_write_used_index: None,
            readonly_write_used_len: None,
            readonly_write_interrupt_status: None,
            blockers: vec![error.render_blocker()],
        },
    }
}

pub const WINDOWS_ARM_BOOT_DISK_DEFAULT_SIZE_GIB: u32 = 64;
pub const WINDOWS_ARM_BOOT_DISK_MIN_SIZE_GIB: u32 = 8;
pub const WINDOWS_ARM_UEFI_LOW_CODE_ALIAS_IPA: u64 = 0x0000_0000;
pub const WINDOWS_ARM_UEFI_LOW_VARS_ALIAS_IPA: u64 = 0x0400_0000;
pub const WINDOWS_ARM_UEFI_CODE_IPA: u64 = 0x0800_0000;
pub const WINDOWS_ARM_UEFI_VARS_IPA: u64 = 0x0c00_0000;
pub const WINDOWS_ARM_UEFI_SLOT_BYTES: u64 = 64 * 1024 * 1024;
pub const WINDOWS_ARM_UEFI_PFLASH_BYTES: u64 = WINDOWS_ARM_UEFI_SLOT_BYTES * 2;
pub const WINDOWS_ARM_DEVICE_MMIO_IPA: u64 = 0x1000_0000;
pub const WINDOWS_ARM_DEVICE_MMIO_BYTES: u64 = 0x1000_0000;
const WINDOWS_ARM_PL011_MMIO_IPA: u64 = WINDOWS_ARM_DEVICE_MMIO_IPA;
const WINDOWS_ARM_PL031_MMIO_IPA: u64 = WINDOWS_ARM_DEVICE_MMIO_IPA + 0x1000;
const WINDOWS_ARM_VIRTIO_INSTALLER_ISO_MMIO_IPA: u64 = WINDOWS_ARM_DEVICE_MMIO_IPA + 0x2000;
const WINDOWS_ARM_VIRTIO_TARGET_DISK_MMIO_IPA: u64 = WINDOWS_ARM_DEVICE_MMIO_IPA + 0x3000;
const WINDOWS_ARM_GIC_DISTRIBUTOR_MMIO_IPA: u64 = WINDOWS_ARM_DEVICE_MMIO_IPA + 0x1_0000;
const WINDOWS_ARM_GIC_REDISTRIBUTOR_MMIO_IPA: u64 = WINDOWS_ARM_DEVICE_MMIO_IPA + 0x2_0000;
const WINDOWS_ARM_GIC_DISTRIBUTOR_BYTES: u64 = 0x1_0000;
const WINDOWS_ARM_GIC_REDISTRIBUTOR_BYTES: u64 = 0x2_0000;
const WINDOWS_ARM_GIC_PHANDLE: u32 = 1;
const GIC_SPI: u32 = 0;
const GIC_PPI: u32 = 1;
const IRQ_TYPE_LEVEL_HIGH: u32 = 4;
// FDT GIC SPI interrupt numbers are encoded as the global IRQ minus 32.
const WINDOWS_ARM_PL011_SPI: u32 = 0;
const WINDOWS_ARM_PL031_SPI: u32 = 1;
const WINDOWS_ARM_VIRTIO_INSTALLER_ISO_SPI: u32 = 2;
const WINDOWS_ARM_VIRTIO_TARGET_DISK_SPI: u32 = 3;
const WINDOWS_ARM_PL011_FLAG_VALUE: u64 = 0x90;
const WINDOWS_ARM_PL031_READ_VALUE: u64 = 0x2026_0618;
pub const WINDOWS_ARM_GUEST_RAM_IPA: u64 = 0x4000_0000;
pub const WINDOWS_ARM_EXECUTABLE_DIAGNOSTIC_VECTOR_IPA: u64 = 0x0020_0000;
pub const WINDOWS_ARM_DIAGNOSTIC_VECTOR_IPA: u64 = WINDOWS_ARM_UEFI_CODE_IPA;
pub const WINDOWS_ARM_GUEST_RAM_DIAGNOSTIC_VECTOR_IPA: u64 = WINDOWS_ARM_GUEST_RAM_IPA;
pub const WINDOWS_ARM_DIAGNOSTIC_VECTOR_BYTES: u64 = 0x800;
const WINDOWS_ARM_PLATFORM_DTB_GUEST_RAM_OFFSET: u64 = 0x0001_0000;
const WINDOWS_ARM_PLATFORM_DTB_IPA: u64 =
    WINDOWS_ARM_GUEST_RAM_IPA + WINDOWS_ARM_PLATFORM_DTB_GUEST_RAM_OFFSET;
const WINDOWS_ARM_FIRMWARE_RUN_LOOP_FDT_VCPU_COUNT: u8 = 1;
const WINDOWS_ARM_DIAGNOSTIC_VECTOR_CURRENT_EL_SPX_SYNC_OFFSET: usize = 0x200;
const AARCH64_STAGE1_PAGE_OUTPUT_ADDRESS_MASK: u64 = 0x0000_ffff_ffff_f000;
const WINDOWS_ARM_LOW_VECTOR_DIAGNOSTIC_PAGE_DESCRIPTOR: u64 = 0xf8f;
const AARCH64_HVC_0_INSTRUCTION: u32 = 0xd400_0002;
const AARCH64_HVC_1_INSTRUCTION: u32 = 0xd400_0022;
const AARCH64_ERET_INSTRUCTION: u32 = 0xd69f_03e0;
const WINDOWS_ARM_PLATFORM_DESCRIPTION_DEFAULT_GUEST_RAM_BYTES: u64 = 6 * 1024 * 1024 * 1024;
const WINDOWS_ARM_PLATFORM_DESCRIPTION_DEFAULT_VCPU_COUNT: u8 = 4;
const FDT_MAGIC: u32 = 0xd00d_feed;
const FDT_BEGIN_NODE: u32 = 1;
const FDT_END_NODE: u32 = 2;
const FDT_PROP: u32 = 3;
const FDT_END: u32 = 9;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct WindowsArmDiagnosticVectorSelection {
    requested: bool,
    location: &'static str,
    ipa: u64,
}

fn windows_arm_diagnostic_vector_selection(
    seed_diagnostic_vector: bool,
    seed_guest_ram_diagnostic_vector: bool,
    seed_executable_diagnostic_vector: bool,
) -> WindowsArmDiagnosticVectorSelection {
    let requested = seed_diagnostic_vector
        || seed_guest_ram_diagnostic_vector
        || seed_executable_diagnostic_vector;
    if seed_executable_diagnostic_vector {
        return WindowsArmDiagnosticVectorSelection {
            requested,
            location: "low-pflash-executable-candidate",
            ipa: WINDOWS_ARM_EXECUTABLE_DIAGNOSTIC_VECTOR_IPA,
        };
    }
    if seed_guest_ram_diagnostic_vector {
        return WindowsArmDiagnosticVectorSelection {
            requested,
            location: "guest-ram",
            ipa: WINDOWS_ARM_GUEST_RAM_DIAGNOSTIC_VECTOR_IPA,
        };
    }
    WindowsArmDiagnosticVectorSelection {
        requested,
        location: "pflash",
        ipa: WINDOWS_ARM_DIAGNOSTIC_VECTOR_IPA,
    }
}

fn windows_arm_vector_slot_instruction_is_populated(word: Option<u32>) -> bool {
    !matches!(word, None | Some(0) | Some(0xffff_ffff))
}

fn windows_arm_instruction_is_bridgevm_diagnostic_vector_word(word: u32) -> bool {
    matches!(
        word,
        AARCH64_HVC_0_INSTRUCTION | AARCH64_HVC_1_INSTRUCTION | AARCH64_ERET_INSTRUCTION
    )
}

fn windows_arm_vector_slot_instruction_is_non_diagnostic_populated(word: Option<u32>) -> bool {
    match word {
        Some(word) => {
            windows_arm_vector_slot_instruction_is_populated(Some(word))
                && !windows_arm_instruction_is_bridgevm_diagnostic_vector_word(word)
        }
        None => false,
    }
}

fn windows_arm_gic_redistributor_fdt_bytes(vcpu_count: u8) -> u64 {
    WINDOWS_ARM_GIC_REDISTRIBUTOR_BYTES * u64::from(vcpu_count.max(1))
}

const GPT_SECTOR_BYTES: u64 = 512;
const GPT_SECTOR_BYTES_USIZE: usize = GPT_SECTOR_BYTES as usize;
const GPT_ENTRY_COUNT: usize = 128;
const GPT_ENTRY_SIZE: usize = 128;
const GPT_ENTRY_ARRAY_BYTES: usize = GPT_ENTRY_COUNT * GPT_ENTRY_SIZE;
const GPT_ENTRY_ARRAY_SECTORS: u64 = (GPT_ENTRY_ARRAY_BYTES as u64) / GPT_SECTOR_BYTES;
const GPT_PRIMARY_HEADER_LBA: u64 = 1;
const GPT_PRIMARY_ENTRY_LBA: u64 = 2;
const GPT_FIRST_USABLE_LBA: u64 = GPT_PRIMARY_ENTRY_LBA + GPT_ENTRY_ARRAY_SECTORS;
const WINDOWS_ARM_BOOT_DISK_ALIGNMENT_LBA: u64 = 2048;
const WINDOWS_ARM_ESP_SIZE_BYTES: u64 = 260 * 1024 * 1024;
const WINDOWS_ARM_MSR_SIZE_BYTES: u64 = 16 * 1024 * 1024;
const EFI_SYSTEM_PARTITION_GUID: [u8; 16] = [
    0x28, 0x73, 0x2a, 0xc1, 0x1f, 0xf8, 0xd2, 0x11, 0xba, 0x4b, 0x00, 0xa0, 0xc9, 0x3e, 0xc9, 0x3b,
];
const MICROSOFT_RESERVED_PARTITION_GUID: [u8; 16] = [
    0x16, 0xe3, 0xc9, 0xe3, 0x5c, 0x0b, 0xb8, 0x4d, 0x81, 0x7d, 0xf9, 0x2d, 0xf0, 0x02, 0x15, 0xae,
];
const MICROSOFT_BASIC_DATA_PARTITION_GUID: [u8; 16] = [
    0xa2, 0xa0, 0xd0, 0xeb, 0xe5, 0xb9, 0x33, 0x44, 0x87, 0xc0, 0x68, 0xb6, 0xb7, 0x26, 0x99, 0xc7,
];
const UEFI_FV_SIGNATURE_OFFSET: usize = 0x28;
const UEFI_FV_LENGTH_OFFSET: usize = 0x20;
const UEFI_FV_HEADER_LENGTH_OFFSET: usize = 0x30;
const UEFI_FV_MIN_HEADER_BYTES: usize = 0x38;
const UEFI_FV_SIGNATURE: &[u8; 4] = b"_FVH";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsArmPlatformDescriptionOptions {
    pub guest_ram_bytes: u64,
    pub vcpu_count: u8,
}

impl Default for WindowsArmPlatformDescriptionOptions {
    fn default() -> Self {
        Self {
            guest_ram_bytes: WINDOWS_ARM_PLATFORM_DESCRIPTION_DEFAULT_GUEST_RAM_BYTES,
            vcpu_count: WINDOWS_ARM_PLATFORM_DESCRIPTION_DEFAULT_VCPU_COUNT,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsArmFdtMmioNodeCheck {
    pub label: &'static str,
    pub node_name: &'static str,
    pub base_ipa: Option<u64>,
    pub bytes: Option<u64>,
    pub inside_device_window: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsArmFdtInterruptCheck {
    pub label: &'static str,
    pub node_name: &'static str,
    pub interrupt_type: Option<u32>,
    pub interrupt_number: Option<u32>,
    pub trigger: Option<u32>,
    pub described: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsArmVirtioBlockDeviceMetadata {
    pub role: &'static str,
    pub label: &'static str,
    pub node_name: &'static str,
    pub base_ipa: u64,
    pub bytes: u64,
    pub read_only: bool,
    pub backing_kind: &'static str,
    pub backing_path: Option<PathBuf>,
    pub device_features: u64,
    pub capacity_sectors: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsArmPlatformDescriptionProbe {
    pub qemu_used: bool,
    pub apple_vz_used: bool,
    pub hvf_entered: bool,
    pub format: &'static str,
    pub fdt_blob: Vec<u8>,
    pub fdt_blob_bytes: usize,
    pub fdt_magic: u32,
    pub fdt_magic_verified: bool,
    pub memory_node_base_ipa: Option<u64>,
    pub memory_node_at_guest_ram_base: bool,
    pub requested_cpu_count: u8,
    pub cpu_count: u8,
    pub cpu_count_verified: bool,
    pub device_mmio_start_ipa: u64,
    pub device_mmio_end_ipa: u64,
    pub mmio_nodes: Vec<WindowsArmFdtMmioNodeCheck>,
    pub mmio_nodes_inside_device_window: bool,
    pub root_interrupt_parent: Option<u32>,
    pub gic_phandle: Option<u32>,
    pub gic_distributor_base_ipa: Option<u64>,
    pub gic_distributor_bytes: Option<u64>,
    pub gic_redistributor_base_ipa: Option<u64>,
    pub gic_redistributor_bytes: Option<u64>,
    pub gic_nodes_inside_device_window: bool,
    pub arch_timer_node_present: bool,
    pub arch_timer_interrupt_count: usize,
    pub interrupt_nodes: Vec<WindowsArmFdtInterruptCheck>,
    pub interrupt_nodes_described: bool,
    pub acpi_implemented: bool,
    pub fw_cfg_used: bool,
    pub gic_status: &'static str,
    pub gic_emulated: bool,
    pub blockers: Vec<String>,
}

impl WindowsArmPlatformDescriptionProbe {
    pub fn render_text(&self) -> String {
        let mut output = String::new();
        output.push_str("Windows 11 Arm HVF platform description probe\n");
        output.push_str(if self.qemu_used {
            "QEMU: used\n"
        } else {
            "QEMU: not used\n"
        });
        output.push_str(if self.apple_vz_used {
            "Apple VZ: used\n"
        } else {
            "Apple VZ: not used\n"
        });
        output.push_str(if self.hvf_entered {
            "HVF: entered\n"
        } else {
            "HVF: not entered\n"
        });
        output.push_str("Guest execution: not entered; metadata-only FDT platform description\n");
        output.push_str(&format!("Format: {}\n", self.format));
        output.push_str(&format!("FDT blob bytes: {:#x}\n", self.fdt_blob_bytes));
        output.push_str(&format!("FDT magic: {:#x}\n", self.fdt_magic));
        output.push_str(&format!(
            "FDT magic verified: {}\n",
            self.fdt_magic_verified
        ));
        output.push_str(&format!(
            "Memory node base: {}\n",
            render_optional_u64(self.memory_node_base_ipa)
        ));
        output.push_str(&format!(
            "Memory node at 0x40000000: {}\n",
            self.memory_node_at_guest_ram_base
        ));
        output.push_str(&format!(
            "Requested CPU count: {}\n",
            self.requested_cpu_count
        ));
        output.push_str(&format!("CPU count: {}\n", self.cpu_count));
        output.push_str(&format!(
            "CPU count verified: {}\n",
            self.cpu_count_verified
        ));
        output.push_str(&format!(
            "Device MMIO window: {:#x}..{:#x}\n",
            self.device_mmio_start_ipa, self.device_mmio_end_ipa
        ));
        output.push_str(&format!(
            "PL011/PL031/VirtIO-MMIO installer ISO/target disk nodes inside device window: {}\n",
            self.mmio_nodes_inside_device_window
        ));
        for node in &self.mmio_nodes {
            output.push_str(&format!("{} node: {}\n", node.label, node.node_name));
            output.push_str(&format!(
                "{} node base: {}\n",
                node.label,
                render_optional_u64(node.base_ipa)
            ));
            output.push_str(&format!(
                "{} node bytes: {}\n",
                node.label,
                render_optional_u64(node.bytes)
            ));
            output.push_str(&format!(
                "{} node inside device window: {}\n",
                node.label, node.inside_device_window
            ));
        }
        output.push_str(&format!(
            "Root interrupt-parent: {}\n",
            render_optional_u64(self.root_interrupt_parent.map(u64::from))
        ));
        output.push_str(&format!(
            "GIC phandle: {}\n",
            render_optional_u64(self.gic_phandle.map(u64::from))
        ));
        output.push_str(&format!(
            "GIC distributor base: {}\n",
            render_optional_u64(self.gic_distributor_base_ipa)
        ));
        output.push_str(&format!(
            "GIC distributor bytes: {}\n",
            render_optional_u64(self.gic_distributor_bytes)
        ));
        output.push_str(&format!(
            "GIC redistributor base: {}\n",
            render_optional_u64(self.gic_redistributor_base_ipa)
        ));
        output.push_str(&format!(
            "GIC redistributor bytes: {}\n",
            render_optional_u64(self.gic_redistributor_bytes)
        ));
        output.push_str(&format!(
            "GIC nodes inside device window: {}\n",
            self.gic_nodes_inside_device_window
        ));
        output.push_str(&format!(
            "ARM arch timer node present: {}\n",
            self.arch_timer_node_present
        ));
        output.push_str(&format!(
            "ARM arch timer interrupt count: {}\n",
            self.arch_timer_interrupt_count
        ));
        output.push_str(&format!(
            "Interrupt nodes described: {}\n",
            self.interrupt_nodes_described
        ));
        for interrupt in &self.interrupt_nodes {
            output.push_str(&format!(
                "{} interrupt node: {}\n",
                interrupt.label, interrupt.node_name
            ));
            output.push_str(&format!(
                "{} interrupt type: {}\n",
                interrupt.label,
                render_optional_u64(interrupt.interrupt_type.map(u64::from))
            ));
            output.push_str(&format!(
                "{} interrupt number: {}\n",
                interrupt.label,
                render_optional_u64(interrupt.interrupt_number.map(u64::from))
            ));
            output.push_str(&format!(
                "{} interrupt trigger: {}\n",
                interrupt.label,
                render_optional_u64(interrupt.trigger.map(u64::from))
            ));
            output.push_str(&format!(
                "{} interrupt described: {}\n",
                interrupt.label, interrupt.described
            ));
        }
        output.push_str(if self.acpi_implemented {
            "ACPI: implemented\n"
        } else {
            "ACPI: not implemented\n"
        });
        output.push_str(if self.fw_cfg_used {
            "fw_cfg: used\n"
        } else {
            "fw_cfg: not used\n"
        });
        output.push_str(&format!("GIC: {}\n", self.gic_status));
        output.push_str(&format!("GIC emulated: {}\n", self.gic_emulated));
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsArmBootDiskLayoutOptions {
    pub disk_path: PathBuf,
    pub size_gib: u32,
    pub create: bool,
}

impl Default for WindowsArmBootDiskLayoutOptions {
    fn default() -> Self {
        Self {
            disk_path: PathBuf::from("windows-11-arm-hvf.raw"),
            size_gib: WINDOWS_ARM_BOOT_DISK_DEFAULT_SIZE_GIB,
            create: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsArmBootDiskPartition {
    pub name: &'static str,
    pub role: &'static str,
    pub type_guid: &'static str,
    pub start_lba: u64,
    pub end_lba: u64,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsArmBootDiskLayoutProbe {
    pub disk_path: PathBuf,
    pub requested_size_gib: u32,
    pub disk_size_bytes: Option<u64>,
    pub create_requested: bool,
    pub created: bool,
    pub reopened_for_verification: bool,
    pub protective_mbr_verified: bool,
    pub primary_gpt_verified: bool,
    pub backup_gpt_verified: bool,
    pub partition_entries_verified: bool,
    pub partitions: Vec<WindowsArmBootDiskPartition>,
    pub blockers: Vec<String>,
}

impl WindowsArmBootDiskLayoutProbe {
    pub fn render_text(&self) -> String {
        let mut output = String::new();
        output.push_str("Windows 11 Arm HVF boot disk layout probe\n");
        output.push_str("QEMU: not used\n");
        output.push_str("Apple VZ: not used\n");
        output.push_str("HVF: not entered\n");
        output.push_str(
            "Guest execution: not entered; sparse raw GPT/UEFI Windows target disk layout\n",
        );
        output.push_str(&format!("Disk path: {}\n", self.disk_path.display()));
        output.push_str(&format!(
            "Requested size: {} GiB\n",
            self.requested_size_gib
        ));
        output.push_str(&format!(
            "Disk bytes: {}\n",
            render_optional_u64(self.disk_size_bytes)
        ));
        output.push_str(&format!("Create requested: {}\n", self.create_requested));
        output.push_str(&format!("Created: {}\n", self.created));
        output.push_str(&format!(
            "Reopened for verification: {}\n",
            self.reopened_for_verification
        ));
        output.push_str(&format!(
            "Protective MBR verified: {}\n",
            self.protective_mbr_verified
        ));
        output.push_str(&format!(
            "Primary GPT verified: {}\n",
            self.primary_gpt_verified
        ));
        output.push_str(&format!(
            "Backup GPT verified: {}\n",
            self.backup_gpt_verified
        ));
        output.push_str(&format!(
            "Partition entries verified: {}\n",
            self.partition_entries_verified
        ));
        output.push_str("Partitions:\n");
        for partition in &self.partitions {
            output.push_str(&format!(
                "- {}: {} - type {}, LBA {:#x}..{:#x}, bytes {:#x}\n",
                partition.name,
                partition.role,
                partition.type_guid,
                partition.start_lba,
                partition.end_lba,
                partition.size_bytes
            ));
        }
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsArmUefiFirmwareHandoffOptions {
    pub firmware_path: PathBuf,
    pub vars_template_path: Option<PathBuf>,
    pub vars_path: Option<PathBuf>,
    pub create_vars: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UefiFirmwareVolumeMetadata {
    pub offset: u64,
    pub length_bytes: u64,
    pub header_length: u16,
    pub checksum_verified: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsArmUefiFirmwareHandoffProbe {
    pub firmware_path: PathBuf,
    pub firmware_bytes: Option<u64>,
    pub firmware_slot_ipa: u64,
    pub firmware_slot_bytes: u64,
    pub firmware_volume: Option<UefiFirmwareVolumeMetadata>,
    pub firmware_verified: bool,
    pub vars_template_path: Option<PathBuf>,
    pub vars_template_bytes: Option<u64>,
    pub vars_template_verified: bool,
    pub vars_path: Option<PathBuf>,
    pub vars_bytes: Option<u64>,
    pub vars_slot_ipa: u64,
    pub vars_slot_bytes: u64,
    pub vars_created: bool,
    pub vars_reopened_for_verification: bool,
    pub vars_volume: Option<UefiFirmwareVolumeMetadata>,
    pub vars_verified: bool,
    pub planned_reset_vector_ipa: Option<u64>,
    pub blockers: Vec<String>,
}

impl WindowsArmUefiFirmwareHandoffProbe {
    pub fn render_text(&self) -> String {
        let mut output = String::new();
        output.push_str("Windows 11 Arm HVF UEFI firmware handoff probe\n");
        output.push_str("QEMU: not used\n");
        output.push_str("Apple VZ: not used\n");
        output.push_str("HVF: not entered\n");
        output.push_str(
            "Guest execution: not entered; AArch64 UEFI firmware and vars pflash handoff\n",
        );
        output.push_str(&format!(
            "Firmware path: {}\n",
            self.firmware_path.display()
        ));
        output.push_str(&format!(
            "Firmware bytes: {}\n",
            render_optional_u64(self.firmware_bytes)
        ));
        output.push_str(&format!(
            "Firmware slot IPA: {:#x}\n",
            self.firmware_slot_ipa
        ));
        output.push_str(&format!(
            "Firmware slot bytes: {:#x}\n",
            self.firmware_slot_bytes
        ));
        output.push_str(&format!("Firmware verified: {}\n", self.firmware_verified));
        render_uefi_volume_metadata("Firmware volume", &self.firmware_volume, &mut output);
        output.push_str(&format!(
            "Vars template path: {}\n",
            self.vars_template_path
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "not provided".to_string())
        ));
        output.push_str(&format!(
            "Vars template bytes: {}\n",
            render_optional_u64(self.vars_template_bytes)
        ));
        output.push_str(&format!(
            "Vars template verified: {}\n",
            self.vars_template_verified
        ));
        output.push_str(&format!(
            "Vars path: {}\n",
            self.vars_path
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "not provided".to_string())
        ));
        output.push_str(&format!(
            "Vars bytes: {}\n",
            render_optional_u64(self.vars_bytes)
        ));
        output.push_str(&format!("Vars slot IPA: {:#x}\n", self.vars_slot_ipa));
        output.push_str(&format!("Vars slot bytes: {:#x}\n", self.vars_slot_bytes));
        output.push_str(&format!("Vars created: {}\n", self.vars_created));
        output.push_str(&format!(
            "Vars reopened for verification: {}\n",
            self.vars_reopened_for_verification
        ));
        output.push_str(&format!("Vars verified: {}\n", self.vars_verified));
        render_uefi_volume_metadata("Vars volume", &self.vars_volume, &mut output);
        output.push_str(&format!(
            "Planned reset vector IPA: {}\n",
            render_optional_u64(self.planned_reset_vector_ipa)
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsArmUefiPflashMapOptions {
    pub firmware_path: PathBuf,
    pub vars_template_path: Option<PathBuf>,
    pub vars_path: Option<PathBuf>,
    pub create_vars: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsArmUefiFirmwareRunLoopOptions {
    pub pflash: WindowsArmUefiPflashMapOptions,
    pub execution: WindowsArmUefiFirmwareRunLoopExecutionOptions,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsArmUefiFirmwareDeviceDiscoveryProbe {
    pub run_loop: WindowsArmUefiFirmwareRunLoopProbe,
}

impl WindowsArmUefiFirmwareDeviceDiscoveryProbe {
    pub fn device_boundary_reached(&self) -> bool {
        self.run_loop
            .low_vector_post_repair_first_device_interaction_observed
            || self
                .run_loop
                .low_vector_post_repair_first_unhandled_access_observed
            || self.run_loop.handled_mmio_read_count > 0
            || self.run_loop.handled_mmio_write_count > 0
            || self.run_loop.handled_icc_read_count > 0
            || self.run_loop.handled_icc_write_count > 0
    }

    pub fn device_discovery_ready(&self) -> bool {
        self.device_boundary_reached()
            && !self
                .run_loop
                .low_vector_post_repair_first_unhandled_access_observed
            && self.run_loop.blockers.is_empty()
    }

    pub fn boundary_status(&self) -> &'static str {
        if !self.device_boundary_reached() {
            "not reached"
        } else if self
            .run_loop
            .low_vector_post_repair_first_unhandled_access_observed
        {
            "reached-unhandled"
        } else if self.device_discovery_ready() {
            "reached-handled"
        } else {
            "reached-blocked"
        }
    }

    pub fn render_text(&self) -> String {
        let mut output = String::new();
        output.push_str("Windows 11 Arm HVF UEFI firmware device-discovery probe\n");
        output.push_str("QEMU: not used\n");
        output.push_str("Apple VZ: not used\n");
        output.push_str("Windows boot: not claimed\n");
        output.push_str("Underlying probe: windows-firmware-run-loop-probe\n");
        output.push_str(&format!(
            "Device discovery boundary reached: {}\n",
            self.device_boundary_reached()
        ));
        output.push_str(&format!(
            "Device discovery boundary status: {}\n",
            self.boundary_status()
        ));
        output.push_str(&format!(
            "Device discovery ready: {}\n",
            self.device_discovery_ready()
        ));
        output.push_str(&format!(
            "First post-repair device interaction observed: {}\n",
            self.run_loop
                .low_vector_post_repair_first_device_interaction_observed
        ));
        output.push_str(&format!(
            "First post-repair unhandled access observed: {}\n",
            self.run_loop
                .low_vector_post_repair_first_unhandled_access_observed
        ));
        output.push_str(&format!(
            "Handled MMIO access count: {}\n",
            self.run_loop.handled_mmio_read_count + self.run_loop.handled_mmio_write_count
        ));
        output.push_str(&format!(
            "Handled ICC access count: {}\n",
            self.run_loop.handled_icc_read_count + self.run_loop.handled_icc_write_count
        ));
        if !self.device_boundary_reached() {
            output.push_str(
                "Device discovery blocker: firmware has not reached a non-diagnostic MMIO/sysreg boundary yet\n",
            );
        } else if self
            .run_loop
            .low_vector_post_repair_first_unhandled_access_observed
        {
            output.push_str(
                "Device discovery blocker: first firmware device boundary was unhandled\n",
            );
        } else if !self.run_loop.blockers.is_empty() {
            output
                .push_str("Device discovery blocker: underlying firmware run-loop has blockers\n");
        } else {
            output.push_str("Device discovery blocker: none\n");
        }
        output.push_str("Underlying firmware run-loop report:\n");
        output.push_str(&self.run_loop.render_text());
        output
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsArmUefiFirmwareRunLoopExecutionOptions {
    pub allow_loop: bool,
    pub requested_exits: u32,
    pub guest_ram_mib: u32,
    pub watchdog_timeout_ms: u64,
    pub map_low_pflash_alias: bool,
    pub seed_diagnostic_vector: bool,
    pub seed_guest_ram_diagnostic_vector: bool,
    pub seed_executable_diagnostic_vector: bool,
    pub try_recommended_vector_base_vbar: bool,
    pub continue_after_recommended_vector_base_vbar: bool,
    pub repair_low_vector_diagnostic_page: bool,
    pub remap_low_vector_to_recommended_vector: bool,
    pub continue_after_low_vector_repair: bool,
    pub restore_low_vector_slot_before_eret: bool,
    pub wire_interrupt_timer: bool,
    pub stop_at_first_post_repair_device_boundary: bool,
    pub installer_iso_path: Option<PathBuf>,
    pub writable_target_disk_path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsArmUefiPflashSlotMap {
    pub name: &'static str,
    pub path: PathBuf,
    pub ipa_start: u64,
    pub slot_bytes: u64,
    pub source_bytes: u64,
    pub copied_bytes: u64,
    pub zero_padding_bytes: u64,
    pub writable: bool,
    pub prefix_verified: bool,
    pub padding_zeroed: bool,
}

impl WindowsArmUefiPflashSlotMap {
    pub fn ipa_end_exclusive(&self) -> u64 {
        self.ipa_start.saturating_add(self.slot_bytes)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsArmUefiPflashMapProbe {
    pub firmware_path: PathBuf,
    pub vars_path: Option<PathBuf>,
    pub vars_created: bool,
    pub firmware_verified: bool,
    pub vars_verified: bool,
    pub firmware_slot: Option<WindowsArmUefiPflashSlotMap>,
    pub vars_slot: Option<WindowsArmUefiPflashSlotMap>,
    pub pflash_region_start: u64,
    pub pflash_region_bytes: u64,
    pub pflash_slots_non_overlapping: bool,
    pub guest_ram_overlap_verified: bool,
    pub device_mmio_overlap_verified: bool,
    pub pflash_map_verified: bool,
    pub planned_reset_vector_ipa: Option<u64>,
    pub blockers: Vec<String>,
}

impl WindowsArmUefiPflashMapProbe {
    pub fn render_text(&self) -> String {
        let mut output = String::new();
        output.push_str("Windows 11 Arm HVF UEFI pflash map probe\n");
        output.push_str("QEMU: not used\n");
        output.push_str("Apple VZ: not used\n");
        output.push_str("HVF: not entered\n");
        output.push_str(
            "Guest execution: not entered; AArch64 UEFI pflash slots loaded into memory images\n",
        );
        output.push_str(&format!(
            "Firmware path: {}\n",
            self.firmware_path.display()
        ));
        output.push_str(&format!(
            "Vars path: {}\n",
            self.vars_path
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "not provided".to_string())
        ));
        output.push_str(&format!("Vars created: {}\n", self.vars_created));
        output.push_str(&format!("Firmware verified: {}\n", self.firmware_verified));
        output.push_str(&format!("Vars verified: {}\n", self.vars_verified));
        output.push_str(&format!(
            "Pflash region: {:#x}..{:#x}\n",
            self.pflash_region_start,
            self.pflash_region_start
                .saturating_add(self.pflash_region_bytes)
        ));
        render_uefi_pflash_slot("Firmware pflash", &self.firmware_slot, &mut output);
        render_uefi_pflash_slot("Vars pflash", &self.vars_slot, &mut output);
        output.push_str(&format!(
            "Pflash slots non-overlapping: {}\n",
            self.pflash_slots_non_overlapping
        ));
        output.push_str(&format!(
            "Guest RAM overlap verified: {}\n",
            self.guest_ram_overlap_verified
        ));
        output.push_str(&format!(
            "Device MMIO overlap verified: {}\n",
            self.device_mmio_overlap_verified
        ));
        output.push_str(&format!(
            "Pflash map verified: {}\n",
            self.pflash_map_verified
        ));
        output.push_str(&format!(
            "Planned reset vector IPA: {}\n",
            render_optional_u64(self.planned_reset_vector_ipa)
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsArmUefiPflashHvfMapProbe {
    pub allowed: bool,
    pub attempted: bool,
    pub vm_created: bool,
    pub firmware_memory_allocated: bool,
    pub vars_memory_allocated: bool,
    pub firmware_memory_populated: bool,
    pub vars_memory_populated: bool,
    pub firmware_memory_mapped: bool,
    pub vars_memory_mapped: bool,
    pub firmware_memory_unmapped: bool,
    pub vars_memory_unmapped: bool,
    pub firmware_memory_deallocated: bool,
    pub vars_memory_deallocated: bool,
    pub vm_destroyed: bool,
    pub host: HvfHostCapabilities,
    pub pflash_map_verified: bool,
    pub firmware_slot_ipa: u64,
    pub vars_slot_ipa: u64,
    pub slot_bytes: u64,
    pub firmware_source_bytes: Option<u64>,
    pub vars_source_bytes: Option<u64>,
    pub firmware_map_flags: &'static str,
    pub vars_map_flags: &'static str,
    pub vm_create_status: Option<i32>,
    pub firmware_allocate_status: Option<i32>,
    pub vars_allocate_status: Option<i32>,
    pub firmware_map_status: Option<i32>,
    pub vars_map_status: Option<i32>,
    pub firmware_unmap_status: Option<i32>,
    pub vars_unmap_status: Option<i32>,
    pub firmware_deallocate_status: Option<i32>,
    pub vars_deallocate_status: Option<i32>,
    pub vm_destroy_status: Option<i32>,
    pub blockers: Vec<String>,
}

impl WindowsArmUefiPflashHvfMapProbe {
    pub fn render_text(&self) -> String {
        let mut output = String::new();
        output.push_str("Windows 11 Arm HVF UEFI pflash HVF map/unmap probe\n");
        output.push_str("QEMU: not used\n");
        output.push_str("Apple VZ: not used\n");
        output.push_str("Guest execution: not entered\n");
        output.push_str(&format!("Host: {}\n", self.host.host));
        output.push_str(&format!("Host HVF available: {}\n", self.host.available));
        output.push_str(&format!("Allowed: {}\n", self.allowed));
        output.push_str(&format!("Attempted: {}\n", self.attempted));
        output.push_str(&format!("VM created: {}\n", self.vm_created));
        output.push_str(&format!(
            "Firmware memory allocated: {}\n",
            self.firmware_memory_allocated
        ));
        output.push_str(&format!(
            "Vars memory allocated: {}\n",
            self.vars_memory_allocated
        ));
        output.push_str(&format!(
            "Firmware memory populated: {}\n",
            self.firmware_memory_populated
        ));
        output.push_str(&format!(
            "Vars memory populated: {}\n",
            self.vars_memory_populated
        ));
        output.push_str(&format!(
            "Firmware memory mapped: {}\n",
            self.firmware_memory_mapped
        ));
        output.push_str(&format!(
            "Vars memory mapped: {}\n",
            self.vars_memory_mapped
        ));
        output.push_str(&format!(
            "Firmware memory unmapped: {}\n",
            self.firmware_memory_unmapped
        ));
        output.push_str(&format!(
            "Vars memory unmapped: {}\n",
            self.vars_memory_unmapped
        ));
        output.push_str(&format!(
            "Firmware memory deallocated: {}\n",
            self.firmware_memory_deallocated
        ));
        output.push_str(&format!(
            "Vars memory deallocated: {}\n",
            self.vars_memory_deallocated
        ));
        output.push_str(&format!("VM destroyed: {}\n", self.vm_destroyed));
        output.push_str(&format!(
            "Pflash map verified: {}\n",
            self.pflash_map_verified
        ));
        output.push_str(&format!(
            "Firmware slot IPA: {:#x}\n",
            self.firmware_slot_ipa
        ));
        output.push_str(&format!("Vars slot IPA: {:#x}\n", self.vars_slot_ipa));
        output.push_str(&format!("Slot bytes: {:#x}\n", self.slot_bytes));
        output.push_str(&format!(
            "Firmware source bytes: {}\n",
            render_optional_u64(self.firmware_source_bytes)
        ));
        output.push_str(&format!(
            "Vars source bytes: {}\n",
            render_optional_u64(self.vars_source_bytes)
        ));
        output.push_str(&format!(
            "Firmware map flags: {}\n",
            self.firmware_map_flags
        ));
        output.push_str(&format!("Vars map flags: {}\n", self.vars_map_flags));
        output.push_str(&format!(
            "VM create status: {}\n",
            render_optional_status(self.vm_create_status)
        ));
        output.push_str(&format!(
            "VM create status name: {}\n",
            render_optional_status_name(self.vm_create_status)
        ));
        output.push_str(&format!(
            "Firmware allocate status: {}\n",
            render_optional_status(self.firmware_allocate_status)
        ));
        output.push_str(&format!(
            "Firmware allocate status name: {}\n",
            render_optional_status_name(self.firmware_allocate_status)
        ));
        output.push_str(&format!(
            "Vars allocate status: {}\n",
            render_optional_status(self.vars_allocate_status)
        ));
        output.push_str(&format!(
            "Vars allocate status name: {}\n",
            render_optional_status_name(self.vars_allocate_status)
        ));
        output.push_str(&format!(
            "Firmware map status: {}\n",
            render_optional_status(self.firmware_map_status)
        ));
        output.push_str(&format!(
            "Firmware map status name: {}\n",
            render_optional_status_name(self.firmware_map_status)
        ));
        output.push_str(&format!(
            "Vars map status: {}\n",
            render_optional_status(self.vars_map_status)
        ));
        output.push_str(&format!(
            "Vars map status name: {}\n",
            render_optional_status_name(self.vars_map_status)
        ));
        output.push_str(&format!(
            "Firmware unmap status: {}\n",
            render_optional_status(self.firmware_unmap_status)
        ));
        output.push_str(&format!(
            "Firmware unmap status name: {}\n",
            render_optional_status_name(self.firmware_unmap_status)
        ));
        output.push_str(&format!(
            "Vars unmap status: {}\n",
            render_optional_status(self.vars_unmap_status)
        ));
        output.push_str(&format!(
            "Vars unmap status name: {}\n",
            render_optional_status_name(self.vars_unmap_status)
        ));
        output.push_str(&format!(
            "Firmware deallocate status: {}\n",
            render_optional_status(self.firmware_deallocate_status)
        ));
        output.push_str(&format!(
            "Firmware deallocate status name: {}\n",
            render_optional_status_name(self.firmware_deallocate_status)
        ));
        output.push_str(&format!(
            "Vars deallocate status: {}\n",
            render_optional_status(self.vars_deallocate_status)
        ));
        output.push_str(&format!(
            "Vars deallocate status name: {}\n",
            render_optional_status_name(self.vars_deallocate_status)
        ));
        output.push_str(&format!(
            "VM destroy status: {}\n",
            render_optional_status(self.vm_destroy_status)
        ));
        output.push_str(&format!(
            "VM destroy status name: {}\n",
            render_optional_status_name(self.vm_destroy_status)
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsArmUefiResetVectorEntryProbe {
    pub allowed: bool,
    pub attempted: bool,
    pub vm_created: bool,
    pub firmware_memory_allocated: bool,
    pub vars_memory_allocated: bool,
    pub firmware_memory_populated: bool,
    pub vars_memory_populated: bool,
    pub firmware_memory_mapped: bool,
    pub vars_memory_mapped: bool,
    pub vcpu_created: bool,
    pub pc_set: bool,
    pub cpsr_set: bool,
    pub run_attempted: bool,
    pub reset_vector_entry_observed: bool,
    pub firmware_progress_observed: bool,
    pub watchdog_cancel_fired: bool,
    pub vcpu_destroyed: bool,
    pub firmware_memory_unmapped: bool,
    pub vars_memory_unmapped: bool,
    pub firmware_memory_deallocated: bool,
    pub vars_memory_deallocated: bool,
    pub vm_destroyed: bool,
    pub host: HvfHostCapabilities,
    pub pflash_map_verified: bool,
    pub reset_vector_ipa: u64,
    pub firmware_slot_ipa: u64,
    pub vars_slot_ipa: u64,
    pub slot_bytes: u64,
    pub firmware_source_bytes: Option<u64>,
    pub vars_source_bytes: Option<u64>,
    pub firmware_map_flags: &'static str,
    pub vars_map_flags: &'static str,
    pub vm_create_status: Option<i32>,
    pub firmware_allocate_status: Option<i32>,
    pub vars_allocate_status: Option<i32>,
    pub firmware_map_status: Option<i32>,
    pub vars_map_status: Option<i32>,
    pub vcpu_create_status: Option<i32>,
    pub pc_set_status: Option<i32>,
    pub cpsr_set_status: Option<i32>,
    pub run_status: Option<i32>,
    pub exit_reason: Option<u32>,
    pub exit_syndrome: Option<u64>,
    pub exit_exception_class: Option<u64>,
    pub exit_virtual_address: Option<u64>,
    pub exit_physical_address: Option<u64>,
    pub pc_after_run_status: Option<i32>,
    pub pc_after_run: Option<u64>,
    pub watchdog_cancel_status: Option<i32>,
    pub vcpu_destroy_status: Option<i32>,
    pub firmware_unmap_status: Option<i32>,
    pub vars_unmap_status: Option<i32>,
    pub firmware_deallocate_status: Option<i32>,
    pub vars_deallocate_status: Option<i32>,
    pub vm_destroy_status: Option<i32>,
    pub blockers: Vec<String>,
}

impl WindowsArmUefiResetVectorEntryProbe {
    pub fn render_text(&self) -> String {
        let mut output = String::new();
        output.push_str("Windows 11 Arm HVF UEFI reset-vector entry probe\n");
        output.push_str("QEMU: not used\n");
        output.push_str("Apple VZ: not used\n");
        output.push_str("Guest execution: UEFI reset vector entered under watchdog\n");
        output.push_str("Windows boot: not claimed\n");
        output.push_str(&format!("Host: {}\n", self.host.host));
        output.push_str(&format!("Host HVF available: {}\n", self.host.available));
        output.push_str(&format!("Allowed: {}\n", self.allowed));
        output.push_str(&format!("Attempted: {}\n", self.attempted));
        output.push_str(&format!("VM created: {}\n", self.vm_created));
        output.push_str(&format!(
            "Firmware memory allocated: {}\n",
            self.firmware_memory_allocated
        ));
        output.push_str(&format!(
            "Vars memory allocated: {}\n",
            self.vars_memory_allocated
        ));
        output.push_str(&format!(
            "Firmware memory populated: {}\n",
            self.firmware_memory_populated
        ));
        output.push_str(&format!(
            "Vars memory populated: {}\n",
            self.vars_memory_populated
        ));
        output.push_str(&format!(
            "Firmware memory mapped: {}\n",
            self.firmware_memory_mapped
        ));
        output.push_str(&format!(
            "Vars memory mapped: {}\n",
            self.vars_memory_mapped
        ));
        output.push_str(&format!("vCPU created: {}\n", self.vcpu_created));
        output.push_str(&format!("PC set: {}\n", self.pc_set));
        output.push_str(&format!("CPSR set: {}\n", self.cpsr_set));
        output.push_str(&format!("Run attempted: {}\n", self.run_attempted));
        output.push_str(&format!(
            "Reset-vector entry observed: {}\n",
            self.reset_vector_entry_observed
        ));
        output.push_str(&format!(
            "Firmware progress observed: {}\n",
            self.firmware_progress_observed
        ));
        output.push_str(&format!(
            "Watchdog cancel fired: {}\n",
            self.watchdog_cancel_fired
        ));
        output.push_str(&format!("vCPU destroyed: {}\n", self.vcpu_destroyed));
        output.push_str(&format!(
            "Firmware memory unmapped: {}\n",
            self.firmware_memory_unmapped
        ));
        output.push_str(&format!(
            "Vars memory unmapped: {}\n",
            self.vars_memory_unmapped
        ));
        output.push_str(&format!(
            "Firmware memory deallocated: {}\n",
            self.firmware_memory_deallocated
        ));
        output.push_str(&format!(
            "Vars memory deallocated: {}\n",
            self.vars_memory_deallocated
        ));
        output.push_str(&format!("VM destroyed: {}\n", self.vm_destroyed));
        output.push_str(&format!(
            "Pflash map verified: {}\n",
            self.pflash_map_verified
        ));
        output.push_str(&format!("Reset vector IPA: {:#x}\n", self.reset_vector_ipa));
        output.push_str(&format!(
            "Firmware slot IPA: {:#x}\n",
            self.firmware_slot_ipa
        ));
        output.push_str(&format!("Vars slot IPA: {:#x}\n", self.vars_slot_ipa));
        output.push_str(&format!("Slot bytes: {:#x}\n", self.slot_bytes));
        output.push_str(&format!(
            "Firmware source bytes: {}\n",
            render_optional_u64(self.firmware_source_bytes)
        ));
        output.push_str(&format!(
            "Vars source bytes: {}\n",
            render_optional_u64(self.vars_source_bytes)
        ));
        output.push_str(&format!(
            "Firmware map flags: {}\n",
            self.firmware_map_flags
        ));
        output.push_str(&format!("Vars map flags: {}\n", self.vars_map_flags));
        output.push_str(&format!(
            "VM create status name: {}\n",
            render_optional_status_name(self.vm_create_status)
        ));
        output.push_str(&format!(
            "Firmware allocate status name: {}\n",
            render_optional_status_name(self.firmware_allocate_status)
        ));
        output.push_str(&format!(
            "Vars allocate status name: {}\n",
            render_optional_status_name(self.vars_allocate_status)
        ));
        output.push_str(&format!(
            "Firmware map status name: {}\n",
            render_optional_status_name(self.firmware_map_status)
        ));
        output.push_str(&format!(
            "Vars map status name: {}\n",
            render_optional_status_name(self.vars_map_status)
        ));
        output.push_str(&format!(
            "vCPU create status name: {}\n",
            render_optional_status_name(self.vcpu_create_status)
        ));
        output.push_str(&format!(
            "PC set status name: {}\n",
            render_optional_status_name(self.pc_set_status)
        ));
        output.push_str(&format!(
            "CPSR set status name: {}\n",
            render_optional_status_name(self.cpsr_set_status)
        ));
        output.push_str(&format!(
            "Run status: {}\n",
            render_optional_status(self.run_status)
        ));
        output.push_str(&format!(
            "Run status name: {}\n",
            render_optional_status_name(self.run_status)
        ));
        output.push_str(&format!(
            "Exit reason: {}\n",
            render_optional_exit_reason(self.exit_reason)
        ));
        output.push_str(&format!(
            "Exit reason name: {}\n",
            render_optional_exit_reason_name(self.exit_reason)
        ));
        output.push_str(&format!(
            "Exit syndrome: {}\n",
            render_optional_u64(self.exit_syndrome)
        ));
        output.push_str(&format!(
            "Exit exception class: {}\n",
            render_optional_u64(self.exit_exception_class)
        ));
        output.push_str(&format!(
            "Exit exception class name: {}\n",
            render_optional_exception_class_name(self.exit_exception_class)
        ));
        output.push_str(&format!(
            "Exit virtual address: {}\n",
            render_optional_u64(self.exit_virtual_address)
        ));
        output.push_str(&format!(
            "Exit physical address: {}\n",
            render_optional_u64(self.exit_physical_address)
        ));
        output.push_str(&format!(
            "PC after run status name: {}\n",
            render_optional_status_name(self.pc_after_run_status)
        ));
        output.push_str(&format!(
            "PC after run: {}\n",
            render_optional_u64(self.pc_after_run)
        ));
        output.push_str(&format!(
            "Watchdog cancel status name: {}\n",
            render_optional_status_name(self.watchdog_cancel_status)
        ));
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
            "Firmware deallocate status name: {}\n",
            render_optional_status_name(self.firmware_deallocate_status)
        ));
        output.push_str(&format!(
            "Vars deallocate status name: {}\n",
            render_optional_status_name(self.vars_deallocate_status)
        ));
        output.push_str(&format!(
            "VM destroy status name: {}\n",
            render_optional_status_name(self.vm_destroy_status)
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsArmUefiFirmwareRunLoopExit {
    pub index: u32,
    pub run_status: Option<i32>,
    pub exit_reason: Option<u32>,
    pub exit_syndrome: Option<u64>,
    pub exit_exception_class: Option<u64>,
    pub exit_virtual_address: Option<u64>,
    pub exit_physical_address: Option<u64>,
    pub pc_after_exit_status: Option<i32>,
    pub pc_after_exit: Option<u64>,
    pub instruction_word_after_exit: Option<u32>,
    pub instruction_hint_after_exit: &'static str,
    pub pc_stage1_leaf_level_after_exit: Option<u8>,
    pub pc_stage1_leaf_descriptor_after_exit: Option<u64>,
    pub pc_stage1_leaf_descriptor_kind_after_exit: &'static str,
    pub pc_stage1_leaf_pxn_after_exit: Option<bool>,
    pub pc_stage1_leaf_uxn_after_exit: Option<bool>,
    pub stage1_descriptor_samples_after_exit: Vec<WindowsArmUefiStage1DescriptorSample>,
    pub stage1_walk_entries_after_exit: Vec<WindowsArmUefiStage1WalkEntry>,
    pub stage1_executable_candidates_after_exit: Vec<WindowsArmUefiStage1ExecutableCandidate>,
    pub x0_after_exit: Option<u64>,
    pub x1_after_exit: Option<u64>,
    pub x2_after_exit: Option<u64>,
    pub x3_after_exit: Option<u64>,
    pub x4_after_exit: Option<u64>,
    pub cpsr_after_exit: Option<u64>,
    pub vbar_el1_after_exit: Option<u64>,
    pub elr_el1_after_exit: Option<u64>,
    pub esr_el1_after_exit: Option<u64>,
    pub far_el1_after_exit: Option<u64>,
    pub spsr_el1_after_exit: Option<u64>,
    pub sctlr_el1_after_exit: Option<u64>,
    pub tcr_el1_after_exit: Option<u64>,
    pub ttbr0_el1_after_exit: Option<u64>,
    pub ttbr1_el1_after_exit: Option<u64>,
    pub mair_el1_after_exit: Option<u64>,
    pub sp_el1_after_exit: Option<u64>,
    pub watchdog_cancel_status: Option<i32>,
    pub vtimer_auto_mask_get_status: Option<i32>,
    pub vtimer_auto_mask_after_exit: Option<bool>,
    pub vtimer_rearm_cval_value: Option<u64>,
    pub vtimer_rearm_cval_set_status: Option<i32>,
    pub vtimer_ppi_pending_recorded: Option<bool>,
    pub vtimer_irq_line_assertable: Option<bool>,
    pub vtimer_gic_group1_enabled: Option<bool>,
    pub vtimer_gic_priority_mask: Option<u8>,
    pub vtimer_gic_running_priority: Option<u8>,
    pub vtimer_gic_priority_threshold: Option<u8>,
    pub vtimer_gic_pending_intid: Option<u32>,
    pub vtimer_pending_irq_set_status: Option<i32>,
    pub vtimer_unmask_status: Option<i32>,
    pub handled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsArmUefiStage1DescriptorSample {
    pub label: &'static str,
    pub virtual_address: u64,
    pub region: &'static str,
    pub level: Option<u8>,
    pub descriptor: Option<u64>,
    pub descriptor_kind: &'static str,
    pub output_address: Option<u64>,
    pub attr_index: Option<u8>,
    pub access_permissions: Option<u8>,
    pub shareability: Option<u8>,
    pub access_flag: Option<bool>,
    pub pxn: Option<bool>,
    pub uxn: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsArmUefiStage1WalkEntry {
    pub label: &'static str,
    pub virtual_address: u64,
    pub region: &'static str,
    pub level: u8,
    pub table_ipa: u64,
    pub index: u64,
    pub entry_ipa: u64,
    pub descriptor: Option<u64>,
    pub descriptor_kind: &'static str,
    pub next_table_ipa: Option<u64>,
    pub output_address: Option<u64>,
    pub attr_index: Option<u8>,
    pub access_permissions: Option<u8>,
    pub shareability: Option<u8>,
    pub access_flag: Option<bool>,
    pub pxn: Option<bool>,
    pub uxn: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsArmUefiStage1ExecutableCandidate {
    pub virtual_address: u64,
    pub region: &'static str,
    pub level: u8,
    pub descriptor: u64,
    pub descriptor_kind: &'static str,
    pub output_address: Option<u64>,
    pub span_bytes: Option<u64>,
    pub vector_sync_virtual_address: Option<u64>,
    pub vector_sync_physical_address: Option<u64>,
    pub vector_sync_instruction_word: Option<u32>,
    pub vector_sync_instruction_hint: &'static str,
    pub vector_base_scan_scanned_count: u32,
    pub vector_base_scan_suppressed_count: u32,
    pub vector_base_scan_limit_reached: bool,
    pub recommended_vector_base_candidate: Option<WindowsArmUefiVectorBaseRecommendation>,
    pub vector_base_candidates: Vec<WindowsArmUefiVectorBaseCandidate>,
    pub attr_index: u8,
    pub access_permissions: u8,
    pub shareability: u8,
    pub access_flag: bool,
    pub pxn: bool,
    pub uxn: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsArmUefiVectorBaseCandidate {
    pub base_virtual_address: u64,
    pub base_physical_address: Option<u64>,
    pub current_el_sp0_sync_instruction_word: Option<u32>,
    pub current_el_spx_sync_instruction_word: Option<u32>,
    pub lower_aarch64_sync_instruction_word: Option<u32>,
    pub lower_aarch32_sync_instruction_word: Option<u32>,
    pub current_el_spx_sync_instruction_hint: &'static str,
    pub populated_slot_count: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsArmUefiVectorBaseRecommendation {
    pub base_virtual_address: u64,
    pub base_physical_address: Option<u64>,
    pub current_el_spx_sync_instruction_word: Option<u32>,
    pub current_el_spx_sync_instruction_hint: &'static str,
    pub reason: &'static str,
}

impl WindowsArmUefiVectorBaseRecommendation {
    fn is_populated_low_vector_remap_target(&self) -> bool {
        self.base_physical_address.is_some()
            && windows_arm_vector_slot_instruction_is_non_diagnostic_populated(
                self.current_el_spx_sync_instruction_word,
            )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct WindowsArmUefiVectorBaseCandidateScan {
    scanned_count: u32,
    suppressed_count: u32,
    limit_reached: bool,
    candidates: Vec<WindowsArmUefiVectorBaseCandidate>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsArmUefiFirmwareRunLoopProbe {
    pub allowed: bool,
    pub attempted: bool,
    pub vm_created: bool,
    pub firmware_memory_allocated: bool,
    pub vars_memory_allocated: bool,
    pub guest_ram_memory_allocated: bool,
    pub firmware_memory_populated: bool,
    pub vars_memory_populated: bool,
    pub firmware_memory_mapped: bool,
    pub vars_memory_mapped: bool,
    pub low_firmware_alias_mapped: bool,
    pub low_vars_alias_mapped: bool,
    pub guest_ram_memory_mapped: bool,
    pub platform_dtb_populated: bool,
    pub diagnostic_vector_seed_requested: bool,
    pub diagnostic_vector_populated: bool,
    pub low_vector_diagnostic_page_repair_requested: bool,
    pub low_vector_diagnostic_page_repaired: bool,
    pub low_vector_diagnostic_page_slot_restored: bool,
    pub low_vector_diagnostic_page_restore_before_eret_requested: bool,
    pub low_vector_diagnostic_page_restore_before_eret_attempted: bool,
    pub low_vector_diagnostic_page_entry_ipa: Option<u64>,
    pub low_vector_diagnostic_page_previous_descriptor: Option<u64>,
    pub low_vector_diagnostic_page_descriptor: Option<u64>,
    pub low_vector_diagnostic_page_repeated_fault_observed: bool,
    pub low_vector_recommended_vector_remap_requested: bool,
    pub low_vector_recommended_vector_remap_attempted: bool,
    pub low_vector_recommended_vector_remap_succeeded: bool,
    pub low_vector_recommended_vector_remap_target_physical_address: Option<u64>,
    pub low_vector_recommended_vector_remap_descriptor: Option<u64>,
    pub low_vector_post_repair_continue_requested: bool,
    pub low_vector_post_repair_continue_attempted: bool,
    pub stop_at_first_post_repair_device_boundary_requested: bool,
    pub low_vector_post_repair_unsupported_exit_observed: bool,
    pub low_vector_post_repair_unsupported_exit_reason: Option<u32>,
    pub low_vector_post_repair_unsupported_exit_diagnosis: &'static str,
    pub low_vector_post_repair_first_exit_observed: bool,
    pub low_vector_post_repair_first_exit_index: Option<u32>,
    pub low_vector_post_repair_first_exit_reason: Option<u32>,
    pub low_vector_post_repair_first_exit_diagnosis: &'static str,
    pub low_vector_post_repair_first_exit_pc: Option<u64>,
    pub low_vector_post_repair_first_interaction_kind: &'static str,
    pub low_vector_post_repair_first_exit_access_kind: &'static str,
    pub low_vector_post_repair_first_exit_access_direction: &'static str,
    pub low_vector_post_repair_first_exit_access_address: Option<u64>,
    pub low_vector_post_repair_first_exit_access_sysreg: Option<u16>,
    pub low_vector_post_repair_first_exit_access_syndrome: Option<u64>,
    pub low_vector_post_repair_first_device_interaction_observed: bool,
    pub low_vector_post_repair_first_device_interaction_index: Option<u32>,
    pub low_vector_post_repair_first_device_interaction_reason: Option<u32>,
    pub low_vector_post_repair_first_device_interaction_diagnosis: &'static str,
    pub low_vector_post_repair_first_device_interaction_pc: Option<u64>,
    pub low_vector_post_repair_first_device_interaction_kind: &'static str,
    pub low_vector_post_repair_first_device_interaction_access_kind: &'static str,
    pub low_vector_post_repair_first_device_interaction_access_direction: &'static str,
    pub low_vector_post_repair_first_device_interaction_access_address: Option<u64>,
    pub low_vector_post_repair_first_device_interaction_access_sysreg: Option<u16>,
    pub low_vector_post_repair_first_device_interaction_access_syndrome: Option<u64>,
    pub low_vector_post_repair_first_unhandled_access_observed: bool,
    pub low_vector_post_repair_first_unhandled_access_index: Option<u32>,
    pub low_vector_post_repair_first_unhandled_access_reason: Option<u32>,
    pub low_vector_post_repair_first_unhandled_access_diagnosis: &'static str,
    pub low_vector_post_repair_first_unhandled_access_pc: Option<u64>,
    pub low_vector_post_repair_first_unhandled_access_syndrome: Option<u64>,
    pub low_vector_post_repair_first_unhandled_access_kind: &'static str,
    pub low_vector_post_repair_first_unhandled_access_direction: &'static str,
    pub low_vector_post_repair_first_unhandled_access_register: Option<u8>,
    pub low_vector_post_repair_first_unhandled_access_value: Option<u64>,
    pub low_vector_post_repair_first_unhandled_access_handler_result: &'static str,
    pub low_vector_post_repair_first_unhandled_access_mmio_ipa: Option<u64>,
    pub low_vector_post_repair_first_unhandled_access_mmio_width: Option<u8>,
    pub low_vector_post_repair_first_unhandled_access_mmio_device_kind: &'static str,
    pub low_vector_post_repair_first_unhandled_access_sysreg: Option<u16>,
    pub low_vector_post_repair_first_unhandled_access_sysreg_name: &'static str,
    pub low_vector_post_repair_first_unhandled_access_sysreg_op0: Option<u8>,
    pub low_vector_post_repair_first_unhandled_access_sysreg_op1: Option<u8>,
    pub low_vector_post_repair_first_unhandled_access_sysreg_crn: Option<u8>,
    pub low_vector_post_repair_first_unhandled_access_sysreg_crm: Option<u8>,
    pub low_vector_post_repair_first_unhandled_access_sysreg_op2: Option<u8>,
    pub low_vector_diagnostic_page_resume_attempted: bool,
    pub low_vector_diagnostic_page_resume_armed: bool,
    pub low_vector_diagnostic_page_resume_original_pc: Option<u64>,
    pub low_vector_diagnostic_page_resume_original_elr_el1: Option<u64>,
    pub low_vector_diagnostic_page_resume_original_esr_el1: Option<u64>,
    pub low_vector_diagnostic_page_resume_original_far_el1: Option<u64>,
    pub low_vector_diagnostic_page_resume_original_spsr_el1: Option<u64>,
    pub low_vector_diagnostic_page_original_slot_bytes: Option<[u8; 12]>,
    pub low_vector_diagnostic_page_resume_target_instruction_before_eret: Option<u32>,
    pub low_vector_diagnostic_page_resume_target_stage1_leaf_descriptor_before_eret: Option<u64>,
    pub low_vector_diagnostic_page_resume_target_stage1_leaf_kind_before_eret: &'static str,
    pub low_vector_diagnostic_page_resume_target_is_installed_diagnostic_hvc_before_eret: bool,
    pub low_vector_diagnostic_page_resume_elr_el1_set_status: Option<i32>,
    pub low_vector_diagnostic_page_resume_spsr_el1_set_status: Option<i32>,
    pub low_vector_diagnostic_page_resume_cpsr_set_status: Option<i32>,
    pub low_vector_diagnostic_page_resume_pc_set_status: Option<i32>,
    pub vcpu_created: bool,
    pub pc_set: bool,
    pub x0_dtb_ipa_set: bool,
    pub cpsr_set: bool,
    pub sp_el1_set: bool,
    pub diagnostic_vector_vbar_el1_set: bool,
    pub recommended_vector_base_vbar_requested: bool,
    pub recommended_vector_base_vbar_attempted: bool,
    pub recommended_vector_base_vbar_set: bool,
    pub recommended_vector_base_vbar_diagnostic_vector_populated: bool,
    pub recommended_vector_base_vbar_resume_requested: bool,
    pub recommended_vector_base_vbar_resume_attempted: bool,
    pub recommended_vector_base_vbar_resume_armed: bool,
    pub interrupt_timer_wiring_requested: bool,
    pub interrupt_timer_initialized: bool,
    pub run_loop_attempted: bool,
    pub firmware_progress_observed: bool,
    pub unsupported_exit_observed: bool,
    pub watchdog_cancel_fired: bool,
    pub vcpu_destroyed: bool,
    pub firmware_memory_unmapped: bool,
    pub vars_memory_unmapped: bool,
    pub guest_ram_memory_unmapped: bool,
    pub firmware_memory_deallocated: bool,
    pub vars_memory_deallocated: bool,
    pub guest_ram_memory_deallocated: bool,
    pub vm_destroyed: bool,
    pub host: HvfHostCapabilities,
    pub pflash_map_verified: bool,
    pub reset_vector_ipa: u64,
    pub firmware_slot_ipa: u64,
    pub vars_slot_ipa: u64,
    pub low_firmware_alias_ipa: u64,
    pub low_vars_alias_ipa: u64,
    pub guest_ram_ipa: u64,
    pub platform_dtb_ipa: u64,
    pub platform_dtb_guest_ram_offset: u64,
    pub sp_el1_seed_ipa: u64,
    pub diagnostic_vector_location: &'static str,
    pub diagnostic_vector_ipa: u64,
    pub diagnostic_vector_bytes: u64,
    pub recommended_vector_base_vbar_source_exit_index: Option<u32>,
    pub recommended_vector_base_vbar_target: Option<u64>,
    pub recommended_vector_base_vbar_target_physical_address: Option<u64>,
    pub recommended_vector_base_vbar_reason: &'static str,
    pub recommended_vector_base_vbar_current_el_spx_sync_instruction_word: Option<u32>,
    pub recommended_vector_base_vbar_current_el_spx_sync_instruction_hint: &'static str,
    pub recommended_vector_base_vbar_followup_exit_observed: bool,
    pub recommended_vector_base_vbar_followup_exit_index: Option<u32>,
    pub recommended_vector_base_vbar_followup_exit_reason: Option<u32>,
    pub recommended_vector_base_vbar_followup_exit_diagnosis: &'static str,
    pub recommended_vector_base_vbar_followup_pc: Option<u64>,
    pub recommended_vector_base_vbar_followup_vbar_el1: Option<u64>,
    pub recommended_vector_base_vbar_followup_target_still_set: bool,
    pub recommended_vector_base_vbar_resume_original_pc: Option<u64>,
    pub recommended_vector_base_vbar_resume_original_elr_el1: Option<u64>,
    pub recommended_vector_base_vbar_resume_original_esr_el1: Option<u64>,
    pub recommended_vector_base_vbar_resume_original_far_el1: Option<u64>,
    pub recommended_vector_base_vbar_resume_original_spsr_el1: Option<u64>,
    pub slot_bytes: u64,
    pub guest_ram_bytes: u64,
    pub platform_dtb_bytes: usize,
    pub platform_dtb_magic: u32,
    pub platform_dtb_magic_verified: bool,
    pub requested_exits: u32,
    pub observed_exits: u32,
    pub watchdog_timeout_ms: u64,
    pub vtimer_offset_value: Option<u64>,
    pub cntv_cval_value: Option<u64>,
    pub cntv_ctl_value: Option<u64>,
    pub vtimer_exit_count: u32,
    pub pending_irq_injected_count: u32,
    pub device_irq_injected_count: u32,
    pub device_irq_cleared_count: u32,
    pub handled_mmio_read_count: u32,
    pub handled_mmio_write_count: u32,
    pub handled_pl011_mmio_count: u32,
    pub handled_pl031_mmio_count: u32,
    pub handled_gicd_mmio_count: u32,
    pub handled_gicr_mmio_count: u32,
    pub handled_virtio_installer_iso_mmio_count: u32,
    pub handled_virtio_target_disk_mmio_count: u32,
    pub virtio_queue_notify_count: u32,
    pub virtio_request_completion_count: u32,
    pub handled_icc_read_count: u32,
    pub handled_icc_write_count: u32,
    pub handled_icc_iar1_read_count: u32,
    pub handled_icc_eoir1_write_count: u32,
    pub handled_icc_dir_write_count: u32,
    pub last_icc_iar1_intid: Option<u32>,
    pub last_icc_eoir1_intid: Option<u32>,
    pub last_icc_dir_intid: Option<u32>,
    pub firmware_source_bytes: Option<u64>,
    pub vars_source_bytes: Option<u64>,
    pub installer_iso_path: Option<PathBuf>,
    pub writable_target_disk_path: Option<PathBuf>,
    pub block_devices: Vec<WindowsArmVirtioBlockDeviceMetadata>,
    pub firmware_map_flags: &'static str,
    pub vars_map_flags: &'static str,
    pub low_firmware_alias_map_flags: &'static str,
    pub low_vars_alias_map_flags: &'static str,
    pub guest_ram_map_flags: &'static str,
    pub low_pflash_alias_requested: bool,
    pub vm_create_status: Option<i32>,
    pub firmware_allocate_status: Option<i32>,
    pub vars_allocate_status: Option<i32>,
    pub guest_ram_allocate_status: Option<i32>,
    pub firmware_map_status: Option<i32>,
    pub vars_map_status: Option<i32>,
    pub low_firmware_alias_map_status: Option<i32>,
    pub low_vars_alias_map_status: Option<i32>,
    pub guest_ram_map_status: Option<i32>,
    pub vcpu_create_status: Option<i32>,
    pub pc_set_status: Option<i32>,
    pub x0_dtb_ipa_set_status: Option<i32>,
    pub cpsr_set_status: Option<i32>,
    pub sp_el1_set_status: Option<i32>,
    pub diagnostic_vector_vbar_el1_set_status: Option<i32>,
    pub recommended_vector_base_vbar_set_status: Option<i32>,
    pub recommended_vector_base_vbar_resume_vbar_el1_set_status: Option<i32>,
    pub recommended_vector_base_vbar_resume_elr_el1_set_status: Option<i32>,
    pub recommended_vector_base_vbar_resume_spsr_el1_set_status: Option<i32>,
    pub recommended_vector_base_vbar_resume_pc_set_status: Option<i32>,
    pub vtimer_offset_set_status: Option<i32>,
    pub cntv_cval_set_status: Option<i32>,
    pub cntv_ctl_set_status: Option<i32>,
    pub vtimer_initial_unmask_status: Option<i32>,
    pub last_pending_irq_set_status: Option<i32>,
    pub last_device_irq_set_status: Option<i32>,
    pub last_device_irq_clear_status: Option<i32>,
    pub last_vtimer_unmask_status: Option<i32>,
    pub final_pc_status: Option<i32>,
    pub final_pc: Option<u64>,
    pub vcpu_destroy_status: Option<i32>,
    pub firmware_unmap_status: Option<i32>,
    pub vars_unmap_status: Option<i32>,
    pub low_firmware_alias_unmap_status: Option<i32>,
    pub low_vars_alias_unmap_status: Option<i32>,
    pub guest_ram_unmap_status: Option<i32>,
    pub firmware_deallocate_status: Option<i32>,
    pub vars_deallocate_status: Option<i32>,
    pub guest_ram_deallocate_status: Option<i32>,
    pub vm_destroy_status: Option<i32>,
    pub exits: Vec<WindowsArmUefiFirmwareRunLoopExit>,
    pub blockers: Vec<String>,
}

impl WindowsArmUefiFirmwareRunLoopProbe {
    fn low_vector_post_repair_first_exit_telemetry(&self) -> LowVectorPostRepairExitTelemetry {
        LowVectorPostRepairExitTelemetry {
            observed: self.low_vector_post_repair_first_exit_observed,
            index: self.low_vector_post_repair_first_exit_index,
            reason: self.low_vector_post_repair_first_exit_reason,
            diagnosis: self.low_vector_post_repair_first_exit_diagnosis,
            pc: self.low_vector_post_repair_first_exit_pc,
            interaction_kind: self.low_vector_post_repair_first_interaction_kind,
            access: LowVectorPostRepairAccessTelemetry {
                kind: self.low_vector_post_repair_first_exit_access_kind,
                direction: self.low_vector_post_repair_first_exit_access_direction,
                address: self.low_vector_post_repair_first_exit_access_address,
                sysreg: self.low_vector_post_repair_first_exit_access_sysreg,
                syndrome: self.low_vector_post_repair_first_exit_access_syndrome,
            },
        }
    }

    fn low_vector_post_repair_first_device_interaction_telemetry(
        &self,
    ) -> LowVectorPostRepairExitTelemetry {
        LowVectorPostRepairExitTelemetry {
            observed: self.low_vector_post_repair_first_device_interaction_observed,
            index: self.low_vector_post_repair_first_device_interaction_index,
            reason: self.low_vector_post_repair_first_device_interaction_reason,
            diagnosis: self.low_vector_post_repair_first_device_interaction_diagnosis,
            pc: self.low_vector_post_repair_first_device_interaction_pc,
            interaction_kind: self.low_vector_post_repair_first_device_interaction_kind,
            access: LowVectorPostRepairAccessTelemetry {
                kind: self.low_vector_post_repair_first_device_interaction_access_kind,
                direction: self.low_vector_post_repair_first_device_interaction_access_direction,
                address: self.low_vector_post_repair_first_device_interaction_access_address,
                sysreg: self.low_vector_post_repair_first_device_interaction_access_sysreg,
                syndrome: self.low_vector_post_repair_first_device_interaction_access_syndrome,
            },
        }
    }

    fn low_vector_post_repair_first_unhandled_access_telemetry(
        &self,
    ) -> LowVectorPostRepairUnhandledAccessTelemetry {
        LowVectorPostRepairUnhandledAccessTelemetry {
            observed: self.low_vector_post_repair_first_unhandled_access_observed,
            index: self.low_vector_post_repair_first_unhandled_access_index,
            reason: self.low_vector_post_repair_first_unhandled_access_reason,
            diagnosis: self.low_vector_post_repair_first_unhandled_access_diagnosis,
            pc: self.low_vector_post_repair_first_unhandled_access_pc,
            syndrome: self.low_vector_post_repair_first_unhandled_access_syndrome,
            kind: self.low_vector_post_repair_first_unhandled_access_kind,
            access: self.low_vector_post_repair_first_unhandled_access_direction,
            register: self.low_vector_post_repair_first_unhandled_access_register,
            value: self.low_vector_post_repair_first_unhandled_access_value,
            handler_result: self.low_vector_post_repair_first_unhandled_access_handler_result,
            mmio_ipa: self.low_vector_post_repair_first_unhandled_access_mmio_ipa,
            mmio_width: self.low_vector_post_repair_first_unhandled_access_mmio_width,
            mmio_device_kind: self.low_vector_post_repair_first_unhandled_access_mmio_device_kind,
            sysreg: self.low_vector_post_repair_first_unhandled_access_sysreg,
            sysreg_name: self.low_vector_post_repair_first_unhandled_access_sysreg_name,
            sysreg_op0: self.low_vector_post_repair_first_unhandled_access_sysreg_op0,
            sysreg_op1: self.low_vector_post_repair_first_unhandled_access_sysreg_op1,
            sysreg_crn: self.low_vector_post_repair_first_unhandled_access_sysreg_crn,
            sysreg_crm: self.low_vector_post_repair_first_unhandled_access_sysreg_crm,
            sysreg_op2: self.low_vector_post_repair_first_unhandled_access_sysreg_op2,
        }
    }

    pub fn render_text(&self) -> String {
        let mut output = String::new();
        output.push_str("Windows 11 Arm HVF UEFI firmware run-loop probe\n");
        output.push_str("QEMU: not used\n");
        output.push_str("Apple VZ: not used\n");
        output.push_str("Guest execution: bounded UEFI firmware exit classification loop\n");
        output.push_str("Windows boot: not claimed\n");
        output.push_str(&format!(
            "Device models: {}\n",
            WINDOWS_ARM_FIRMWARE_MMIO_DEVICE_MODELS
        ));
        output.push_str(&format!("Host: {}\n", self.host.host));
        output.push_str(&format!("Host HVF available: {}\n", self.host.available));
        output.push_str(&format!("Allowed: {}\n", self.allowed));
        output.push_str(&format!("Attempted: {}\n", self.attempted));
        output.push_str(&format!("VM created: {}\n", self.vm_created));
        output.push_str(&format!(
            "Firmware memory allocated: {}\n",
            self.firmware_memory_allocated
        ));
        output.push_str(&format!(
            "Vars memory allocated: {}\n",
            self.vars_memory_allocated
        ));
        output.push_str(&format!(
            "Guest RAM memory allocated: {}\n",
            self.guest_ram_memory_allocated
        ));
        output.push_str(&format!(
            "Firmware memory populated: {}\n",
            self.firmware_memory_populated
        ));
        output.push_str(&format!(
            "Vars memory populated: {}\n",
            self.vars_memory_populated
        ));
        output.push_str(&format!(
            "Firmware memory mapped: {}\n",
            self.firmware_memory_mapped
        ));
        output.push_str(&format!(
            "Vars memory mapped: {}\n",
            self.vars_memory_mapped
        ));
        output.push_str(&format!(
            "Low firmware alias mapped: {}\n",
            self.low_firmware_alias_mapped
        ));
        output.push_str(&format!(
            "Low vars alias mapped: {}\n",
            self.low_vars_alias_mapped
        ));
        output.push_str(&format!(
            "Guest RAM memory mapped: {}\n",
            self.guest_ram_memory_mapped
        ));
        output.push_str(&format!(
            "Platform DTB populated: {}\n",
            self.platform_dtb_populated
        ));
        output.push_str(&format!(
            "Diagnostic vector seed requested: {}\n",
            self.diagnostic_vector_seed_requested
        ));
        output.push_str(&format!(
            "Diagnostic vector populated: {}\n",
            self.diagnostic_vector_populated
        ));
        output.push_str(&format!(
            "Low vector diagnostic page repair requested: {}\n",
            self.low_vector_diagnostic_page_repair_requested
        ));
        output.push_str(&format!(
            "Low vector diagnostic page repaired: {}\n",
            self.low_vector_diagnostic_page_repaired
        ));
        output.push_str(&format!(
            "Low vector diagnostic page slot restored: {}\n",
            self.low_vector_diagnostic_page_slot_restored
        ));
        output.push_str(&format!(
            "Low vector diagnostic page restore before ERET requested: {}\n",
            self.low_vector_diagnostic_page_restore_before_eret_requested
        ));
        output.push_str(&format!(
            "Low vector diagnostic page restore before ERET attempted: {}\n",
            self.low_vector_diagnostic_page_restore_before_eret_attempted
        ));
        output.push_str(&format!(
            "Low vector diagnostic page entry IPA: {}\n",
            render_optional_u64(self.low_vector_diagnostic_page_entry_ipa)
        ));
        output.push_str(&format!(
            "Low vector diagnostic page previous descriptor: {}\n",
            render_optional_u64(self.low_vector_diagnostic_page_previous_descriptor)
        ));
        output.push_str(&format!(
            "Low vector diagnostic page descriptor: {}\n",
            render_optional_u64(self.low_vector_diagnostic_page_descriptor)
        ));
        output.push_str(&format!(
            "Low vector diagnostic page repeated fault observed: {}\n",
            self.low_vector_diagnostic_page_repeated_fault_observed
        ));
        output.push_str(&format!(
            "Low vector recommended-vector remap requested: {}\n",
            self.low_vector_recommended_vector_remap_requested
        ));
        output.push_str(&format!(
            "Low vector recommended-vector remap attempted: {}\n",
            self.low_vector_recommended_vector_remap_attempted
        ));
        output.push_str(&format!(
            "Low vector recommended-vector remap succeeded: {}\n",
            self.low_vector_recommended_vector_remap_succeeded
        ));
        output.push_str(&format!(
            "Low vector recommended-vector remap target PA: {}\n",
            render_optional_u64(self.low_vector_recommended_vector_remap_target_physical_address)
        ));
        output.push_str(&format!(
            "Low vector recommended-vector remap descriptor: {}\n",
            render_optional_u64(self.low_vector_recommended_vector_remap_descriptor)
        ));
        output.push_str(&format!(
            "Continue after low-vector repair requested: {}\n",
            self.low_vector_post_repair_continue_requested
        ));
        output.push_str(&format!(
            "Continue after low-vector repair attempted: {}\n",
            self.low_vector_post_repair_continue_attempted
        ));
        output.push_str(&format!(
            "Stop at first post-repair device boundary requested: {}\n",
            self.stop_at_first_post_repair_device_boundary_requested
        ));
        output.push_str(&format!(
            "Post-repair unsupported exit observed: {}\n",
            self.low_vector_post_repair_unsupported_exit_observed
        ));
        output.push_str(&format!(
            "Post-repair unsupported exit reason name: {}\n",
            render_optional_exit_reason_name(self.low_vector_post_repair_unsupported_exit_reason)
        ));
        output.push_str(&format!(
            "Post-repair unsupported exit classification: {}\n",
            self.low_vector_post_repair_unsupported_exit_diagnosis
        ));
        let post_repair_first_exit = self.low_vector_post_repair_first_exit_telemetry();
        let post_repair_first_exit_context =
            low_vector_post_repair_context_exit(&self.exits, post_repair_first_exit.index);
        append_low_vector_post_repair_exit_telemetry(
            &mut output,
            "Post-repair first exit",
            &post_repair_first_exit,
            "Post-repair first interaction kind",
            post_repair_first_exit_context,
        );
        let post_repair_first_device_interaction =
            self.low_vector_post_repair_first_device_interaction_telemetry();
        let post_repair_first_device_interaction_context = low_vector_post_repair_context_exit(
            &self.exits,
            post_repair_first_device_interaction.index,
        );
        append_low_vector_post_repair_exit_telemetry(
            &mut output,
            "Post-repair first device interaction",
            &post_repair_first_device_interaction,
            "Post-repair first device interaction kind",
            post_repair_first_device_interaction_context,
        );
        let post_repair_first_unhandled_access =
            self.low_vector_post_repair_first_unhandled_access_telemetry();
        append_low_vector_post_repair_unhandled_access_telemetry(
            &mut output,
            "Post-repair first unhandled access",
            &post_repair_first_unhandled_access,
        );
        output.push_str(&format!(
            "Low vector diagnostic page resume attempted: {}\n",
            self.low_vector_diagnostic_page_resume_attempted
        ));
        output.push_str(&format!(
            "Low vector diagnostic page resume armed: {}\n",
            self.low_vector_diagnostic_page_resume_armed
        ));
        output.push_str(&format!(
            "Low vector diagnostic page resume original PC: {}\n",
            render_optional_u64(self.low_vector_diagnostic_page_resume_original_pc)
        ));
        output.push_str(&format!(
            "Low vector diagnostic page resume original ELR_EL1: {}\n",
            render_optional_u64(self.low_vector_diagnostic_page_resume_original_elr_el1)
        ));
        output.push_str(&format!(
            "Low vector diagnostic page resume original ESR_EL1: {}\n",
            render_optional_u64(self.low_vector_diagnostic_page_resume_original_esr_el1)
        ));
        output.push_str(&format!(
            "Low vector diagnostic page resume original FAR_EL1: {}\n",
            render_optional_u64(self.low_vector_diagnostic_page_resume_original_far_el1)
        ));
        output.push_str(&format!(
            "Low vector diagnostic page resume original SPSR_EL1: {}\n",
            render_optional_u64(self.low_vector_diagnostic_page_resume_original_spsr_el1)
        ));
        output.push_str(&format!(
            "Low vector diagnostic page original slot bytes: {}\n",
            self.low_vector_diagnostic_page_original_slot_bytes
                .as_ref()
                .map_or_else(
                    || "not observed".to_string(),
                    |bytes| render_hex_bytes(bytes)
                )
        ));
        let original_sync_instruction = self
            .low_vector_diagnostic_page_original_slot_bytes
            .and_then(|bytes| Some(u32::from_le_bytes(bytes[0..4].try_into().ok()?)));
        output.push_str(&format!(
            "Low vector diagnostic page original sync instruction: {}\n",
            render_optional_instruction_word(original_sync_instruction)
        ));
        output.push_str(&format!(
            "Low vector diagnostic page original sync hint: {}\n",
            original_sync_instruction
                .map(aarch64_instruction_hint)
                .unwrap_or("not observed")
        ));
        output.push_str(&format!(
            "Low vector diagnostic page resume target instruction before ERET: {}\n",
            render_optional_instruction_word(
                self.low_vector_diagnostic_page_resume_target_instruction_before_eret,
            )
        ));
        output.push_str(&format!(
            "Low vector diagnostic page resume target hint before ERET: {}\n",
            self.low_vector_diagnostic_page_resume_target_instruction_before_eret
                .map(aarch64_instruction_hint)
                .unwrap_or("not observed")
        ));
        output.push_str(&format!(
            "Low vector diagnostic page resume target stage-1 descriptor before ERET: {}\n",
            render_optional_u64(
                self.low_vector_diagnostic_page_resume_target_stage1_leaf_descriptor_before_eret,
            )
        ));
        output.push_str(&format!(
            "Low vector diagnostic page resume target stage-1 kind before ERET: {}\n",
            self.low_vector_diagnostic_page_resume_target_stage1_leaf_kind_before_eret
        ));
        output.push_str(&format!(
            "Low vector diagnostic page resume target is installed diagnostic HVC before ERET: {}\n",
            self.low_vector_diagnostic_page_resume_target_is_installed_diagnostic_hvc_before_eret
        ));
        output.push_str(&format!("vCPU created: {}\n", self.vcpu_created));
        output.push_str(&format!("PC set: {}\n", self.pc_set));
        output.push_str(&format!("X0 DTB IPA set: {}\n", self.x0_dtb_ipa_set));
        output.push_str(&format!("CPSR set: {}\n", self.cpsr_set));
        output.push_str(&format!("SP_EL1 set: {}\n", self.sp_el1_set));
        output.push_str(&format!(
            "Diagnostic vector VBAR_EL1 set: {}\n",
            self.diagnostic_vector_vbar_el1_set
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR requested: {}\n",
            self.recommended_vector_base_vbar_requested
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR attempted: {}\n",
            self.recommended_vector_base_vbar_attempted
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR set: {}\n",
            self.recommended_vector_base_vbar_set
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR diagnostic vector populated: {}\n",
            self.recommended_vector_base_vbar_diagnostic_vector_populated
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR resume requested: {}\n",
            self.recommended_vector_base_vbar_resume_requested
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR resume attempted: {}\n",
            self.recommended_vector_base_vbar_resume_attempted
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR resume armed: {}\n",
            self.recommended_vector_base_vbar_resume_armed
        ));
        output.push_str(&format!(
            "Interrupt/timer wiring requested: {}\n",
            self.interrupt_timer_wiring_requested
        ));
        output.push_str(&format!(
            "Interrupt/timer initialized: {}\n",
            self.interrupt_timer_initialized
        ));
        output.push_str(&format!(
            "Run loop attempted: {}\n",
            self.run_loop_attempted
        ));
        output.push_str(&format!(
            "Firmware progress observed: {}\n",
            self.firmware_progress_observed
        ));
        output.push_str(&format!(
            "Unsupported exit observed: {}\n",
            self.unsupported_exit_observed
        ));
        output.push_str(&format!(
            "Watchdog cancel fired: {}\n",
            self.watchdog_cancel_fired
        ));
        output.push_str(&format!("vCPU destroyed: {}\n", self.vcpu_destroyed));
        output.push_str(&format!(
            "Firmware memory unmapped: {}\n",
            self.firmware_memory_unmapped
        ));
        output.push_str(&format!(
            "Vars memory unmapped: {}\n",
            self.vars_memory_unmapped
        ));
        output.push_str(&format!(
            "Guest RAM memory unmapped: {}\n",
            self.guest_ram_memory_unmapped
        ));
        output.push_str(&format!(
            "Firmware memory deallocated: {}\n",
            self.firmware_memory_deallocated
        ));
        output.push_str(&format!(
            "Vars memory deallocated: {}\n",
            self.vars_memory_deallocated
        ));
        output.push_str(&format!(
            "Guest RAM memory deallocated: {}\n",
            self.guest_ram_memory_deallocated
        ));
        output.push_str(&format!("VM destroyed: {}\n", self.vm_destroyed));
        output.push_str(&format!(
            "Pflash map verified: {}\n",
            self.pflash_map_verified
        ));
        output.push_str(&format!("Reset vector IPA: {:#x}\n", self.reset_vector_ipa));
        output.push_str(&format!(
            "Firmware slot IPA: {:#x}\n",
            self.firmware_slot_ipa
        ));
        output.push_str(&format!("Vars slot IPA: {:#x}\n", self.vars_slot_ipa));
        output.push_str(&format!(
            "Low firmware alias IPA: {:#x}\n",
            self.low_firmware_alias_ipa
        ));
        output.push_str(&format!(
            "Low vars alias IPA: {:#x}\n",
            self.low_vars_alias_ipa
        ));
        output.push_str(&format!("Guest RAM IPA: {:#x}\n", self.guest_ram_ipa));
        output.push_str(&format!("Platform DTB IPA: {:#x}\n", self.platform_dtb_ipa));
        output.push_str(&format!(
            "Platform DTB guest RAM offset: {:#x}\n",
            self.platform_dtb_guest_ram_offset
        ));
        output.push_str(&format!("SP_EL1 seed IPA: {:#x}\n", self.sp_el1_seed_ipa));
        output.push_str(&format!(
            "Diagnostic vector location: {}\n",
            self.diagnostic_vector_location
        ));
        output.push_str(&format!(
            "Diagnostic vector IPA: {:#x}\n",
            self.diagnostic_vector_ipa
        ));
        output.push_str(&format!(
            "Diagnostic vector bytes: {:#x}\n",
            self.diagnostic_vector_bytes
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR source exit: {}\n",
            render_optional_intid(self.recommended_vector_base_vbar_source_exit_index)
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR target: {}\n",
            render_optional_u64(self.recommended_vector_base_vbar_target)
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR target PA: {}\n",
            render_optional_u64(self.recommended_vector_base_vbar_target_physical_address)
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR reason: {}\n",
            self.recommended_vector_base_vbar_reason
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR current EL/SPx sync instruction: {}\n",
            render_optional_instruction_word(
                self.recommended_vector_base_vbar_current_el_spx_sync_instruction_word,
            )
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR current EL/SPx sync hint: {}\n",
            self.recommended_vector_base_vbar_current_el_spx_sync_instruction_hint
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR follow-up exit observed: {}\n",
            self.recommended_vector_base_vbar_followup_exit_observed
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR follow-up exit: {}\n",
            render_optional_intid(self.recommended_vector_base_vbar_followup_exit_index)
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR follow-up exit reason name: {}\n",
            render_optional_exit_reason_name(
                self.recommended_vector_base_vbar_followup_exit_reason
            )
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR follow-up classification: {}\n",
            self.recommended_vector_base_vbar_followup_exit_diagnosis
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR follow-up PC: {}\n",
            render_optional_u64(self.recommended_vector_base_vbar_followup_pc)
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR follow-up VBAR_EL1: {}\n",
            render_optional_u64(self.recommended_vector_base_vbar_followup_vbar_el1)
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR follow-up target still set: {}\n",
            self.recommended_vector_base_vbar_followup_target_still_set
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR resume original PC: {}\n",
            render_optional_u64(self.recommended_vector_base_vbar_resume_original_pc)
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR resume original ELR_EL1: {}\n",
            render_optional_u64(self.recommended_vector_base_vbar_resume_original_elr_el1)
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR resume original ESR_EL1: {}\n",
            render_optional_u64(self.recommended_vector_base_vbar_resume_original_esr_el1)
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR resume original FAR_EL1: {}\n",
            render_optional_u64(self.recommended_vector_base_vbar_resume_original_far_el1)
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR resume original SPSR_EL1: {}\n",
            render_optional_u64(self.recommended_vector_base_vbar_resume_original_spsr_el1)
        ));
        output.push_str(&format!("Slot bytes: {:#x}\n", self.slot_bytes));
        output.push_str(&format!("Guest RAM bytes: {:#x}\n", self.guest_ram_bytes));
        output.push_str(&format!(
            "Platform DTB bytes: {:#x}\n",
            self.platform_dtb_bytes
        ));
        output.push_str(&format!(
            "Platform DTB magic: {:#x}\n",
            self.platform_dtb_magic
        ));
        output.push_str(&format!(
            "Platform DTB magic verified: {}\n",
            self.platform_dtb_magic_verified
        ));
        output.push_str(&format!("Requested exits: {}\n", self.requested_exits));
        output.push_str(&format!("Observed exits: {}\n", self.observed_exits));
        output.push_str(&format!(
            "Watchdog timeout ms: {}\n",
            self.watchdog_timeout_ms
        ));
        output.push_str(&format!(
            "VTimer offset value: {}\n",
            render_optional_u64(self.vtimer_offset_value)
        ));
        output.push_str(&format!(
            "CNTV_CVAL_EL0 value: {}\n",
            render_optional_u64(self.cntv_cval_value)
        ));
        output.push_str(&format!(
            "CNTV_CTL_EL0 value: {}\n",
            render_optional_u64(self.cntv_ctl_value)
        ));
        output.push_str(&format!("VTimer exit count: {}\n", self.vtimer_exit_count));
        output.push_str(&format!(
            "Pending IRQ injected count: {}\n",
            self.pending_irq_injected_count
        ));
        output.push_str(&format!(
            "Device IRQ line asserted count: {}\n",
            self.device_irq_injected_count
        ));
        output.push_str(&format!(
            "Device IRQ line deasserted count: {}\n",
            self.device_irq_cleared_count
        ));
        output.push_str(&format!(
            "Handled MMIO read count: {}\n",
            self.handled_mmio_read_count
        ));
        output.push_str(&format!(
            "Handled MMIO write count: {}\n",
            self.handled_mmio_write_count
        ));
        output.push_str(&format!(
            "Handled PL011 MMIO count: {}\n",
            self.handled_pl011_mmio_count
        ));
        output.push_str(&format!(
            "Handled PL031 MMIO count: {}\n",
            self.handled_pl031_mmio_count
        ));
        output.push_str(&format!(
            "Handled GICD MMIO count: {}\n",
            self.handled_gicd_mmio_count
        ));
        output.push_str(&format!(
            "Handled GICR MMIO count: {}\n",
            self.handled_gicr_mmio_count
        ));
        output.push_str(&format!(
            "Handled VirtIO installer ISO MMIO count: {}\n",
            self.handled_virtio_installer_iso_mmio_count
        ));
        output.push_str(&format!(
            "Handled VirtIO target disk MMIO count: {}\n",
            self.handled_virtio_target_disk_mmio_count
        ));
        output.push_str(&format!(
            "VirtIO queue_notify count: {}\n",
            self.virtio_queue_notify_count
        ));
        output.push_str(&format!(
            "VirtIO request completion count: {}\n",
            self.virtio_request_completion_count
        ));
        output.push_str(&format!(
            "Handled ICC read count: {}\n",
            self.handled_icc_read_count
        ));
        output.push_str(&format!(
            "Handled ICC write count: {}\n",
            self.handled_icc_write_count
        ));
        output.push_str(&format!(
            "Handled ICC_IAR1 read count: {}\n",
            self.handled_icc_iar1_read_count
        ));
        output.push_str(&format!(
            "Handled ICC_EOIR1 write count: {}\n",
            self.handled_icc_eoir1_write_count
        ));
        output.push_str(&format!(
            "Handled ICC_DIR write count: {}\n",
            self.handled_icc_dir_write_count
        ));
        output.push_str(&format!(
            "Last ICC_IAR1 INTID: {}\n",
            render_optional_intid(self.last_icc_iar1_intid)
        ));
        output.push_str(&format!(
            "Last ICC_EOIR1 INTID: {}\n",
            render_optional_intid(self.last_icc_eoir1_intid)
        ));
        output.push_str(&format!(
            "Last ICC_DIR INTID: {}\n",
            render_optional_intid(self.last_icc_dir_intid)
        ));
        output.push_str(&format!(
            "Firmware source bytes: {}\n",
            render_optional_u64(self.firmware_source_bytes)
        ));
        output.push_str(&format!(
            "Vars source bytes: {}\n",
            render_optional_u64(self.vars_source_bytes)
        ));
        output.push_str(&format!(
            "Installer ISO path: {}\n",
            self.installer_iso_path.as_ref().map_or_else(
                || "not provided".to_string(),
                |path| path.display().to_string()
            )
        ));
        output.push_str(&format!(
            "Writable target disk path: {}\n",
            self.writable_target_disk_path.as_ref().map_or_else(
                || "not provided".to_string(),
                |path| path.display().to_string()
            )
        ));
        output.push_str("Firmware block devices:\n");
        for device in &self.block_devices {
            output.push_str(&format!(
                "- role={}, label={}, node={}, base={:#x}, bytes={:#x}, read_only={}, backing_kind={}, backing_path={}, device_features={:#x}, capacity_sectors={:#x}\n",
                device.role,
                device.label,
                device.node_name,
                device.base_ipa,
                device.bytes,
                device.read_only,
                device.backing_kind,
                device
                    .backing_path
                    .as_ref()
                    .map_or_else(|| "not provided".to_string(), |path| path.display().to_string()),
                device.device_features,
                device.capacity_sectors,
            ));
        }
        output.push_str(&format!(
            "Firmware map flags: {}\n",
            self.firmware_map_flags
        ));
        output.push_str(&format!("Vars map flags: {}\n", self.vars_map_flags));
        output.push_str(&format!(
            "Low firmware alias map flags: {}\n",
            self.low_firmware_alias_map_flags
        ));
        output.push_str(&format!(
            "Low vars alias map flags: {}\n",
            self.low_vars_alias_map_flags
        ));
        output.push_str(&format!(
            "Guest RAM map flags: {}\n",
            self.guest_ram_map_flags
        ));
        output.push_str(&format!(
            "Low pflash alias requested: {}\n",
            self.low_pflash_alias_requested
        ));
        output.push_str(&format!(
            "VM create status name: {}\n",
            render_optional_status_name(self.vm_create_status)
        ));
        output.push_str(&format!(
            "Firmware allocate status name: {}\n",
            render_optional_status_name(self.firmware_allocate_status)
        ));
        output.push_str(&format!(
            "Vars allocate status name: {}\n",
            render_optional_status_name(self.vars_allocate_status)
        ));
        output.push_str(&format!(
            "Guest RAM allocate status name: {}\n",
            render_optional_status_name(self.guest_ram_allocate_status)
        ));
        output.push_str(&format!(
            "Firmware map status name: {}\n",
            render_optional_status_name(self.firmware_map_status)
        ));
        output.push_str(&format!(
            "Vars map status name: {}\n",
            render_optional_status_name(self.vars_map_status)
        ));
        output.push_str(&format!(
            "Low firmware alias map status name: {}\n",
            render_optional_status_name(self.low_firmware_alias_map_status)
        ));
        output.push_str(&format!(
            "Low vars alias map status name: {}\n",
            render_optional_status_name(self.low_vars_alias_map_status)
        ));
        output.push_str(&format!(
            "Guest RAM map status name: {}\n",
            render_optional_status_name(self.guest_ram_map_status)
        ));
        output.push_str(&format!(
            "vCPU create status name: {}\n",
            render_optional_status_name(self.vcpu_create_status)
        ));
        output.push_str(&format!(
            "PC set status name: {}\n",
            render_optional_status_name(self.pc_set_status)
        ));
        output.push_str(&format!(
            "X0 DTB IPA set status name: {}\n",
            render_optional_status_name(self.x0_dtb_ipa_set_status)
        ));
        output.push_str(&format!(
            "CPSR set status name: {}\n",
            render_optional_status_name(self.cpsr_set_status)
        ));
        output.push_str(&format!(
            "SP_EL1 set status name: {}\n",
            render_optional_status_name(self.sp_el1_set_status)
        ));
        output.push_str(&format!(
            "Diagnostic vector VBAR_EL1 set status name: {}\n",
            render_optional_status_name(self.diagnostic_vector_vbar_el1_set_status)
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR set status name: {}\n",
            render_optional_status_name(self.recommended_vector_base_vbar_set_status)
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR resume ELR_EL1 set status name: {}\n",
            render_optional_status_name(
                self.recommended_vector_base_vbar_resume_elr_el1_set_status,
            )
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR resume VBAR_EL1 set status name: {}\n",
            render_optional_status_name(
                self.recommended_vector_base_vbar_resume_vbar_el1_set_status,
            )
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR resume SPSR_EL1 set status name: {}\n",
            render_optional_status_name(
                self.recommended_vector_base_vbar_resume_spsr_el1_set_status,
            )
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR resume PC set status name: {}\n",
            render_optional_status_name(self.recommended_vector_base_vbar_resume_pc_set_status)
        ));
        output.push_str(&format!(
            "Low vector diagnostic page resume ELR_EL1 set status name: {}\n",
            render_optional_status_name(self.low_vector_diagnostic_page_resume_elr_el1_set_status)
        ));
        output.push_str(&format!(
            "Low vector diagnostic page resume SPSR_EL1 set status name: {}\n",
            render_optional_status_name(self.low_vector_diagnostic_page_resume_spsr_el1_set_status)
        ));
        output.push_str(&format!(
            "Low vector diagnostic page resume CPSR set status name: {}\n",
            render_optional_status_name(self.low_vector_diagnostic_page_resume_cpsr_set_status)
        ));
        output.push_str(&format!(
            "Low vector diagnostic page resume PC set status name: {}\n",
            render_optional_status_name(self.low_vector_diagnostic_page_resume_pc_set_status)
        ));
        output.push_str(&format!(
            "VTimer offset set status name: {}\n",
            render_optional_status_name(self.vtimer_offset_set_status)
        ));
        output.push_str(&format!(
            "CNTV_CVAL_EL0 set status name: {}\n",
            render_optional_status_name(self.cntv_cval_set_status)
        ));
        output.push_str(&format!(
            "CNTV_CTL_EL0 set status name: {}\n",
            render_optional_status_name(self.cntv_ctl_set_status)
        ));
        output.push_str(&format!(
            "VTimer initial unmask status name: {}\n",
            render_optional_status_name(self.vtimer_initial_unmask_status)
        ));
        output.push_str(&format!(
            "Last pending IRQ set status name: {}\n",
            render_optional_status_name(self.last_pending_irq_set_status)
        ));
        output.push_str(&format!(
            "Last device IRQ line assert status name: {}\n",
            render_optional_status_name(self.last_device_irq_set_status)
        ));
        output.push_str(&format!(
            "Last device IRQ line deassert status name: {}\n",
            render_optional_status_name(self.last_device_irq_clear_status)
        ));
        output.push_str(&format!(
            "Last VTimer unmask status name: {}\n",
            render_optional_status_name(self.last_vtimer_unmask_status)
        ));
        output.push_str(&format!(
            "Final PC status name: {}\n",
            render_optional_status_name(self.final_pc_status)
        ));
        output.push_str(&format!(
            "Final PC: {}\n",
            render_optional_u64(self.final_pc)
        ));
        output.push_str("Run-loop exits:\n");
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct UefiFirmwareFileVerification {
    bytes: u64,
    volume: UefiFirmwareVolumeMetadata,
}

pub fn probe_windows_11_arm_uefi_firmware_handoff(
    options: WindowsArmUefiFirmwareHandoffOptions,
) -> WindowsArmUefiFirmwareHandoffProbe {
    let mut blockers = Vec::new();
    let mut firmware_bytes = None;
    let mut firmware_volume = None;
    let mut firmware_verified = false;
    let mut vars_template_bytes = None;
    let mut vars_template_verified = false;
    let mut vars_bytes = None;
    let mut vars_created = false;
    let mut vars_reopened_for_verification = false;
    let mut vars_volume = None;
    let mut vars_verified = false;

    match verify_uefi_firmware_file(&options.firmware_path, WINDOWS_ARM_UEFI_SLOT_BYTES) {
        Ok(verification) => {
            firmware_bytes = Some(verification.bytes);
            firmware_volume = Some(verification.volume);
            firmware_verified = true;
        }
        Err(error) => blockers.push(format!("firmware verification failed: {error}")),
    }

    if let Some(template_path) = &options.vars_template_path {
        match verify_uefi_firmware_file(template_path, WINDOWS_ARM_UEFI_SLOT_BYTES) {
            Ok(verification) => {
                vars_template_bytes = Some(verification.bytes);
                vars_template_verified = true;
            }
            Err(error) => blockers.push(format!("vars template verification failed: {error}")),
        }
    }

    if options.create_vars {
        match (&options.vars_template_path, &options.vars_path) {
            (Some(template_path), Some(vars_path)) => {
                if vars_path.exists() {
                    blockers.push(format!(
                        "vars path already exists; refusing to overwrite {}",
                        vars_path.display()
                    ));
                } else if vars_template_verified {
                    match copy_uefi_vars_template(template_path, vars_path) {
                        Ok(()) => {
                            vars_created = true;
                            match verify_uefi_firmware_file(vars_path, WINDOWS_ARM_UEFI_SLOT_BYTES)
                            {
                                Ok(verification) => {
                                    vars_bytes = Some(verification.bytes);
                                    vars_volume = Some(verification.volume);
                                    vars_reopened_for_verification = true;
                                    vars_verified = true;
                                }
                                Err(error) => blockers.push(format!(
                                    "created vars store verification failed: {error}"
                                )),
                            }
                        }
                        Err(error) => blockers.push(format!("vars creation failed: {error}")),
                    }
                }
            }
            (None, _) => blockers.push(
                "--vars-template is required with --create-vars for a mutable UEFI variable store"
                    .to_string(),
            ),
            (_, None) => blockers.push(
                "--vars is required with --create-vars for a mutable UEFI variable store"
                    .to_string(),
            ),
        }
    } else if let Some(vars_path) = &options.vars_path {
        match verify_uefi_firmware_file(vars_path, WINDOWS_ARM_UEFI_SLOT_BYTES) {
            Ok(verification) => {
                vars_bytes = Some(verification.bytes);
                vars_volume = Some(verification.volume);
                vars_reopened_for_verification = true;
                vars_verified = true;
            }
            Err(error) => blockers.push(format!("vars store verification failed: {error}")),
        }
    } else if options.vars_template_path.is_some() {
        blockers.push(
            "vars template was verified, but no mutable --vars path was supplied".to_string(),
        );
    } else {
        blockers.push("UEFI variable store is required for Windows firmware handoff".to_string());
    }

    let planned_reset_vector_ipa =
        (firmware_verified && vars_verified).then_some(WINDOWS_ARM_UEFI_CODE_IPA);

    WindowsArmUefiFirmwareHandoffProbe {
        firmware_path: options.firmware_path,
        firmware_bytes,
        firmware_slot_ipa: WINDOWS_ARM_UEFI_CODE_IPA,
        firmware_slot_bytes: WINDOWS_ARM_UEFI_SLOT_BYTES,
        firmware_volume,
        firmware_verified,
        vars_template_path: options.vars_template_path,
        vars_template_bytes,
        vars_template_verified,
        vars_path: options.vars_path,
        vars_bytes,
        vars_slot_ipa: WINDOWS_ARM_UEFI_VARS_IPA,
        vars_slot_bytes: WINDOWS_ARM_UEFI_SLOT_BYTES,
        vars_created,
        vars_reopened_for_verification,
        vars_volume,
        vars_verified,
        planned_reset_vector_ipa,
        blockers,
    }
}

pub fn probe_windows_11_arm_uefi_pflash_map(
    options: WindowsArmUefiPflashMapOptions,
) -> WindowsArmUefiPflashMapProbe {
    let handoff =
        probe_windows_11_arm_uefi_firmware_handoff(WindowsArmUefiFirmwareHandoffOptions {
            firmware_path: options.firmware_path,
            vars_template_path: options.vars_template_path,
            vars_path: options.vars_path,
            create_vars: options.create_vars,
        });
    let mut blockers = handoff.blockers.clone();

    let firmware_slot = if handoff.firmware_verified {
        match load_uefi_pflash_slot(
            "code",
            &handoff.firmware_path,
            WINDOWS_ARM_UEFI_CODE_IPA,
            WINDOWS_ARM_UEFI_SLOT_BYTES,
            false,
        ) {
            Ok(slot) => Some(slot),
            Err(error) => {
                blockers.push(format!("firmware pflash load failed: {error}"));
                None
            }
        }
    } else {
        None
    };

    let vars_slot = if handoff.vars_verified {
        match &handoff.vars_path {
            Some(vars_path) => match load_uefi_pflash_slot(
                "vars",
                vars_path,
                WINDOWS_ARM_UEFI_VARS_IPA,
                WINDOWS_ARM_UEFI_SLOT_BYTES,
                true,
            ) {
                Ok(slot) => Some(slot),
                Err(error) => {
                    blockers.push(format!("vars pflash load failed: {error}"));
                    None
                }
            },
            None => {
                blockers.push("verified vars store has no path for pflash mapping".to_string());
                None
            }
        }
    } else {
        None
    };

    let firmware_slot_loaded = firmware_slot
        .as_ref()
        .is_some_and(|slot| slot.prefix_verified && slot.padding_zeroed);
    let vars_slot_loaded = vars_slot
        .as_ref()
        .is_some_and(|slot| slot.prefix_verified && slot.padding_zeroed);

    let pflash_slots_non_overlapping = match (&firmware_slot, &vars_slot) {
        (Some(firmware_slot), Some(vars_slot)) => {
            firmware_slot.ipa_start == WINDOWS_ARM_UEFI_CODE_IPA
                && firmware_slot.ipa_end_exclusive() == WINDOWS_ARM_UEFI_VARS_IPA
                && vars_slot.ipa_start == WINDOWS_ARM_UEFI_VARS_IPA
                && vars_slot.ipa_end_exclusive() == WINDOWS_ARM_DEVICE_MMIO_IPA
                && !ipa_ranges_overlap(
                    firmware_slot.ipa_start,
                    firmware_slot.slot_bytes,
                    vars_slot.ipa_start,
                    vars_slot.slot_bytes,
                )
        }
        _ => false,
    };
    let guest_ram_overlap_verified = [&firmware_slot, &vars_slot]
        .into_iter()
        .flatten()
        .all(|slot| slot.ipa_end_exclusive() <= WINDOWS_ARM_GUEST_RAM_IPA);
    let device_mmio_overlap_verified =
        [&firmware_slot, &vars_slot]
            .into_iter()
            .flatten()
            .all(|slot| {
                !ipa_ranges_overlap(
                    slot.ipa_start,
                    slot.slot_bytes,
                    WINDOWS_ARM_DEVICE_MMIO_IPA,
                    WINDOWS_ARM_DEVICE_MMIO_BYTES,
                )
            });
    let pflash_map_verified = firmware_slot_loaded
        && vars_slot_loaded
        && pflash_slots_non_overlapping
        && guest_ram_overlap_verified
        && device_mmio_overlap_verified;

    if (firmware_slot_loaded || vars_slot_loaded) && !pflash_slots_non_overlapping {
        blockers.push("pflash code/vars IPA range verification failed".to_string());
    }
    if (firmware_slot_loaded || vars_slot_loaded) && !guest_ram_overlap_verified {
        blockers.push("pflash slots overlap the planned guest RAM window".to_string());
    }
    if (firmware_slot_loaded || vars_slot_loaded) && !device_mmio_overlap_verified {
        blockers.push("pflash slots overlap the planned device MMIO window".to_string());
    }

    WindowsArmUefiPflashMapProbe {
        firmware_path: handoff.firmware_path,
        vars_path: handoff.vars_path,
        vars_created: handoff.vars_created,
        firmware_verified: handoff.firmware_verified,
        vars_verified: handoff.vars_verified,
        firmware_slot,
        vars_slot,
        pflash_region_start: WINDOWS_ARM_UEFI_CODE_IPA,
        pflash_region_bytes: WINDOWS_ARM_UEFI_PFLASH_BYTES,
        pflash_slots_non_overlapping,
        guest_ram_overlap_verified,
        device_mmio_overlap_verified,
        pflash_map_verified,
        planned_reset_vector_ipa: pflash_map_verified.then_some(WINDOWS_ARM_UEFI_CODE_IPA),
        blockers,
    }
}

pub fn probe_windows_11_arm_uefi_pflash_hvf_map(
    options: WindowsArmUefiPflashMapOptions,
    allow_map: bool,
) -> WindowsArmUefiPflashHvfMapProbe {
    let pflash_map = probe_windows_11_arm_uefi_pflash_map(options);
    let host = query_hvf_host_capabilities();
    platform::probe_windows_11_arm_uefi_pflash_hvf_map(allow_map, pflash_map, host)
}

pub fn probe_windows_11_arm_uefi_reset_vector_entry(
    options: WindowsArmUefiPflashMapOptions,
    allow_entry: bool,
) -> WindowsArmUefiResetVectorEntryProbe {
    let pflash_map = probe_windows_11_arm_uefi_pflash_map(options);
    let host = query_hvf_host_capabilities();
    platform::probe_windows_11_arm_uefi_reset_vector_entry(allow_entry, pflash_map, host)
}

pub fn probe_windows_11_arm_uefi_firmware_run_loop(
    options: WindowsArmUefiFirmwareRunLoopOptions,
) -> WindowsArmUefiFirmwareRunLoopProbe {
    let WindowsArmUefiFirmwareRunLoopOptions { pflash, execution } = options;
    let pflash_map = probe_windows_11_arm_uefi_pflash_map(pflash);
    let host = query_hvf_host_capabilities();
    platform::probe_windows_11_arm_uefi_firmware_run_loop(execution, pflash_map, host)
}

pub fn probe_windows_11_arm_uefi_firmware_device_discovery(
    options: WindowsArmUefiFirmwareRunLoopOptions,
) -> WindowsArmUefiFirmwareDeviceDiscoveryProbe {
    let mut options = options;
    options.execution.map_low_pflash_alias = true;
    options.execution.repair_low_vector_diagnostic_page = true;
    options.execution.continue_after_low_vector_repair = true;
    options.execution.wire_interrupt_timer = true;
    options.execution.stop_at_first_post_repair_device_boundary = true;
    WindowsArmUefiFirmwareDeviceDiscoveryProbe {
        run_loop: probe_windows_11_arm_uefi_firmware_run_loop(options),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct WindowsArmBootDiskLayoutVerification {
    protective_mbr_verified: bool,
    primary_gpt_verified: bool,
    backup_gpt_verified: bool,
    partition_entries_verified: bool,
    disk_size_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct GptHeader {
    current_lba: u64,
    backup_lba: u64,
    first_usable_lba: u64,
    last_usable_lba: u64,
    entries_lba: u64,
    entry_count: u32,
    entry_size: u32,
    entries_crc32: u32,
}

pub fn probe_windows_11_arm_boot_disk_layout(
    options: WindowsArmBootDiskLayoutOptions,
) -> WindowsArmBootDiskLayoutProbe {
    let mut blockers = Vec::new();
    let requested_size_bytes = match gib_to_bytes(options.size_gib) {
        Some(bytes) if options.size_gib >= WINDOWS_ARM_BOOT_DISK_MIN_SIZE_GIB => bytes,
        Some(_) => {
            blockers.push(format!(
                "--size-gib must be at least {WINDOWS_ARM_BOOT_DISK_MIN_SIZE_GIB} for the Windows Arm GPT layout"
            ));
            0
        }
        None => {
            blockers.push("--size-gib is too large to represent safely".to_string());
            0
        }
    };
    let mut disk_size_bytes = (requested_size_bytes > 0).then_some(requested_size_bytes);
    let mut created = false;
    let mut reopened_for_verification = false;
    let mut verification = WindowsArmBootDiskLayoutVerification {
        protective_mbr_verified: false,
        primary_gpt_verified: false,
        backup_gpt_verified: false,
        partition_entries_verified: false,
        disk_size_bytes: requested_size_bytes,
    };

    if requested_size_bytes > 0 {
        if options.create {
            if options.disk_path.exists() {
                blockers.push(format!(
                    "disk path already exists; refusing to overwrite {}",
                    options.disk_path.display()
                ));
            } else {
                match write_windows_arm_boot_disk_layout(&options.disk_path, requested_size_bytes) {
                    Ok(()) => {
                        created = true;
                        match verify_windows_arm_boot_disk_layout(&options.disk_path) {
                            Ok(result) => {
                                reopened_for_verification = true;
                                disk_size_bytes = Some(result.disk_size_bytes);
                                verification = result;
                            }
                            Err(error) => blockers.push(format!(
                                "created disk could not be reopened and verified: {error}"
                            )),
                        }
                    }
                    Err(error) => blockers.push(format!("create failed: {error}")),
                }
            }
        } else {
            match std::fs::metadata(&options.disk_path) {
                Ok(metadata) => {
                    disk_size_bytes = Some(metadata.len());
                    match verify_windows_arm_boot_disk_layout(&options.disk_path) {
                        Ok(result) => {
                            reopened_for_verification = true;
                            verification = result;
                        }
                        Err(error) => blockers
                            .push(format!("existing disk layout verification failed: {error}")),
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => blockers.push(
                    "disk file does not exist; pass --create to write a sparse raw GPT layout"
                        .to_string(),
                ),
                Err(error) => blockers.push(format!("disk metadata read failed: {error}")),
            }
        }
    }

    let partitions = disk_size_bytes
        .and_then(|bytes| windows_arm_boot_disk_partitions(bytes).ok())
        .unwrap_or_default();

    WindowsArmBootDiskLayoutProbe {
        disk_path: options.disk_path,
        requested_size_gib: options.size_gib,
        disk_size_bytes,
        create_requested: options.create,
        created,
        reopened_for_verification,
        protective_mbr_verified: verification.protective_mbr_verified,
        primary_gpt_verified: verification.primary_gpt_verified,
        backup_gpt_verified: verification.backup_gpt_verified,
        partition_entries_verified: verification.partition_entries_verified,
        partitions,
        blockers,
    }
}

pub fn probe_windows_11_arm_platform_description(
    options: WindowsArmPlatformDescriptionOptions,
) -> WindowsArmPlatformDescriptionProbe {
    let fdt_blob = build_windows_arm_platform_fdt_blob(&options);
    let summary = inspect_windows_arm_platform_fdt_blob(&fdt_blob);
    let device_mmio_end_ipa =
        WINDOWS_ARM_DEVICE_MMIO_IPA.saturating_add(WINDOWS_ARM_DEVICE_MMIO_BYTES);
    let mmio_nodes = vec![
        WindowsArmFdtMmioNodeCheck {
            label: "PL011",
            node_name: "serial@10000000",
            base_ipa: summary.pl011.map(|range| range.base_ipa),
            bytes: summary.pl011.map(|range| range.bytes),
            inside_device_window: summary.pl011.is_some_and(fdt_range_inside_device_window),
        },
        WindowsArmFdtMmioNodeCheck {
            label: "PL031",
            node_name: "rtc@10001000",
            base_ipa: summary.pl031.map(|range| range.base_ipa),
            bytes: summary.pl031.map(|range| range.bytes),
            inside_device_window: summary.pl031.is_some_and(fdt_range_inside_device_window),
        },
        WindowsArmFdtMmioNodeCheck {
            label: "VirtIO-MMIO installer ISO",
            node_name: "virtio_mmio@10002000",
            base_ipa: summary.virtio_installer_iso.map(|range| range.base_ipa),
            bytes: summary.virtio_installer_iso.map(|range| range.bytes),
            inside_device_window: summary
                .virtio_installer_iso
                .is_some_and(fdt_range_inside_device_window),
        },
        WindowsArmFdtMmioNodeCheck {
            label: "VirtIO-MMIO target disk",
            node_name: "virtio_mmio@10003000",
            base_ipa: summary.virtio_target_disk.map(|range| range.base_ipa),
            bytes: summary.virtio_target_disk.map(|range| range.bytes),
            inside_device_window: summary
                .virtio_target_disk
                .is_some_and(fdt_range_inside_device_window),
        },
    ];
    let mmio_nodes_inside_device_window = mmio_nodes.iter().all(|node| node.inside_device_window);
    let gic_nodes_inside_device_window = summary
        .gic_distributor
        .is_some_and(fdt_range_inside_device_window)
        && summary
            .gic_redistributor
            .is_some_and(fdt_range_inside_device_window);
    let arch_timer_node_present = !summary.arch_timer_interrupts.is_empty();
    let arch_timer_interrupt_count = summary.arch_timer_interrupts.len();
    let interrupt_nodes = vec![
        WindowsArmFdtInterruptCheck {
            label: "PL011",
            node_name: "serial@10000000",
            interrupt_type: summary
                .pl011_interrupt
                .map(|interrupt| interrupt.interrupt_type),
            interrupt_number: summary
                .pl011_interrupt
                .map(|interrupt| interrupt.interrupt_number),
            trigger: summary.pl011_interrupt.map(|interrupt| interrupt.trigger),
            described: summary.pl011_interrupt.is_some(),
        },
        WindowsArmFdtInterruptCheck {
            label: "PL031",
            node_name: "rtc@10001000",
            interrupt_type: summary
                .pl031_interrupt
                .map(|interrupt| interrupt.interrupt_type),
            interrupt_number: summary
                .pl031_interrupt
                .map(|interrupt| interrupt.interrupt_number),
            trigger: summary.pl031_interrupt.map(|interrupt| interrupt.trigger),
            described: summary.pl031_interrupt.is_some(),
        },
        WindowsArmFdtInterruptCheck {
            label: "VirtIO-MMIO installer ISO",
            node_name: "virtio_mmio@10002000",
            interrupt_type: summary
                .virtio_installer_iso_interrupt
                .map(|interrupt| interrupt.interrupt_type),
            interrupt_number: summary
                .virtio_installer_iso_interrupt
                .map(|interrupt| interrupt.interrupt_number),
            trigger: summary
                .virtio_installer_iso_interrupt
                .map(|interrupt| interrupt.trigger),
            described: summary.virtio_installer_iso_interrupt.is_some(),
        },
        WindowsArmFdtInterruptCheck {
            label: "VirtIO-MMIO target disk",
            node_name: "virtio_mmio@10003000",
            interrupt_type: summary
                .virtio_target_disk_interrupt
                .map(|interrupt| interrupt.interrupt_type),
            interrupt_number: summary
                .virtio_target_disk_interrupt
                .map(|interrupt| interrupt.interrupt_number),
            trigger: summary
                .virtio_target_disk_interrupt
                .map(|interrupt| interrupt.trigger),
            described: summary.virtio_target_disk_interrupt.is_some(),
        },
    ];
    let interrupt_nodes_described = interrupt_nodes.iter().all(|node| node.described);
    let memory_node_at_guest_ram_base =
        summary.memory_node_base_ipa == Some(WINDOWS_ARM_GUEST_RAM_IPA);
    let cpu_count_verified = summary.cpu_count == options.vcpu_count;
    let mut blockers = summary.blockers;

    if options.guest_ram_bytes == 0 {
        blockers.push("guest RAM FDT reg size must be non-zero".to_string());
    }
    if options.vcpu_count == 0 {
        blockers.push("FDT CPU count must be non-zero for Windows Arm".to_string());
    }
    if summary.fdt_magic != FDT_MAGIC {
        blockers.push("FDT header magic did not match 0xd00dfeed".to_string());
    }
    if !memory_node_at_guest_ram_base {
        blockers.push("FDT memory node is not rooted at the Windows Arm guest RAM IPA".to_string());
    }
    if !cpu_count_verified {
        blockers.push("FDT CPU node count does not match requested vCPU count".to_string());
    }
    if !mmio_nodes_inside_device_window {
        blockers.push(
            "FDT PL011/PL031/VirtIO-MMIO installer ISO/target disk nodes are not fully inside the Windows device window"
                .to_string(),
        );
    }
    if summary.root_interrupt_parent != Some(WINDOWS_ARM_GIC_PHANDLE) {
        blockers.push("FDT root interrupt-parent does not point at the GIC phandle".to_string());
    }
    if summary.gic_phandle != Some(WINDOWS_ARM_GIC_PHANDLE) || !summary.gic_interrupt_controller {
        blockers.push("FDT GICv3 interrupt-controller node is incomplete".to_string());
    }
    if !gic_nodes_inside_device_window {
        blockers.push(
            "FDT GIC distributor/redistributor nodes are not fully inside the Windows device window"
                .to_string(),
        );
    }
    if arch_timer_interrupt_count != 4 {
        blockers.push("FDT ARM arch timer must describe four timer interrupts".to_string());
    }
    if !interrupt_nodes_described {
        blockers
            .push("FDT PL011/PL031/VirtIO-MMIO interrupt properties are incomplete".to_string());
    }

    WindowsArmPlatformDescriptionProbe {
        qemu_used: false,
        apple_vz_used: false,
        hvf_entered: false,
        format: "FDT",
        fdt_blob_bytes: fdt_blob.len(),
        fdt_blob,
        fdt_magic: summary.fdt_magic,
        fdt_magic_verified: summary.fdt_magic == FDT_MAGIC,
        memory_node_base_ipa: summary.memory_node_base_ipa,
        memory_node_at_guest_ram_base,
        requested_cpu_count: options.vcpu_count,
        cpu_count: summary.cpu_count,
        cpu_count_verified,
        device_mmio_start_ipa: WINDOWS_ARM_DEVICE_MMIO_IPA,
        device_mmio_end_ipa,
        mmio_nodes,
        mmio_nodes_inside_device_window,
        root_interrupt_parent: summary.root_interrupt_parent,
        gic_phandle: summary.gic_phandle,
        gic_distributor_base_ipa: summary.gic_distributor.map(|range| range.base_ipa),
        gic_distributor_bytes: summary.gic_distributor.map(|range| range.bytes),
        gic_redistributor_base_ipa: summary.gic_redistributor.map(|range| range.base_ipa),
        gic_redistributor_bytes: summary.gic_redistributor.map(|range| range.bytes),
        gic_nodes_inside_device_window,
        arch_timer_node_present,
        arch_timer_interrupt_count,
        interrupt_nodes,
        interrupt_nodes_described,
        acpi_implemented: false,
        fw_cfg_used: false,
        gic_status: "described/not emulated",
        gic_emulated: false,
        blockers,
    }
}

fn windows_arm_firmware_block_devices(
    installer_iso_path: Option<PathBuf>,
    writable_target_disk_path: Option<PathBuf>,
) -> Vec<WindowsArmVirtioBlockDeviceMetadata> {
    let installer_capacity_sectors =
        windows_arm_block_capacity_sectors(installer_iso_path.as_ref());
    let target_capacity_sectors =
        windows_arm_block_capacity_sectors(writable_target_disk_path.as_ref());
    vec![
        WindowsArmVirtioBlockDeviceMetadata {
            role: "installer-iso",
            label: "VirtIO-MMIO installer ISO",
            node_name: "virtio_mmio@10002000",
            base_ipa: WINDOWS_ARM_VIRTIO_INSTALLER_ISO_MMIO_IPA,
            bytes: VIRTIO_MMIO_REGISTER_WINDOW_BYTES,
            read_only: true,
            backing_kind: "host-iso-readonly",
            backing_path: installer_iso_path,
            device_features: VIRTIO_BLK_F_RO,
            capacity_sectors: installer_capacity_sectors,
        },
        WindowsArmVirtioBlockDeviceMetadata {
            role: "target-disk",
            label: "VirtIO-MMIO target disk",
            node_name: "virtio_mmio@10003000",
            base_ipa: WINDOWS_ARM_VIRTIO_TARGET_DISK_MMIO_IPA,
            bytes: VIRTIO_MMIO_REGISTER_WINDOW_BYTES,
            read_only: false,
            backing_kind: "host-file-writable",
            backing_path: writable_target_disk_path,
            device_features: VIRTIO_MMIO_BLOCK_DEVICE_FEATURES_VALUE,
            capacity_sectors: target_capacity_sectors,
        },
    ]
}

fn windows_arm_block_capacity_sectors(path: Option<&PathBuf>) -> u64 {
    path.and_then(|path| std::fs::metadata(path).ok())
        .map(|metadata| metadata.len() / VIRTIO_BLOCK_SECTOR_BYTES)
        .unwrap_or(VIRTIO_MMIO_BLOCK_CAPACITY_SECTORS)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FdtRegRange {
    base_ipa: u64,
    bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FdtInterruptSpec {
    interrupt_type: u32,
    interrupt_number: u32,
    trigger: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct WindowsArmPlatformFdtSummary {
    fdt_magic: u32,
    memory_node_base_ipa: Option<u64>,
    cpu_count: u8,
    root_interrupt_parent: Option<u32>,
    gic_phandle: Option<u32>,
    gic_interrupt_controller: bool,
    gic_distributor: Option<FdtRegRange>,
    gic_redistributor: Option<FdtRegRange>,
    arch_timer_interrupts: Vec<FdtInterruptSpec>,
    pl011: Option<FdtRegRange>,
    pl011_interrupt: Option<FdtInterruptSpec>,
    pl031: Option<FdtRegRange>,
    pl031_interrupt: Option<FdtInterruptSpec>,
    virtio_installer_iso: Option<FdtRegRange>,
    virtio_installer_iso_interrupt: Option<FdtInterruptSpec>,
    virtio_target_disk: Option<FdtRegRange>,
    virtio_target_disk_interrupt: Option<FdtInterruptSpec>,
    blockers: Vec<String>,
}

#[derive(Default)]
struct FdtBlobBuilder {
    structure: Vec<u8>,
    strings: Vec<u8>,
}

impl FdtBlobBuilder {
    fn begin_node(&mut self, name: &str) {
        push_be_u32(&mut self.structure, FDT_BEGIN_NODE);
        self.structure.extend_from_slice(name.as_bytes());
        self.structure.push(0);
        pad_to_4(&mut self.structure);
    }

    fn end_node(&mut self) {
        push_be_u32(&mut self.structure, FDT_END_NODE);
    }

    fn prop_raw(&mut self, name: &str, data: &[u8]) {
        let name_offset = self.add_string(name);
        push_be_u32(&mut self.structure, FDT_PROP);
        push_be_u32(&mut self.structure, data.len() as u32);
        push_be_u32(&mut self.structure, name_offset);
        self.structure.extend_from_slice(data);
        pad_to_4(&mut self.structure);
    }

    fn prop_u32(&mut self, name: &str, value: u32) {
        self.prop_raw(name, &value.to_be_bytes());
    }

    fn prop_empty(&mut self, name: &str) {
        self.prop_raw(name, &[]);
    }

    fn prop_u32_list(&mut self, name: &str, values: &[u32]) {
        let mut data = Vec::with_capacity(values.len() * 4);
        for value in values {
            data.extend_from_slice(&value.to_be_bytes());
        }
        self.prop_raw(name, &data);
    }

    fn prop_string(&mut self, name: &str, value: &str) {
        let mut data = Vec::with_capacity(value.len() + 1);
        data.extend_from_slice(value.as_bytes());
        data.push(0);
        self.prop_raw(name, &data);
    }

    fn prop_string_list(&mut self, name: &str, values: &[&str]) {
        let mut data = Vec::new();
        for value in values {
            data.extend_from_slice(value.as_bytes());
            data.push(0);
        }
        self.prop_raw(name, &data);
    }

    fn prop_reg64(&mut self, base_ipa: u64, bytes: u64) {
        let mut data = Vec::with_capacity(16);
        data.extend_from_slice(&base_ipa.to_be_bytes());
        data.extend_from_slice(&bytes.to_be_bytes());
        self.prop_raw("reg", &data);
    }

    fn prop_reg64_pairs(&mut self, ranges: &[(u64, u64)]) {
        let mut data = Vec::with_capacity(ranges.len() * 16);
        for (base_ipa, bytes) in ranges {
            data.extend_from_slice(&base_ipa.to_be_bytes());
            data.extend_from_slice(&bytes.to_be_bytes());
        }
        self.prop_raw("reg", &data);
    }

    fn prop_gic_interrupt(&mut self, interrupt_type: u32, interrupt_number: u32, trigger: u32) {
        self.prop_u32_list("interrupts", &[interrupt_type, interrupt_number, trigger]);
    }

    fn add_string(&mut self, name: &str) -> u32 {
        let offset = self.strings.len() as u32;
        self.strings.extend_from_slice(name.as_bytes());
        self.strings.push(0);
        offset
    }

    fn finish(mut self) -> Vec<u8> {
        push_be_u32(&mut self.structure, FDT_END);
        pad_to_4(&mut self.structure);

        let header_bytes = 40_u32;
        let mem_rsvmap_bytes = 16_u32;
        let off_mem_rsvmap = header_bytes;
        let off_dt_struct = off_mem_rsvmap + mem_rsvmap_bytes;
        let off_dt_strings = off_dt_struct + self.structure.len() as u32;
        let totalsize = off_dt_strings + self.strings.len() as u32;

        let mut blob = Vec::with_capacity(totalsize as usize);
        push_be_u32(&mut blob, FDT_MAGIC);
        push_be_u32(&mut blob, totalsize);
        push_be_u32(&mut blob, off_dt_struct);
        push_be_u32(&mut blob, off_dt_strings);
        push_be_u32(&mut blob, off_mem_rsvmap);
        push_be_u32(&mut blob, 17);
        push_be_u32(&mut blob, 16);
        push_be_u32(&mut blob, 0);
        push_be_u32(&mut blob, self.strings.len() as u32);
        push_be_u32(&mut blob, self.structure.len() as u32);
        push_be_u64(&mut blob, 0);
        push_be_u64(&mut blob, 0);
        blob.extend_from_slice(&self.structure);
        blob.extend_from_slice(&self.strings);
        blob
    }
}

fn build_windows_arm_platform_fdt_blob(options: &WindowsArmPlatformDescriptionOptions) -> Vec<u8> {
    let mut builder = FdtBlobBuilder::default();

    builder.begin_node("");
    builder.prop_string("compatible", "bridgevm,windows-arm-hvf");
    builder.prop_string("model", "BridgeVM Windows 11 Arm HVF");
    builder.prop_u32("#address-cells", 2);
    builder.prop_u32("#size-cells", 2);
    builder.prop_u32("interrupt-parent", WINDOWS_ARM_GIC_PHANDLE);

    builder.begin_node("chosen");
    builder.end_node();

    builder.begin_node(&format!("memory@{:x}", WINDOWS_ARM_GUEST_RAM_IPA));
    builder.prop_string("device_type", "memory");
    builder.prop_reg64(WINDOWS_ARM_GUEST_RAM_IPA, options.guest_ram_bytes);
    builder.end_node();

    builder.begin_node("cpus");
    builder.prop_u32("#address-cells", 1);
    builder.prop_u32("#size-cells", 0);
    for cpu_index in 0..options.vcpu_count {
        builder.begin_node(&format!("cpu@{cpu_index:x}"));
        builder.prop_string("device_type", "cpu");
        builder.prop_string("compatible", "arm,arm-v8");
        builder.prop_u32("reg", u32::from(cpu_index));
        builder.end_node();
    }
    builder.end_node();

    builder.begin_node("timer");
    builder.prop_string("compatible", "arm,armv8-timer");
    builder.prop_u32_list(
        "interrupts",
        &[
            GIC_PPI,
            13,
            IRQ_TYPE_LEVEL_HIGH,
            GIC_PPI,
            14,
            IRQ_TYPE_LEVEL_HIGH,
            GIC_PPI,
            11,
            IRQ_TYPE_LEVEL_HIGH,
            GIC_PPI,
            10,
            IRQ_TYPE_LEVEL_HIGH,
        ],
    );
    builder.prop_empty("always-on");
    builder.end_node();

    builder.begin_node("intc@10010000");
    builder.prop_string("compatible", "arm,gic-v3");
    builder.prop_empty("interrupt-controller");
    builder.prop_u32("#interrupt-cells", 3);
    builder.prop_u32("#address-cells", 2);
    builder.prop_u32("#size-cells", 2);
    builder.prop_u32("phandle", WINDOWS_ARM_GIC_PHANDLE);
    builder.prop_reg64_pairs(&[
        (
            WINDOWS_ARM_GIC_DISTRIBUTOR_MMIO_IPA,
            WINDOWS_ARM_GIC_DISTRIBUTOR_BYTES,
        ),
        (
            WINDOWS_ARM_GIC_REDISTRIBUTOR_MMIO_IPA,
            windows_arm_gic_redistributor_fdt_bytes(options.vcpu_count),
        ),
    ]);
    builder.end_node();

    builder.begin_node("serial@10000000");
    builder.prop_string_list("compatible", &["arm,pl011", "arm,primecell"]);
    builder.prop_reg64(WINDOWS_ARM_PL011_MMIO_IPA, PL011_REGISTER_WINDOW_BYTES);
    builder.prop_gic_interrupt(GIC_SPI, WINDOWS_ARM_PL011_SPI, IRQ_TYPE_LEVEL_HIGH);
    builder.end_node();

    builder.begin_node("rtc@10001000");
    builder.prop_string_list("compatible", &["arm,pl031", "arm,primecell"]);
    builder.prop_reg64(WINDOWS_ARM_PL031_MMIO_IPA, PL031_REGISTER_WINDOW_BYTES);
    builder.prop_gic_interrupt(GIC_SPI, WINDOWS_ARM_PL031_SPI, IRQ_TYPE_LEVEL_HIGH);
    builder.end_node();

    builder.begin_node("virtio_mmio@10002000");
    builder.prop_string("compatible", "virtio,mmio");
    builder.prop_reg64(
        WINDOWS_ARM_VIRTIO_INSTALLER_ISO_MMIO_IPA,
        VIRTIO_MMIO_REGISTER_WINDOW_BYTES,
    );
    builder.prop_gic_interrupt(
        GIC_SPI,
        WINDOWS_ARM_VIRTIO_INSTALLER_ISO_SPI,
        IRQ_TYPE_LEVEL_HIGH,
    );
    builder.end_node();

    builder.begin_node("virtio_mmio@10003000");
    builder.prop_string("compatible", "virtio,mmio");
    builder.prop_reg64(
        WINDOWS_ARM_VIRTIO_TARGET_DISK_MMIO_IPA,
        VIRTIO_MMIO_REGISTER_WINDOW_BYTES,
    );
    builder.prop_gic_interrupt(
        GIC_SPI,
        WINDOWS_ARM_VIRTIO_TARGET_DISK_SPI,
        IRQ_TYPE_LEVEL_HIGH,
    );
    builder.end_node();

    builder.end_node();
    builder.finish()
}

fn build_windows_arm_firmware_run_loop_fdt_blob(guest_ram_bytes: u64) -> Vec<u8> {
    build_windows_arm_platform_fdt_blob(&WindowsArmPlatformDescriptionOptions {
        guest_ram_bytes,
        vcpu_count: WINDOWS_ARM_FIRMWARE_RUN_LOOP_FDT_VCPU_COUNT,
    })
}

fn windows_arm_firmware_run_loop_dtb_metadata(guest_ram_bytes: u64) -> (usize, u32, bool) {
    let blob = build_windows_arm_firmware_run_loop_fdt_blob(guest_ram_bytes);
    let magic = read_be_u32(&blob, 0).unwrap_or(0);
    (blob.len(), magic, magic == FDT_MAGIC)
}

fn inspect_windows_arm_platform_fdt_blob(blob: &[u8]) -> WindowsArmPlatformFdtSummary {
    let mut blockers = Vec::new();
    let fdt_magic = read_be_u32(blob, 0).unwrap_or(0);
    let totalsize = read_be_u32(blob, 4).unwrap_or(0) as usize;
    let off_dt_struct = read_be_u32(blob, 8).unwrap_or(0) as usize;
    let off_dt_strings = read_be_u32(blob, 12).unwrap_or(0) as usize;
    let size_dt_strings = read_be_u32(blob, 32).unwrap_or(0) as usize;
    let size_dt_struct = read_be_u32(blob, 36).unwrap_or(0) as usize;
    let mut summary = WindowsArmPlatformFdtSummary {
        fdt_magic,
        memory_node_base_ipa: None,
        cpu_count: 0,
        root_interrupt_parent: None,
        gic_phandle: None,
        gic_interrupt_controller: false,
        gic_distributor: None,
        gic_redistributor: None,
        arch_timer_interrupts: Vec::new(),
        pl011: None,
        pl011_interrupt: None,
        pl031: None,
        pl031_interrupt: None,
        virtio_installer_iso: None,
        virtio_installer_iso_interrupt: None,
        virtio_target_disk: None,
        virtio_target_disk_interrupt: None,
        blockers: Vec::new(),
    };

    if blob.len() < 40 {
        summary
            .blockers
            .push("FDT blob is shorter than the header".to_string());
        return summary;
    }
    if totalsize > blob.len() {
        blockers.push("FDT totalsize exceeds blob length".to_string());
    }
    let Some(struct_end) = off_dt_struct.checked_add(size_dt_struct) else {
        blockers.push("FDT structure block range overflowed".to_string());
        summary.blockers = blockers;
        return summary;
    };
    let Some(strings_end) = off_dt_strings.checked_add(size_dt_strings) else {
        blockers.push("FDT strings block range overflowed".to_string());
        summary.blockers = blockers;
        return summary;
    };
    if struct_end > blob.len() || strings_end > blob.len() {
        blockers.push("FDT block offsets exceed blob length".to_string());
        summary.blockers = blockers;
        return summary;
    }

    let structure = &blob[off_dt_struct..struct_end];
    let strings = &blob[off_dt_strings..strings_end];
    let mut offset = 0_usize;
    let mut path: Vec<String> = Vec::new();

    while offset + 4 <= structure.len() {
        let Some(token) = read_be_u32(structure, offset) else {
            blockers.push("FDT structure token read failed".to_string());
            break;
        };
        offset += 4;

        match token {
            FDT_BEGIN_NODE => {
                let Some((name, next_offset)) = read_fdt_node_name(structure, offset) else {
                    blockers.push("FDT node name read failed".to_string());
                    break;
                };
                offset = next_offset;
                if !name.is_empty() {
                    if path.len() == 1 && path[0] == "cpus" && name.starts_with("cpu@") {
                        summary.cpu_count = summary.cpu_count.saturating_add(1);
                    }
                    path.push(name);
                }
            }
            FDT_END_NODE => {
                let _ = path.pop();
            }
            FDT_PROP => {
                if offset + 8 > structure.len() {
                    blockers.push("FDT property header is truncated".to_string());
                    break;
                }
                let len = read_be_u32(structure, offset).unwrap_or(0) as usize;
                let name_offset = read_be_u32(structure, offset + 4).unwrap_or(0) as usize;
                offset += 8;
                let Some(data_end) = offset.checked_add(len) else {
                    blockers.push("FDT property data range overflowed".to_string());
                    break;
                };
                if data_end > structure.len() {
                    blockers.push("FDT property data is truncated".to_string());
                    break;
                }
                let data = &structure[offset..data_end];
                offset = align_up_to_4(data_end);
                let Some(name) = read_fdt_string(strings, name_offset) else {
                    blockers.push("FDT property name offset is invalid".to_string());
                    continue;
                };
                match name {
                    "reg" => record_windows_arm_fdt_reg(&path, data, &mut summary),
                    "interrupt-parent" if path.is_empty() => {
                        summary.root_interrupt_parent = read_fdt_u32(data);
                    }
                    "phandle" if path.last().is_some_and(|node| node == "intc@10010000") => {
                        summary.gic_phandle = read_fdt_u32(data);
                    }
                    "interrupt-controller"
                        if path.last().is_some_and(|node| node == "intc@10010000") =>
                    {
                        summary.gic_interrupt_controller = true;
                    }
                    "interrupts" => {
                        record_windows_arm_fdt_interrupts(&path, data, &mut summary);
                    }
                    _ => {}
                }
            }
            FDT_END => break,
            _ => {
                blockers.push(format!("unsupported FDT structure token {token:#x}"));
                break;
            }
        }
    }

    if summary.memory_node_base_ipa.is_none() {
        blockers.push("FDT memory node reg was not found".to_string());
    }
    if summary.pl011.is_none() {
        blockers.push("FDT PL011 node reg was not found".to_string());
    }
    if summary.pl031.is_none() {
        blockers.push("FDT PL031 node reg was not found".to_string());
    }
    if summary.virtio_installer_iso.is_none() {
        blockers.push("FDT VirtIO-MMIO installer ISO node reg was not found".to_string());
    }
    if summary.virtio_target_disk.is_none() {
        blockers.push("FDT VirtIO-MMIO target disk node reg was not found".to_string());
    }
    summary.blockers = blockers;
    summary
}

fn record_windows_arm_fdt_reg(
    path: &[String],
    data: &[u8],
    summary: &mut WindowsArmPlatformFdtSummary,
) {
    let Some(node) = path.last().map(String::as_str) else {
        return;
    };
    if path.len() == 2 && path[0] == "cpus" && node.starts_with("cpu@") {
        return;
    }
    if node == "intc@10010000" {
        let ranges = read_fdt_reg64_pairs(data);
        summary.gic_distributor = ranges.first().copied();
        summary.gic_redistributor = ranges.get(1).copied();
        return;
    }
    let Some(range) = read_fdt_reg64(data) else {
        return;
    };

    match node {
        name if name.starts_with("memory@") => {
            summary.memory_node_base_ipa = Some(range.base_ipa);
        }
        "serial@10000000" => summary.pl011 = Some(range),
        "rtc@10001000" => summary.pl031 = Some(range),
        "virtio_mmio@10002000" => summary.virtio_installer_iso = Some(range),
        "virtio_mmio@10003000" => summary.virtio_target_disk = Some(range),
        _ => {}
    }
}

fn record_windows_arm_fdt_interrupts(
    path: &[String],
    data: &[u8],
    summary: &mut WindowsArmPlatformFdtSummary,
) {
    let Some(node) = path.last().map(String::as_str) else {
        return;
    };
    let interrupts = read_fdt_interrupts(data);
    match node {
        "timer" => summary.arch_timer_interrupts = interrupts,
        "serial@10000000" => summary.pl011_interrupt = interrupts.first().copied(),
        "rtc@10001000" => summary.pl031_interrupt = interrupts.first().copied(),
        "virtio_mmio@10002000" => {
            summary.virtio_installer_iso_interrupt = interrupts.first().copied();
        }
        "virtio_mmio@10003000" => {
            summary.virtio_target_disk_interrupt = interrupts.first().copied();
        }
        _ => {}
    }
}

fn fdt_range_inside_device_window(range: FdtRegRange) -> bool {
    let Some(end) = range.base_ipa.checked_add(range.bytes) else {
        return false;
    };
    range.base_ipa >= WINDOWS_ARM_DEVICE_MMIO_IPA
        && end <= WINDOWS_ARM_DEVICE_MMIO_IPA.saturating_add(WINDOWS_ARM_DEVICE_MMIO_BYTES)
}

fn read_fdt_reg64(data: &[u8]) -> Option<FdtRegRange> {
    Some(FdtRegRange {
        base_ipa: read_be_u64(data, 0)?,
        bytes: read_be_u64(data, 8)?,
    })
}

fn read_fdt_reg64_pairs(data: &[u8]) -> Vec<FdtRegRange> {
    let mut ranges = Vec::new();
    for chunk in data.chunks_exact(16) {
        if let Some(range) = read_fdt_reg64(chunk) {
            ranges.push(range);
        }
    }
    ranges
}

fn read_fdt_u32(data: &[u8]) -> Option<u32> {
    read_be_u32(data, 0)
}

fn read_fdt_interrupts(data: &[u8]) -> Vec<FdtInterruptSpec> {
    let mut interrupts = Vec::new();
    for chunk in data.chunks_exact(12) {
        if let (Some(interrupt_type), Some(interrupt_number), Some(trigger)) = (
            read_be_u32(chunk, 0),
            read_be_u32(chunk, 4),
            read_be_u32(chunk, 8),
        ) {
            interrupts.push(FdtInterruptSpec {
                interrupt_type,
                interrupt_number,
                trigger,
            });
        }
    }
    interrupts
}

fn read_fdt_node_name(data: &[u8], offset: usize) -> Option<(String, usize)> {
    let mut end = offset;
    while end < data.len() && data[end] != 0 {
        end += 1;
    }
    if end >= data.len() {
        return None;
    }
    let name = std::str::from_utf8(&data[offset..end]).ok()?.to_string();
    Some((name, align_up_to_4(end + 1)))
}

fn read_fdt_string(data: &[u8], offset: usize) -> Option<&str> {
    let mut end = offset;
    while end < data.len() && data[end] != 0 {
        end += 1;
    }
    if end >= data.len() {
        return None;
    }
    std::str::from_utf8(&data[offset..end]).ok()
}

fn read_be_u32(data: &[u8], offset: usize) -> Option<u32> {
    let bytes = data.get(offset..offset.checked_add(4)?)?;
    Some(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn read_be_u64(data: &[u8], offset: usize) -> Option<u64> {
    let bytes = data.get(offset..offset.checked_add(8)?)?;
    Some(u64::from_be_bytes([
        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
    ]))
}

fn push_be_u32(output: &mut Vec<u8>, value: u32) {
    output.extend_from_slice(&value.to_be_bytes());
}

fn push_be_u64(output: &mut Vec<u8>, value: u64) {
    output.extend_from_slice(&value.to_be_bytes());
}

fn pad_to_4(output: &mut Vec<u8>) {
    while output.len() % 4 != 0 {
        output.push(0);
    }
}

fn align_up_to_4(value: usize) -> usize {
    (value + 3) & !3
}

fn gib_to_bytes(size_gib: u32) -> Option<u64> {
    u64::from(size_gib).checked_mul(1024 * 1024 * 1024)
}

fn align_lba(value: u64, alignment: u64) -> u64 {
    if alignment == 0 {
        return value;
    }
    value.div_ceil(alignment) * alignment
}

fn windows_arm_boot_disk_partitions(
    disk_size_bytes: u64,
) -> Result<Vec<WindowsArmBootDiskPartition>, String> {
    if disk_size_bytes < gib_to_bytes(WINDOWS_ARM_BOOT_DISK_MIN_SIZE_GIB).unwrap_or(0) {
        return Err(format!(
            "disk is smaller than {WINDOWS_ARM_BOOT_DISK_MIN_SIZE_GIB} GiB"
        ));
    }
    if disk_size_bytes % GPT_SECTOR_BYTES != 0 {
        return Err("disk size is not 512-byte sector aligned".to_string());
    }
    let total_lbas = disk_size_bytes / GPT_SECTOR_BYTES;
    if total_lbas <= GPT_FIRST_USABLE_LBA + GPT_ENTRY_ARRAY_SECTORS + 1 {
        return Err("disk does not have enough sectors for GPT headers".to_string());
    }
    let backup_header_lba = total_lbas - 1;
    let last_usable_lba = backup_header_lba - GPT_ENTRY_ARRAY_SECTORS - 1;

    let esp_start_lba = align_lba(GPT_FIRST_USABLE_LBA, WINDOWS_ARM_BOOT_DISK_ALIGNMENT_LBA);
    let esp_sectors = WINDOWS_ARM_ESP_SIZE_BYTES / GPT_SECTOR_BYTES;
    let esp_end_lba = esp_start_lba + esp_sectors - 1;
    let msr_start_lba = align_lba(esp_end_lba + 1, WINDOWS_ARM_BOOT_DISK_ALIGNMENT_LBA);
    let msr_sectors = WINDOWS_ARM_MSR_SIZE_BYTES / GPT_SECTOR_BYTES;
    let msr_end_lba = msr_start_lba + msr_sectors - 1;
    let windows_start_lba = align_lba(msr_end_lba + 1, WINDOWS_ARM_BOOT_DISK_ALIGNMENT_LBA);
    if windows_start_lba > last_usable_lba {
        return Err("disk does not have room for a Windows data partition".to_string());
    }

    Ok(vec![
        WindowsArmBootDiskPartition {
            name: "EFI System Partition",
            role: "UEFI boot files and Windows Boot Manager target",
            type_guid: "C12A7328-F81F-11D2-BA4B-00A0C93EC93B",
            start_lba: esp_start_lba,
            end_lba: esp_end_lba,
            size_bytes: WINDOWS_ARM_ESP_SIZE_BYTES,
        },
        WindowsArmBootDiskPartition {
            name: "Microsoft Reserved",
            role: "Windows GPT reserved partition",
            type_guid: "E3C9E316-0B5C-4DB8-817D-F92DF00215AE",
            start_lba: msr_start_lba,
            end_lba: msr_end_lba,
            size_bytes: WINDOWS_ARM_MSR_SIZE_BYTES,
        },
        WindowsArmBootDiskPartition {
            name: "Windows Basic Data",
            role: "Windows installation target partition",
            type_guid: "EBD0A0A2-B9E5-4433-87C0-68B6B72699C7",
            start_lba: windows_start_lba,
            end_lba: last_usable_lba,
            size_bytes: (last_usable_lba - windows_start_lba + 1) * GPT_SECTOR_BYTES,
        },
    ])
}

fn write_windows_arm_boot_disk_layout(path: &PathBuf, disk_size_bytes: u64) -> Result<(), String> {
    let partitions = windows_arm_boot_disk_partitions(disk_size_bytes)?;
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| error.to_string())?;
    file.set_len(disk_size_bytes)
        .map_err(|error| error.to_string())?;

    let total_lbas = disk_size_bytes / GPT_SECTOR_BYTES;
    let backup_header_lba = total_lbas - 1;
    let backup_entry_lba = backup_header_lba - GPT_ENTRY_ARRAY_SECTORS;
    let last_usable_lba = backup_entry_lba - 1;
    let disk_guid = stable_guid_bytes(path, "disk", disk_size_bytes);
    let entries = build_gpt_entry_array(path, disk_size_bytes, &partitions);
    let entries_crc32 = crc32(&entries);

    write_protective_mbr(&mut file, total_lbas)?;
    write_all_at(
        &mut file,
        GPT_PRIMARY_ENTRY_LBA * GPT_SECTOR_BYTES,
        &entries,
    )?;
    let primary_header = build_gpt_header(
        GPT_PRIMARY_HEADER_LBA,
        backup_header_lba,
        GPT_FIRST_USABLE_LBA,
        last_usable_lba,
        disk_guid,
        GPT_PRIMARY_ENTRY_LBA,
        entries_crc32,
    );
    write_all_at(
        &mut file,
        GPT_PRIMARY_HEADER_LBA * GPT_SECTOR_BYTES,
        &primary_header,
    )?;
    write_all_at(&mut file, backup_entry_lba * GPT_SECTOR_BYTES, &entries)?;
    let backup_header = build_gpt_header(
        backup_header_lba,
        GPT_PRIMARY_HEADER_LBA,
        GPT_FIRST_USABLE_LBA,
        last_usable_lba,
        disk_guid,
        backup_entry_lba,
        entries_crc32,
    );
    write_all_at(
        &mut file,
        backup_header_lba * GPT_SECTOR_BYTES,
        &backup_header,
    )?;
    file.sync_all().map_err(|error| error.to_string())?;
    Ok(())
}

fn verify_windows_arm_boot_disk_layout(
    path: &PathBuf,
) -> Result<WindowsArmBootDiskLayoutVerification, String> {
    let mut file = File::open(path).map_err(|error| error.to_string())?;
    let disk_size_bytes = file.metadata().map_err(|error| error.to_string())?.len();
    let partitions = windows_arm_boot_disk_partitions(disk_size_bytes)?;
    let total_lbas = disk_size_bytes / GPT_SECTOR_BYTES;
    let backup_header_lba = total_lbas - 1;
    let backup_entry_lba = backup_header_lba - GPT_ENTRY_ARRAY_SECTORS;
    let last_usable_lba = backup_entry_lba - 1;
    let entries = read_exact_at(
        &mut file,
        GPT_PRIMARY_ENTRY_LBA * GPT_SECTOR_BYTES,
        GPT_ENTRY_ARRAY_BYTES,
    )?;
    let entries_crc32 = crc32(&entries);

    verify_protective_mbr(&mut file, total_lbas)?;
    let primary_header = read_gpt_header(&mut file, GPT_PRIMARY_HEADER_LBA)?;
    verify_gpt_header(
        &primary_header,
        GPT_PRIMARY_HEADER_LBA,
        backup_header_lba,
        GPT_PRIMARY_ENTRY_LBA,
        GPT_FIRST_USABLE_LBA,
        last_usable_lba,
        entries_crc32,
    )?;
    verify_gpt_entries(&entries, &partitions)?;

    let backup_entries = read_exact_at(
        &mut file,
        backup_entry_lba * GPT_SECTOR_BYTES,
        GPT_ENTRY_ARRAY_BYTES,
    )?;
    if crc32(&backup_entries) != entries_crc32 {
        return Err("backup GPT partition-entry CRC does not match primary".to_string());
    }
    let backup_header = read_gpt_header(&mut file, backup_header_lba)?;
    verify_gpt_header(
        &backup_header,
        backup_header_lba,
        GPT_PRIMARY_HEADER_LBA,
        backup_entry_lba,
        GPT_FIRST_USABLE_LBA,
        last_usable_lba,
        entries_crc32,
    )?;

    Ok(WindowsArmBootDiskLayoutVerification {
        protective_mbr_verified: true,
        primary_gpt_verified: true,
        backup_gpt_verified: true,
        partition_entries_verified: true,
        disk_size_bytes,
    })
}

fn write_protective_mbr(file: &mut File, total_lbas: u64) -> Result<(), String> {
    let mut mbr = [0_u8; GPT_SECTOR_BYTES_USIZE];
    let partition_len = total_lbas.saturating_sub(1).min(u64::from(u32::MAX)) as u32;
    mbr[446 + 1] = 0xff;
    mbr[446 + 2] = 0xff;
    mbr[446 + 3] = 0xff;
    mbr[446 + 4] = 0xee;
    mbr[446 + 5] = 0xff;
    mbr[446 + 6] = 0xff;
    mbr[446 + 7] = 0xff;
    mbr[446 + 8..446 + 12].copy_from_slice(&1_u32.to_le_bytes());
    mbr[446 + 12..446 + 16].copy_from_slice(&partition_len.to_le_bytes());
    mbr[510] = 0x55;
    mbr[511] = 0xaa;
    write_all_at(file, 0, &mbr)
}

fn verify_protective_mbr(file: &mut File, total_lbas: u64) -> Result<(), String> {
    let mbr = read_exact_at(file, 0, GPT_SECTOR_BYTES_USIZE)?;
    if mbr[510] != 0x55 || mbr[511] != 0xaa {
        return Err("protective MBR signature is missing".to_string());
    }
    if mbr[446 + 4] != 0xee {
        return Err("protective MBR does not contain a GPT protective partition".to_string());
    }
    let start_lba = u32::from_le_bytes(
        mbr[446 + 8..446 + 12]
            .try_into()
            .map_err(|_| "protective MBR start LBA parse failed".to_string())?,
    );
    if start_lba != 1 {
        return Err("protective MBR start LBA is not 1".to_string());
    }
    let partition_len = u32::from_le_bytes(
        mbr[446 + 12..446 + 16]
            .try_into()
            .map_err(|_| "protective MBR length parse failed".to_string())?,
    );
    let expected_len = total_lbas.saturating_sub(1).min(u64::from(u32::MAX)) as u32;
    if partition_len != expected_len {
        return Err("protective MBR length does not cover the disk".to_string());
    }
    Ok(())
}

fn build_gpt_entry_array(
    path: &Path,
    disk_size_bytes: u64,
    partitions: &[WindowsArmBootDiskPartition],
) -> Vec<u8> {
    let mut entries = vec![0_u8; GPT_ENTRY_ARRAY_BYTES];
    for (index, partition) in partitions.iter().enumerate() {
        let type_guid = match partition.type_guid {
            "C12A7328-F81F-11D2-BA4B-00A0C93EC93B" => EFI_SYSTEM_PARTITION_GUID,
            "E3C9E316-0B5C-4DB8-817D-F92DF00215AE" => MICROSOFT_RESERVED_PARTITION_GUID,
            "EBD0A0A2-B9E5-4433-87C0-68B6B72699C7" => MICROSOFT_BASIC_DATA_PARTITION_GUID,
            _ => [0_u8; 16],
        };
        let unique_guid = stable_guid_bytes(path, partition.name, disk_size_bytes);
        let offset = index * GPT_ENTRY_SIZE;
        entries[offset..offset + 16].copy_from_slice(&type_guid);
        entries[offset + 16..offset + 32].copy_from_slice(&unique_guid);
        entries[offset + 32..offset + 40].copy_from_slice(&partition.start_lba.to_le_bytes());
        entries[offset + 40..offset + 48].copy_from_slice(&partition.end_lba.to_le_bytes());
        for (name_index, code_unit) in partition.name.encode_utf16().take(36).enumerate() {
            let name_offset = offset + 56 + name_index * 2;
            entries[name_offset..name_offset + 2].copy_from_slice(&code_unit.to_le_bytes());
        }
    }
    entries
}

fn build_gpt_header(
    current_lba: u64,
    backup_lba: u64,
    first_usable_lba: u64,
    last_usable_lba: u64,
    disk_guid: [u8; 16],
    entries_lba: u64,
    entries_crc32: u32,
) -> [u8; GPT_SECTOR_BYTES_USIZE] {
    let mut header = [0_u8; GPT_SECTOR_BYTES_USIZE];
    header[0..8].copy_from_slice(b"EFI PART");
    header[8..12].copy_from_slice(&0x0001_0000_u32.to_le_bytes());
    header[12..16].copy_from_slice(&92_u32.to_le_bytes());
    header[24..32].copy_from_slice(&current_lba.to_le_bytes());
    header[32..40].copy_from_slice(&backup_lba.to_le_bytes());
    header[40..48].copy_from_slice(&first_usable_lba.to_le_bytes());
    header[48..56].copy_from_slice(&last_usable_lba.to_le_bytes());
    header[56..72].copy_from_slice(&disk_guid);
    header[72..80].copy_from_slice(&entries_lba.to_le_bytes());
    header[80..84].copy_from_slice(&(GPT_ENTRY_COUNT as u32).to_le_bytes());
    header[84..88].copy_from_slice(&(GPT_ENTRY_SIZE as u32).to_le_bytes());
    header[88..92].copy_from_slice(&entries_crc32.to_le_bytes());
    let header_crc32 = crc32(&header[0..92]);
    header[16..20].copy_from_slice(&header_crc32.to_le_bytes());
    header
}

fn read_gpt_header(file: &mut File, lba: u64) -> Result<GptHeader, String> {
    let mut header = read_exact_at(file, lba * GPT_SECTOR_BYTES, GPT_SECTOR_BYTES_USIZE)?;
    if &header[0..8] != b"EFI PART" {
        return Err(format!(
            "GPT header at LBA {lba:#x} has an invalid signature"
        ));
    }
    let header_size = u32::from_le_bytes(
        header[12..16]
            .try_into()
            .map_err(|_| "GPT header size parse failed".to_string())?,
    ) as usize;
    if !(92..=GPT_SECTOR_BYTES_USIZE).contains(&header_size) {
        return Err("GPT header size is invalid".to_string());
    }
    let stored_crc = u32::from_le_bytes(
        header[16..20]
            .try_into()
            .map_err(|_| "GPT header CRC parse failed".to_string())?,
    );
    header[16..20].fill(0);
    let computed_crc = crc32(&header[0..header_size]);
    if stored_crc != computed_crc {
        return Err("GPT header CRC verification failed".to_string());
    }
    Ok(GptHeader {
        current_lba: u64_from_le(&header, 24)?,
        backup_lba: u64_from_le(&header, 32)?,
        first_usable_lba: u64_from_le(&header, 40)?,
        last_usable_lba: u64_from_le(&header, 48)?,
        entries_lba: u64_from_le(&header, 72)?,
        entry_count: u32_from_le(&header, 80)?,
        entry_size: u32_from_le(&header, 84)?,
        entries_crc32: u32_from_le(&header, 88)?,
    })
}

fn verify_gpt_header(
    header: &GptHeader,
    current_lba: u64,
    backup_lba: u64,
    entries_lba: u64,
    first_usable_lba: u64,
    last_usable_lba: u64,
    entries_crc32: u32,
) -> Result<(), String> {
    if header.current_lba != current_lba {
        return Err("GPT header current LBA mismatch".to_string());
    }
    if header.backup_lba != backup_lba {
        return Err("GPT header backup LBA mismatch".to_string());
    }
    if header.entries_lba != entries_lba {
        return Err("GPT header partition-entry LBA mismatch".to_string());
    }
    if header.first_usable_lba != first_usable_lba || header.last_usable_lba != last_usable_lba {
        return Err("GPT header usable LBA range mismatch".to_string());
    }
    if header.entry_count != GPT_ENTRY_COUNT as u32 || header.entry_size != GPT_ENTRY_SIZE as u32 {
        return Err("GPT header partition-entry geometry mismatch".to_string());
    }
    if header.entries_crc32 != entries_crc32 {
        return Err("GPT partition-entry CRC mismatch".to_string());
    }
    Ok(())
}

fn verify_gpt_entries(
    entries: &[u8],
    partitions: &[WindowsArmBootDiskPartition],
) -> Result<(), String> {
    for (index, partition) in partitions.iter().enumerate() {
        let offset = index * GPT_ENTRY_SIZE;
        let expected_type_guid = match partition.type_guid {
            "C12A7328-F81F-11D2-BA4B-00A0C93EC93B" => EFI_SYSTEM_PARTITION_GUID,
            "E3C9E316-0B5C-4DB8-817D-F92DF00215AE" => MICROSOFT_RESERVED_PARTITION_GUID,
            "EBD0A0A2-B9E5-4433-87C0-68B6B72699C7" => MICROSOFT_BASIC_DATA_PARTITION_GUID,
            _ => return Err("unknown partition type GUID".to_string()),
        };
        if entries[offset..offset + 16] != expected_type_guid {
            return Err(format!("partition {} type GUID mismatch", partition.name));
        }
        if u64_from_le(entries, offset + 32)? != partition.start_lba
            || u64_from_le(entries, offset + 40)? != partition.end_lba
        {
            return Err(format!("partition {} LBA range mismatch", partition.name));
        }
        if decode_gpt_partition_name(&entries[offset + 56..offset + GPT_ENTRY_SIZE])
            != partition.name
        {
            return Err(format!("partition {} name mismatch", partition.name));
        }
    }
    Ok(())
}

fn verify_uefi_firmware_file(
    path: &PathBuf,
    slot_bytes: u64,
) -> Result<UefiFirmwareFileVerification, String> {
    let mut file = File::open(path).map_err(|error| error.to_string())?;
    let bytes = file.metadata().map_err(|error| error.to_string())?.len();
    if bytes == 0 {
        return Err("file is empty".to_string());
    }
    if bytes > slot_bytes {
        return Err(format!(
            "file is larger than the planned pflash slot ({bytes:#x} > {slot_bytes:#x})"
        ));
    }
    let len: usize = bytes
        .try_into()
        .map_err(|_| "file is too large to inspect on this host".to_string())?;
    let mut contents = vec![0_u8; len];
    file.read_exact(&mut contents)
        .map_err(|error| error.to_string())?;
    let volume = detect_uefi_firmware_volume(&contents)?;
    Ok(UefiFirmwareFileVerification { bytes, volume })
}

fn load_uefi_pflash_slot(
    name: &'static str,
    path: &PathBuf,
    ipa_start: u64,
    slot_bytes: u64,
    writable: bool,
) -> Result<WindowsArmUefiPflashSlotMap, String> {
    let slot_len: usize = slot_bytes
        .try_into()
        .map_err(|_| "pflash slot is too large to allocate on this host".to_string())?;
    let source = media::read_bounded_file(path, slot_len).map_err(|error| error.to_string())?;
    if source.is_empty() {
        return Err("file is empty".to_string());
    }
    let source_bytes =
        u64::try_from(source.len()).map_err(|_| "file is too large to map".to_string())?;
    let mut slot = vec![0_u8; slot_len];
    slot[..source.len()].copy_from_slice(&source);
    let prefix_verified = slot[..source.len()] == source[..];
    let padding_zeroed = slot[source.len()..].iter().all(|byte| *byte == 0);

    Ok(WindowsArmUefiPflashSlotMap {
        name,
        path: path.clone(),
        ipa_start,
        slot_bytes,
        source_bytes,
        copied_bytes: source_bytes,
        zero_padding_bytes: slot_bytes - source_bytes,
        writable,
        prefix_verified,
        padding_zeroed,
    })
}

fn copy_uefi_vars_template(template_path: &PathBuf, vars_path: &PathBuf) -> Result<(), String> {
    std::fs::copy(template_path, vars_path).map_err(|error| error.to_string())?;
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(vars_path)
        .map_err(|error| error.to_string())?;
    file.sync_all().map_err(|error| error.to_string())
}

fn detect_uefi_firmware_volume(bytes: &[u8]) -> Result<UefiFirmwareVolumeMetadata, String> {
    if bytes.len() < UEFI_FV_MIN_HEADER_BYTES {
        return Err("file is too small for a UEFI firmware volume header".to_string());
    }
    let search_end = bytes.len().min(64 * 1024);
    for signature_offset in UEFI_FV_SIGNATURE_OFFSET..search_end.saturating_sub(4) {
        if &bytes[signature_offset..signature_offset + 4] != UEFI_FV_SIGNATURE {
            continue;
        }
        let offset = signature_offset - UEFI_FV_SIGNATURE_OFFSET;
        if offset + UEFI_FV_MIN_HEADER_BYTES > bytes.len() {
            continue;
        }
        let length_bytes = u64_from_le(bytes, offset + UEFI_FV_LENGTH_OFFSET)?;
        let header_length = u16_from_le(bytes, offset + UEFI_FV_HEADER_LENGTH_OFFSET)?;
        let header_length_usize = usize::from(header_length);
        if header_length_usize < UEFI_FV_MIN_HEADER_BYTES {
            return Err("UEFI firmware volume header length is too small".to_string());
        }
        if header_length_usize % 2 != 0 {
            return Err("UEFI firmware volume header length is not 16-bit aligned".to_string());
        }
        if offset + header_length_usize > bytes.len() {
            return Err("UEFI firmware volume header extends past the file".to_string());
        }
        let length_usize: usize = length_bytes
            .try_into()
            .map_err(|_| "UEFI firmware volume length is too large to inspect".to_string())?;
        if length_usize < header_length_usize {
            return Err("UEFI firmware volume length is smaller than its header".to_string());
        }
        if offset + length_usize > bytes.len() {
            return Err("UEFI firmware volume length extends past the file".to_string());
        }
        let header = &bytes[offset..offset + header_length_usize];
        if uefi_checksum16(header) != 0 {
            return Err("UEFI firmware volume header checksum verification failed".to_string());
        }
        return Ok(UefiFirmwareVolumeMetadata {
            offset: offset as u64,
            length_bytes,
            header_length,
            checksum_verified: true,
        });
    }
    Err("UEFI firmware volume signature _FVH was not found".to_string())
}

fn render_uefi_volume_metadata(
    label: &str,
    volume: &Option<UefiFirmwareVolumeMetadata>,
    output: &mut String,
) {
    match volume {
        Some(volume) => {
            output.push_str(&format!("{label} detected: true\n"));
            output.push_str(&format!("{label} offset: {:#x}\n", volume.offset));
            output.push_str(&format!(
                "{label} length bytes: {:#x}\n",
                volume.length_bytes
            ));
            output.push_str(&format!(
                "{label} header length: {:#x}\n",
                volume.header_length
            ));
            output.push_str(&format!(
                "{label} checksum verified: {}\n",
                volume.checksum_verified
            ));
        }
        None => output.push_str(&format!("{label} detected: false\n")),
    }
}

fn render_uefi_pflash_slot(
    label: &str,
    slot: &Option<WindowsArmUefiPflashSlotMap>,
    output: &mut String,
) {
    match slot {
        Some(slot) => {
            output.push_str(&format!("{label} loaded: true\n"));
            output.push_str(&format!("{label} name: {}\n", slot.name));
            output.push_str(&format!("{label} path: {}\n", slot.path.display()));
            output.push_str(&format!(
                "{label} IPA range: {:#x}..{:#x}\n",
                slot.ipa_start,
                slot.ipa_end_exclusive()
            ));
            output.push_str(&format!("{label} slot bytes: {:#x}\n", slot.slot_bytes));
            output.push_str(&format!("{label} source bytes: {:#x}\n", slot.source_bytes));
            output.push_str(&format!("{label} copied bytes: {:#x}\n", slot.copied_bytes));
            output.push_str(&format!(
                "{label} zero padding bytes: {:#x}\n",
                slot.zero_padding_bytes
            ));
            output.push_str(&format!("{label} writable: {}\n", slot.writable));
            output.push_str(&format!(
                "{label} prefix verified: {}\n",
                slot.prefix_verified
            ));
            output.push_str(&format!(
                "{label} padding zeroed: {}\n",
                slot.padding_zeroed
            ));
        }
        None => output.push_str(&format!("{label} loaded: false\n")),
    }
}

fn ipa_ranges_overlap(left_start: u64, left_size: u64, right_start: u64, right_size: u64) -> bool {
    let left_end = left_start.saturating_add(left_size);
    let right_end = right_start.saturating_add(right_size);
    left_start < right_end && right_start < left_end
}

fn decode_gpt_partition_name(bytes: &[u8]) -> String {
    let mut units = Vec::new();
    for chunk in bytes.chunks_exact(2) {
        let unit = u16::from_le_bytes([chunk[0], chunk[1]]);
        if unit == 0 {
            break;
        }
        units.push(unit);
    }
    String::from_utf16_lossy(&units)
}

fn stable_guid_bytes(path: &Path, label: &str, disk_size_bytes: u64) -> [u8; 16] {
    let mut first = fnv1a64(0xcbf2_9ce4_8422_2325, label.as_bytes());
    first = fnv1a64(first, path.to_string_lossy().as_bytes());
    first = fnv1a64(first, &disk_size_bytes.to_le_bytes());
    let mut second = fnv1a64(0x8422_2325_cbf2_9ce4, path.to_string_lossy().as_bytes());
    second = fnv1a64(second, label.as_bytes());
    second = fnv1a64(second, &disk_size_bytes.to_be_bytes());
    let mut output = [0_u8; 16];
    output[0..8].copy_from_slice(&first.to_le_bytes());
    output[8..16].copy_from_slice(&second.to_le_bytes());
    output[6] = (output[6] & 0x0f) | 0x40;
    output[8] = (output[8] & 0x3f) | 0x80;
    output
}

fn fnv1a64(mut hash: u64, bytes: &[u8]) -> u64 {
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn write_all_at(file: &mut File, offset: u64, bytes: &[u8]) -> Result<(), String> {
    file.seek(SeekFrom::Start(offset))
        .map_err(|error| error.to_string())?;
    file.write_all(bytes).map_err(|error| error.to_string())
}

fn read_exact_at(file: &mut File, offset: u64, len: usize) -> Result<Vec<u8>, String> {
    file.seek(SeekFrom::Start(offset))
        .map_err(|error| error.to_string())?;
    let mut bytes = vec![0_u8; len];
    file.read_exact(&mut bytes)
        .map_err(|error| error.to_string())?;
    Ok(bytes)
}

fn u32_from_le(bytes: &[u8], offset: usize) -> Result<u32, String> {
    Ok(u32::from_le_bytes(
        bytes
            .get(offset..offset + 4)
            .ok_or_else(|| "u32 field out of range".to_string())?
            .try_into()
            .map_err(|_| "u32 field parse failed".to_string())?,
    ))
}

fn u16_from_le(bytes: &[u8], offset: usize) -> Result<u16, String> {
    Ok(u16::from_le_bytes(
        bytes
            .get(offset..offset + 2)
            .ok_or_else(|| "u16 field out of range".to_string())?
            .try_into()
            .map_err(|_| "u16 field parse failed".to_string())?,
    ))
}

fn u64_from_le(bytes: &[u8], offset: usize) -> Result<u64, String> {
    Ok(u64::from_le_bytes(
        bytes
            .get(offset..offset + 8)
            .ok_or_else(|| "u64 field out of range".to_string())?
            .try_into()
            .map_err(|_| "u64 field parse failed".to_string())?,
    ))
}

fn uefi_checksum16(bytes: &[u8]) -> u16 {
    let mut sum = 0_u16;
    for chunk in bytes.chunks_exact(2) {
        sum = sum.wrapping_add(u16::from_le_bytes([chunk[0], chunk[1]]));
    }
    sum
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffff_u32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = 0_u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}

fn append_low_vector_post_repair_exit_telemetry(
    output: &mut String,
    label: &str,
    telemetry: &LowVectorPostRepairExitTelemetry,
    kind_label: &str,
    context_exit: Option<&WindowsArmUefiFirmwareRunLoopExit>,
) {
    output.push_str(&format!("{label} observed: {}\n", telemetry.observed));
    output.push_str(&format!(
        "{label}: {}\n",
        render_optional_intid(telemetry.index)
    ));
    output.push_str(&format!(
        "{label} reason name: {}\n",
        render_optional_exit_reason_name(telemetry.reason)
    ));
    output.push_str(&format!(
        "{label} classification: {}\n",
        telemetry.diagnosis
    ));
    output.push_str(&format!(
        "{label} PC: {}\n",
        render_optional_u64(telemetry.pc)
    ));
    output.push_str(&format!(
        "{label} instruction: {}\n",
        render_optional_instruction_word(
            context_exit.and_then(|exit| exit.instruction_word_after_exit)
        )
    ));
    output.push_str(&format!(
        "{label} instruction hint: {}\n",
        context_exit
            .map(|exit| exit.instruction_hint_after_exit)
            .unwrap_or("not observed")
    ));
    output.push_str(&format!(
        "{label} VBAR_EL1: {}\n",
        render_optional_u64(context_exit.and_then(|exit| exit.vbar_el1_after_exit))
    ));
    output.push_str(&format!(
        "{label} ELR_EL1: {}\n",
        render_optional_u64(context_exit.and_then(|exit| exit.elr_el1_after_exit))
    ));
    output.push_str(&format!(
        "{label} ESR_EL1: {}\n",
        render_optional_u64(context_exit.and_then(|exit| exit.esr_el1_after_exit))
    ));
    output.push_str(&format!(
        "{label} FAR_EL1: {}\n",
        render_optional_u64(context_exit.and_then(|exit| exit.far_el1_after_exit))
    ));
    output.push_str(&format!(
        "{label} SPSR_EL1: {}\n",
        render_optional_u64(context_exit.and_then(|exit| exit.spsr_el1_after_exit))
    ));
    output.push_str(&format!("{label} access kind: {}\n", telemetry.access.kind));
    output.push_str(&format!(
        "{label} access direction: {}\n",
        telemetry.access.direction
    ));
    output.push_str(&format!(
        "{label} access address: {}\n",
        render_optional_u64(telemetry.access.address)
    ));
    output.push_str(&format!(
        "{label} access sysreg: {}\n",
        render_optional_u16_hex(telemetry.access.sysreg)
    ));
    output.push_str(&format!(
        "{label} access syndrome: {}\n",
        render_optional_u64(telemetry.access.syndrome)
    ));
    output.push_str(&format!("{kind_label}: {}\n", telemetry.interaction_kind));
}

fn append_low_vector_post_repair_unhandled_access_telemetry(
    output: &mut String,
    label: &str,
    telemetry: &LowVectorPostRepairUnhandledAccessTelemetry,
) {
    output.push_str(&format!("{label} observed: {}\n", telemetry.observed));
    output.push_str(&format!(
        "{label}: {}\n",
        render_optional_intid(telemetry.index)
    ));
    output.push_str(&format!(
        "{label} reason name: {}\n",
        render_optional_exit_reason_name(telemetry.reason)
    ));
    output.push_str(&format!(
        "{label} classification: {}\n",
        telemetry.diagnosis
    ));
    output.push_str(&format!(
        "{label} PC: {}\n",
        render_optional_u64(telemetry.pc)
    ));
    output.push_str(&format!(
        "{label} syndrome: {}\n",
        render_optional_u64(telemetry.syndrome)
    ));
    output.push_str(&format!("{label} kind: {}\n", telemetry.kind));
    output.push_str(&format!("{label} direction: {}\n", telemetry.access));
    output.push_str(&format!(
        "{label} register: {}\n",
        render_optional_u8(telemetry.register)
    ));
    output.push_str(&format!(
        "{label} value: {}\n",
        render_optional_u64(telemetry.value)
    ));
    output.push_str(&format!(
        "{label} handler result: {}\n",
        telemetry.handler_result
    ));
    output.push_str(&format!(
        "{label} MMIO IPA: {}\n",
        render_optional_u64(telemetry.mmio_ipa)
    ));
    output.push_str(&format!(
        "{label} MMIO width: {}\n",
        render_optional_u8(telemetry.mmio_width)
    ));
    output.push_str(&format!(
        "{label} MMIO device kind: {}\n",
        telemetry.mmio_device_kind
    ));
    output.push_str(&format!(
        "{label} sysreg: {}\n",
        render_optional_u16_hex(telemetry.sysreg)
    ));
    output.push_str(&format!("{label} sysreg name: {}\n", telemetry.sysreg_name));
    output.push_str(&format!(
        "{label} sysreg op0: {}\n",
        render_optional_u8(telemetry.sysreg_op0)
    ));
    output.push_str(&format!(
        "{label} sysreg op1: {}\n",
        render_optional_u8(telemetry.sysreg_op1)
    ));
    output.push_str(&format!(
        "{label} sysreg crn: {}\n",
        render_optional_u8(telemetry.sysreg_crn)
    ));
    output.push_str(&format!(
        "{label} sysreg crm: {}\n",
        render_optional_u8(telemetry.sysreg_crm)
    ));
    output.push_str(&format!(
        "{label} sysreg op2: {}\n",
        render_optional_u8(telemetry.sysreg_op2)
    ));
}

fn low_vector_post_repair_context_exit(
    exits: &[WindowsArmUefiFirmwareRunLoopExit],
    index: Option<u32>,
) -> Option<&WindowsArmUefiFirmwareRunLoopExit> {
    let index = index?;
    exits.iter().find(|exit| exit.index == index)
}

pub(crate) fn render_optional_u32(value: Option<u32>) -> String {
    value.map_or_else(|| "unknown".to_string(), |value| value.to_string())
}

fn render_optional_u16_hex(value: Option<u16>) -> String {
    value.map_or_else(|| "not observed".to_string(), |value| format!("{value:#x}"))
}

fn render_optional_intid(value: Option<u32>) -> String {
    value.map_or_else(|| "not observed".to_string(), |value| value.to_string())
}

fn render_optional_gic_intid(value: Option<u32>) -> String {
    value.map_or_else(
        || "not observed".to_string(),
        |value| match value {
            GICV3_SPURIOUS_INTERRUPT_ID => "spurious".to_string(),
            value => value.to_string(),
        },
    )
}

fn render_optional_u64(value: Option<u64>) -> String {
    value.map_or_else(|| "not observed".to_string(), |value| format!("{value:#x}"))
}

fn render_optional_u8(value: Option<u8>) -> String {
    value.map_or_else(|| "not observed".to_string(), |value| format!("{value:#x}"))
}

fn render_optional_bool(value: Option<bool>) -> &'static str {
    match value {
        Some(true) => "true",
        Some(false) => "false",
        None => "not observed",
    }
}

fn render_optional_instruction_word(value: Option<u32>) -> String {
    value.map_or_else(
        || "not observed".to_string(),
        |value| format!("{value:#010x}"),
    )
}

fn render_hex_bytes(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return "not observed".to_string();
    }
    let mut output = String::from("0x");
    for byte in bytes {
        output.push_str(&format!("{byte:02x}"));
    }
    output
}

fn render_optional_status(value: Option<i32>) -> String {
    value.map_or_else(
        || "not attempted".to_string(),
        |status| format!("{status:#x}"),
    )
}

fn render_optional_status_name(value: Option<i32>) -> &'static str {
    value.map_or("not attempted", hv_return_name)
}

fn render_optional_exit_reason(value: Option<u32>) -> String {
    value.map_or_else(
        || "not observed".to_string(),
        |reason| format!("{reason:#x}"),
    )
}

fn render_optional_exit_reason_name(value: Option<u32>) -> &'static str {
    value.map_or("not observed", hv_exit_reason_name)
}

fn render_optional_exception_class_name(value: Option<u64>) -> &'static str {
    value.map_or("not observed", arm_exception_class_name)
}

fn render_optional_esr_exception_class_name(value: Option<u64>) -> &'static str {
    value.map_or("not observed", |esr| {
        arm_exception_class_name(arm_exception_class(esr))
    })
}

fn render_optional_sctlr_mmu_enabled(value: Option<u64>) -> &'static str {
    match value {
        Some(sctlr) if sctlr & 1 == 1 => "true",
        Some(_) => "false",
        None => "not observed",
    }
}

fn windows_arm_initial_sp_el1_ipa(guest_ram_bytes: u64) -> u64 {
    WINDOWS_ARM_GUEST_RAM_IPA
        .saturating_add(guest_ram_bytes)
        .saturating_sub(16)
        & !0xf
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WindowsArmFirmwareRunLoopDiagnosis {
    DiagnosticVectorHvcExit,
    DiagnosticVectorContinuationHvcExit,
    GuestRamDiagnosticVectorHvcExit,
    GuestRamDiagnosticVectorContinuationHvcExit,
    ExecutableDiagnosticVectorHvcExit,
    ExecutableDiagnosticVectorContinuationHvcExit,
    ExecutableDiagnosticVectorEretLandingHvcExit,
    LowVectorDiagnosticPageHvcExit,
    LowVectorDiagnosticPageEretLandingHvcExit,
    DiagnosticVectorStage1XnPermissionFault,
    GuestRamDiagnosticVectorStage1XnPermissionFault,
    ExecutableDiagnosticVectorStage1XnPermissionFault,
    DiagnosticVectorMmuInstructionAbort,
    GuestRamDiagnosticVectorMmuInstructionAbort,
    ExecutableDiagnosticVectorMmuInstructionAbort,
    RecommendedVectorBaseEmptySyncSlot,
    El1LowVectorMmuTranslationFault,
    ErasedPflashExecution,
    NotClassified,
}

impl WindowsArmFirmwareRunLoopDiagnosis {
    fn as_str(self) -> &'static str {
        match self {
            Self::DiagnosticVectorHvcExit => "diagnostic-vector-hvc-exit",
            Self::DiagnosticVectorContinuationHvcExit => "diagnostic-vector-continuation-hvc-exit",
            Self::GuestRamDiagnosticVectorHvcExit => "guest-ram-diagnostic-vector-hvc-exit",
            Self::GuestRamDiagnosticVectorContinuationHvcExit => {
                "guest-ram-diagnostic-vector-continuation-hvc-exit"
            }
            Self::ExecutableDiagnosticVectorHvcExit => "executable-diagnostic-vector-hvc-exit",
            Self::ExecutableDiagnosticVectorContinuationHvcExit => {
                "executable-diagnostic-vector-continuation-hvc-exit"
            }
            Self::ExecutableDiagnosticVectorEretLandingHvcExit => {
                "executable-diagnostic-vector-eret-landing-hvc-exit"
            }
            Self::LowVectorDiagnosticPageHvcExit => "low-vector-diagnostic-page-hvc-exit",
            Self::LowVectorDiagnosticPageEretLandingHvcExit => {
                "low-vector-diagnostic-page-eret-landing-hvc-exit"
            }
            Self::DiagnosticVectorStage1XnPermissionFault => {
                "diagnostic-vector-stage1-xn-permission-fault"
            }
            Self::GuestRamDiagnosticVectorStage1XnPermissionFault => {
                "guest-ram-diagnostic-vector-stage1-xn-permission-fault"
            }
            Self::ExecutableDiagnosticVectorStage1XnPermissionFault => {
                "executable-diagnostic-vector-stage1-xn-permission-fault"
            }
            Self::DiagnosticVectorMmuInstructionAbort => "diagnostic-vector-mmu-instruction-abort",
            Self::GuestRamDiagnosticVectorMmuInstructionAbort => {
                "guest-ram-diagnostic-vector-mmu-instruction-abort"
            }
            Self::ExecutableDiagnosticVectorMmuInstructionAbort => {
                "executable-diagnostic-vector-mmu-instruction-abort"
            }
            Self::RecommendedVectorBaseEmptySyncSlot => "recommended-vector-base-empty-sync-slot",
            Self::El1LowVectorMmuTranslationFault => "el1-low-vector-mmu-translation-fault",
            Self::ErasedPflashExecution => "erased-pflash-execution",
            Self::NotClassified => "not classified",
        }
    }
}

fn windows_arm_firmware_run_loop_exit_diagnosis(
    exit: &WindowsArmUefiFirmwareRunLoopExit,
) -> &'static str {
    windows_arm_firmware_run_loop_exit_diagnosis_kind(exit).as_str()
}

fn recommended_vector_base_vbar_initial_reason(
    requested: bool,
    diagnostic_vector_seed_requested: bool,
    repair_low_vector_diagnostic_page: bool,
) -> &'static str {
    if !requested {
        "not requested"
    } else if diagnostic_vector_seed_requested {
        "ignored-diagnostic-vector-seed"
    } else if repair_low_vector_diagnostic_page {
        "ignored-low-vector-repair"
    } else {
        "not selected"
    }
}

fn windows_arm_firmware_run_loop_exit_diagnosis_kind(
    exit: &WindowsArmUefiFirmwareRunLoopExit,
) -> WindowsArmFirmwareRunLoopDiagnosis {
    let mmu_enabled = exit
        .sctlr_el1_after_exit
        .map(|sctlr| sctlr & 1 == 1)
        .unwrap_or(false);
    let esr_is_instruction_abort_same_el = exit
        .esr_el1_after_exit
        .map(|esr| arm_exception_class(esr) == 0x21)
        .unwrap_or(false);
    let esr_is_translation_fault_level_3 = exit
        .esr_el1_after_exit
        .map(|esr| arm_abort_fault_status(esr) == 0x07)
        .unwrap_or(false);
    let pflash_diagnostic_vector_sync_pc = WINDOWS_ARM_DIAGNOSTIC_VECTOR_IPA
        + WINDOWS_ARM_DIAGNOSTIC_VECTOR_CURRENT_EL_SPX_SYNC_OFFSET as u64;
    let guest_ram_diagnostic_vector_sync_pc = WINDOWS_ARM_GUEST_RAM_DIAGNOSTIC_VECTOR_IPA
        + WINDOWS_ARM_DIAGNOSTIC_VECTOR_CURRENT_EL_SPX_SYNC_OFFSET as u64;
    let executable_diagnostic_vector_sync_pc = WINDOWS_ARM_EXECUTABLE_DIAGNOSTIC_VECTOR_IPA
        + WINDOWS_ARM_DIAGNOSTIC_VECTOR_CURRENT_EL_SPX_SYNC_OFFSET as u64;
    let pflash_diagnostic_vector_hvc_exit_pc = pflash_diagnostic_vector_sync_pc + 4;
    let guest_ram_diagnostic_vector_hvc_exit_pc = guest_ram_diagnostic_vector_sync_pc + 4;
    let executable_diagnostic_vector_hvc_exit_pc = executable_diagnostic_vector_sync_pc + 4;
    let pflash_diagnostic_vector_continuation_hvc_exit_pc = pflash_diagnostic_vector_sync_pc + 8;
    let guest_ram_diagnostic_vector_continuation_hvc_exit_pc =
        guest_ram_diagnostic_vector_sync_pc + 8;
    let executable_diagnostic_vector_continuation_hvc_exit_pc =
        executable_diagnostic_vector_sync_pc + 8;
    let executable_diagnostic_vector_eret_landing_hvc_exit_pc =
        executable_diagnostic_vector_sync_pc + 12;
    let low_vector_diagnostic_page_hvc_exit_pc =
        WINDOWS_ARM_DIAGNOSTIC_VECTOR_CURRENT_EL_SPX_SYNC_OFFSET as u64 + 4;
    let low_vector_diagnostic_page_eret_landing_hvc_exit_pc =
        WINDOWS_ARM_DIAGNOSTIC_VECTOR_CURRENT_EL_SPX_SYNC_OFFSET as u64 + 12;
    let low_vector_diagnostic_page_is_mapped = exit.pc_stage1_leaf_descriptor_after_exit
        == Some(WINDOWS_ARM_LOW_VECTOR_DIAGNOSTIC_PAGE_DESCRIPTOR);
    if exit.vbar_el1_after_exit == Some(WINDOWS_ARM_DIAGNOSTIC_VECTOR_IPA)
        && (exit.pc_after_exit == Some(pflash_diagnostic_vector_sync_pc)
            || exit.pc_after_exit == Some(pflash_diagnostic_vector_hvc_exit_pc))
        && exit.exit_exception_class == Some(0x16)
    {
        WindowsArmFirmwareRunLoopDiagnosis::DiagnosticVectorHvcExit
    } else if exit.vbar_el1_after_exit == Some(WINDOWS_ARM_DIAGNOSTIC_VECTOR_IPA)
        && exit.pc_after_exit == Some(pflash_diagnostic_vector_continuation_hvc_exit_pc)
        && exit.exit_exception_class == Some(0x16)
    {
        WindowsArmFirmwareRunLoopDiagnosis::DiagnosticVectorContinuationHvcExit
    } else if exit.vbar_el1_after_exit == Some(WINDOWS_ARM_GUEST_RAM_DIAGNOSTIC_VECTOR_IPA)
        && (exit.pc_after_exit == Some(guest_ram_diagnostic_vector_sync_pc)
            || exit.pc_after_exit == Some(guest_ram_diagnostic_vector_hvc_exit_pc))
        && exit.exit_exception_class == Some(0x16)
    {
        WindowsArmFirmwareRunLoopDiagnosis::GuestRamDiagnosticVectorHvcExit
    } else if exit.vbar_el1_after_exit == Some(WINDOWS_ARM_GUEST_RAM_DIAGNOSTIC_VECTOR_IPA)
        && exit.pc_after_exit == Some(guest_ram_diagnostic_vector_continuation_hvc_exit_pc)
        && exit.exit_exception_class == Some(0x16)
    {
        WindowsArmFirmwareRunLoopDiagnosis::GuestRamDiagnosticVectorContinuationHvcExit
    } else if exit.vbar_el1_after_exit == Some(WINDOWS_ARM_EXECUTABLE_DIAGNOSTIC_VECTOR_IPA)
        && (exit.pc_after_exit == Some(executable_diagnostic_vector_sync_pc)
            || exit.pc_after_exit == Some(executable_diagnostic_vector_hvc_exit_pc))
        && exit.exit_exception_class == Some(0x16)
    {
        WindowsArmFirmwareRunLoopDiagnosis::ExecutableDiagnosticVectorHvcExit
    } else if exit.vbar_el1_after_exit == Some(WINDOWS_ARM_EXECUTABLE_DIAGNOSTIC_VECTOR_IPA)
        && exit.pc_after_exit == Some(executable_diagnostic_vector_continuation_hvc_exit_pc)
        && exit.exit_exception_class == Some(0x16)
    {
        WindowsArmFirmwareRunLoopDiagnosis::ExecutableDiagnosticVectorContinuationHvcExit
    } else if exit.vbar_el1_after_exit == Some(WINDOWS_ARM_EXECUTABLE_DIAGNOSTIC_VECTOR_IPA)
        && exit.pc_after_exit == Some(executable_diagnostic_vector_eret_landing_hvc_exit_pc)
        && exit.exit_exception_class == Some(0x16)
    {
        WindowsArmFirmwareRunLoopDiagnosis::ExecutableDiagnosticVectorEretLandingHvcExit
    } else if (exit.vbar_el1_after_exit == Some(0) || low_vector_diagnostic_page_is_mapped)
        && exit.pc_after_exit == Some(low_vector_diagnostic_page_hvc_exit_pc)
        && exit.exit_exception_class == Some(0x16)
        && exit.instruction_word_after_exit == Some(AARCH64_ERET_INSTRUCTION)
    {
        WindowsArmFirmwareRunLoopDiagnosis::LowVectorDiagnosticPageHvcExit
    } else if (exit.vbar_el1_after_exit == Some(0) || low_vector_diagnostic_page_is_mapped)
        && exit.pc_after_exit == Some(low_vector_diagnostic_page_eret_landing_hvc_exit_pc)
        && exit.exit_exception_class == Some(0x16)
    {
        WindowsArmFirmwareRunLoopDiagnosis::LowVectorDiagnosticPageEretLandingHvcExit
    } else if exit.vbar_el1_after_exit == Some(WINDOWS_ARM_DIAGNOSTIC_VECTOR_IPA)
        && exit.pc_after_exit == Some(pflash_diagnostic_vector_sync_pc)
        && mmu_enabled
        && esr_is_instruction_abort_same_el
        && (exit.pc_stage1_leaf_pxn_after_exit == Some(true)
            || exit.pc_stage1_leaf_uxn_after_exit == Some(true))
    {
        WindowsArmFirmwareRunLoopDiagnosis::DiagnosticVectorStage1XnPermissionFault
    } else if exit.vbar_el1_after_exit == Some(WINDOWS_ARM_GUEST_RAM_DIAGNOSTIC_VECTOR_IPA)
        && exit.pc_after_exit == Some(guest_ram_diagnostic_vector_sync_pc)
        && mmu_enabled
        && esr_is_instruction_abort_same_el
        && (exit.pc_stage1_leaf_pxn_after_exit == Some(true)
            || exit.pc_stage1_leaf_uxn_after_exit == Some(true))
    {
        WindowsArmFirmwareRunLoopDiagnosis::GuestRamDiagnosticVectorStage1XnPermissionFault
    } else if exit.vbar_el1_after_exit == Some(WINDOWS_ARM_EXECUTABLE_DIAGNOSTIC_VECTOR_IPA)
        && exit.pc_after_exit == Some(executable_diagnostic_vector_sync_pc)
        && mmu_enabled
        && esr_is_instruction_abort_same_el
        && (exit.pc_stage1_leaf_pxn_after_exit == Some(true)
            || exit.pc_stage1_leaf_uxn_after_exit == Some(true))
    {
        WindowsArmFirmwareRunLoopDiagnosis::ExecutableDiagnosticVectorStage1XnPermissionFault
    } else if exit.vbar_el1_after_exit == Some(WINDOWS_ARM_DIAGNOSTIC_VECTOR_IPA)
        && exit.pc_after_exit == Some(pflash_diagnostic_vector_sync_pc)
        && mmu_enabled
        && esr_is_instruction_abort_same_el
    {
        WindowsArmFirmwareRunLoopDiagnosis::DiagnosticVectorMmuInstructionAbort
    } else if exit.vbar_el1_after_exit == Some(WINDOWS_ARM_GUEST_RAM_DIAGNOSTIC_VECTOR_IPA)
        && exit.pc_after_exit == Some(guest_ram_diagnostic_vector_sync_pc)
        && mmu_enabled
        && esr_is_instruction_abort_same_el
    {
        WindowsArmFirmwareRunLoopDiagnosis::GuestRamDiagnosticVectorMmuInstructionAbort
    } else if exit.vbar_el1_after_exit == Some(WINDOWS_ARM_EXECUTABLE_DIAGNOSTIC_VECTOR_IPA)
        && exit.pc_after_exit == Some(executable_diagnostic_vector_sync_pc)
        && mmu_enabled
        && esr_is_instruction_abort_same_el
    {
        WindowsArmFirmwareRunLoopDiagnosis::ExecutableDiagnosticVectorMmuInstructionAbort
    } else if exit.vbar_el1_after_exit == Some(WINDOWS_ARM_EXECUTABLE_DIAGNOSTIC_VECTOR_IPA)
        && exit.pc_after_exit == Some(executable_diagnostic_vector_sync_pc)
        && exit.instruction_word_after_exit == Some(0)
        && exit.pc_stage1_leaf_pxn_after_exit == Some(false)
        && exit.pc_stage1_leaf_uxn_after_exit == Some(false)
    {
        WindowsArmFirmwareRunLoopDiagnosis::RecommendedVectorBaseEmptySyncSlot
    } else if exit.vbar_el1_after_exit == Some(0)
        && exit.pc_after_exit == Some(0x200)
        && exit.elr_el1_after_exit == Some(0x200)
        && exit.far_el1_after_exit == Some(0x200)
        && mmu_enabled
        && esr_is_instruction_abort_same_el
        && esr_is_translation_fault_level_3
    {
        WindowsArmFirmwareRunLoopDiagnosis::El1LowVectorMmuTranslationFault
    } else if exit.instruction_word_after_exit == Some(0xffff_ffff) {
        WindowsArmFirmwareRunLoopDiagnosis::ErasedPflashExecution
    } else {
        WindowsArmFirmwareRunLoopDiagnosis::NotClassified
    }
}

fn render_optional_abort_iss(value: Option<u64>) -> String {
    value.map_or_else(
        || "not observed".to_string(),
        |syndrome| format!("{:#x}", arm_abort_iss(syndrome)),
    )
}

fn render_optional_abort_fault_status(value: Option<u64>) -> String {
    value.map_or_else(
        || "not observed".to_string(),
        |syndrome| format!("{:#x}", arm_abort_fault_status(syndrome)),
    )
}

fn render_optional_abort_fault_status_name(value: Option<u64>) -> &'static str {
    value.map_or("not observed", |syndrome| {
        arm_abort_fault_status_name(arm_abort_fault_status(syndrome))
    })
}

fn arm_exception_class(syndrome: u64) -> u64 {
    syndrome >> 26
}

fn arm_abort_iss(syndrome: u64) -> u64 {
    syndrome & 0x01ff_ffff
}

fn arm_abort_fault_status(syndrome: u64) -> u64 {
    syndrome & 0x3f
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DecodedMmioDataAbort {
    is_write: bool,
    register: u8,
    width: u8,
}

impl DecodedMmioDataAbort {
    fn access_name(self) -> &'static str {
        if self.is_write {
            "write"
        } else {
            "read"
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DecodedSystemRegisterAccess {
    is_read: bool,
    register: u8,
    sys_reg: u16,
    op0: u8,
    op1: u8,
    crn: u8,
    crm: u8,
    op2: u8,
}

impl DecodedSystemRegisterAccess {
    fn access_name(self) -> &'static str {
        if self.is_read {
            "read"
        } else {
            "write"
        }
    }
}

fn aarch64_sys_reg_encoding(op0: u8, op1: u8, crn: u8, crm: u8, op2: u8) -> u16 {
    (u16::from(op0) << 14)
        | (u16::from(op1) << 11)
        | (u16::from(crn) << 7)
        | (u16::from(crm) << 3)
        | u16::from(op2)
}

fn decode_system_register_trap(syndrome: u64) -> Option<DecodedSystemRegisterAccess> {
    if arm_exception_class(syndrome) != AARCH64_SYSREG_TRAP_EXCEPTION_CLASS {
        return None;
    }
    let iss = arm_abort_iss(syndrome);
    let op0 = ((iss >> 20) & 0x3) as u8;
    let op2 = ((iss >> 17) & 0x7) as u8;
    let op1 = ((iss >> 14) & 0x7) as u8;
    let crn = ((iss >> 10) & 0xf) as u8;
    let register = ((iss >> 5) & 0x1f) as u8;
    let crm = ((iss >> 1) & 0xf) as u8;
    let is_read = (iss & 1) != 0;
    Some(DecodedSystemRegisterAccess {
        is_read,
        register,
        sys_reg: aarch64_sys_reg_encoding(op0, op1, crn, crm, op2),
        op0,
        op1,
        crn,
        crm,
        op2,
    })
}

fn decode_mmio_data_abort(syndrome: u64) -> Option<DecodedMmioDataAbort> {
    if !matches!(arm_exception_class(syndrome), 0x24 | 0x25) {
        return None;
    }
    let iss = arm_abort_iss(syndrome);
    if ((iss >> 24) & 1) == 0 {
        return None;
    }
    if ((iss >> 21) & 1) != 0 {
        return None;
    }
    let register = ((iss >> 16) & 0x1f) as u8;
    if register == 31 {
        return None;
    }
    let width = match (iss >> 22) & 0x3 {
        0 => 1,
        1 => 2,
        2 => 4,
        3 => 8,
        _ => unreachable!("masked two-bit access size"),
    };
    Some(DecodedMmioDataAbort {
        is_write: ((iss >> 6) & 1) != 0,
        register,
        width,
    })
}

fn arm_abort_fault_status_name(status: u64) -> &'static str {
    match status {
        0x00 => "address size fault level 0",
        0x01 => "address size fault level 1",
        0x02 => "address size fault level 2",
        0x03 => "address size fault level 3",
        0x04 => "translation fault level 0",
        0x05 => "translation fault level 1",
        0x06 => "translation fault level 2",
        0x07 => "translation fault level 3",
        0x09 => "access flag fault level 1",
        0x0a => "access flag fault level 2",
        0x0b => "access flag fault level 3",
        0x0d => "permission fault level 1",
        0x0e => "permission fault level 2",
        0x0f => "permission fault level 3",
        0x10 => "synchronous external abort",
        0x14 => "synchronous external abort on translation table walk level 0",
        0x15 => "synchronous external abort on translation table walk level 1",
        0x16 => "synchronous external abort on translation table walk level 2",
        0x17 => "synchronous external abort on translation table walk level 3",
        0x18 => "synchronous parity or ECC error",
        0x1c => "synchronous parity or ECC error on translation table walk level 0",
        0x1d => "synchronous parity or ECC error on translation table walk level 1",
        0x1e => "synchronous parity or ECC error on translation table walk level 2",
        0x1f => "synchronous parity or ECC error on translation table walk level 3",
        0x21 => "alignment fault",
        0x22 => "debug event",
        0x30 => "TLB conflict abort",
        0x3d => "unsupported atomic hardware update fault",
        _ => "unknown",
    }
}

fn windows_arm_guest_region_name(address: Option<u64>, guest_ram_bytes: u64) -> &'static str {
    let Some(address) = address else {
        return "not observed";
    };
    if address >= WINDOWS_ARM_UEFI_CODE_IPA
        && address < WINDOWS_ARM_UEFI_CODE_IPA.saturating_add(WINDOWS_ARM_UEFI_SLOT_BYTES)
    {
        "firmware pflash slot"
    } else if address
        < WINDOWS_ARM_UEFI_LOW_CODE_ALIAS_IPA.saturating_add(WINDOWS_ARM_UEFI_SLOT_BYTES)
    {
        "low firmware pflash alias"
    } else if address >= WINDOWS_ARM_UEFI_LOW_VARS_ALIAS_IPA
        && address < WINDOWS_ARM_UEFI_LOW_VARS_ALIAS_IPA.saturating_add(WINDOWS_ARM_UEFI_SLOT_BYTES)
    {
        "low vars pflash alias"
    } else if address >= WINDOWS_ARM_UEFI_VARS_IPA
        && address < WINDOWS_ARM_UEFI_VARS_IPA.saturating_add(WINDOWS_ARM_UEFI_SLOT_BYTES)
    {
        "vars pflash slot"
    } else if address >= WINDOWS_ARM_DEVICE_MMIO_IPA
        && address < WINDOWS_ARM_DEVICE_MMIO_IPA.saturating_add(WINDOWS_ARM_DEVICE_MMIO_BYTES)
    {
        "Windows device MMIO window"
    } else if address >= WINDOWS_ARM_GUEST_RAM_IPA
        && address < WINDOWS_ARM_GUEST_RAM_IPA.saturating_add(guest_ram_bytes)
    {
        "guest RAM"
    } else {
        "unmapped or unknown"
    }
}

fn aarch64_instruction_hint(instruction: u32) -> &'static str {
    match instruction {
        0xffff_ffff => "erased-pflash",
        0xd400_0002 => "hvc-0",
        0xd400_0022 => "hvc-1",
        0xd69f_03e0 => "eret",
        0xd503_201f => "nop",
        0xd503_203f => "yield",
        0xd503_205f => "wfe",
        0xd503_207f => "wfi",
        0xd503_209f => "sev",
        0xd503_20bf => "sevl",
        _ => "unknown",
    }
}

fn arm_exception_class_name(class: u64) -> &'static str {
    match class {
        0x00 => "unknown reason",
        0x01 => "trapped WFI/WFE",
        0x07 => "trapped SVE/SIMD/FP",
        0x11 => "SVC AArch32",
        0x15 => "SVC AArch64",
        0x16 => "HVC AArch64",
        0x17 => "SMC AArch64",
        0x20 => "instruction abort lower EL",
        0x21 => "instruction abort same EL",
        0x22 => "PC alignment fault",
        0x24 => "data abort lower EL",
        0x25 => "data abort same EL",
        0x26 => "SP alignment fault",
        0x2c => "trapped floating point",
        0x2f => "SError interrupt",
        0x30 => "breakpoint lower EL",
        0x31 => "breakpoint same EL",
        0x32 => "software step lower EL",
        0x33 => "software step same EL",
        0x34 => "watchpoint lower EL",
        0x35 => "watchpoint same EL",
        0x3c => "BRK AArch64",
        _ => "unknown",
    }
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[path = "platform/apple.rs"]
mod platform;

#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
#[path = "platform/unsupported.rs"]
mod platform;

#[cfg(test)]
mod tests {
    use super::*;

    fn test_firmware_run_loop_exit() -> WindowsArmUefiFirmwareRunLoopExit {
        WindowsArmUefiFirmwareRunLoopExit {
            index: 1,
            run_status: None,
            exit_reason: None,
            exit_syndrome: None,
            exit_exception_class: None,
            exit_virtual_address: None,
            exit_physical_address: None,
            pc_after_exit_status: None,
            pc_after_exit: None,
            instruction_word_after_exit: None,
            instruction_hint_after_exit: "not observed",
            pc_stage1_leaf_level_after_exit: None,
            pc_stage1_leaf_descriptor_after_exit: None,
            pc_stage1_leaf_descriptor_kind_after_exit: "not observed",
            pc_stage1_leaf_pxn_after_exit: None,
            pc_stage1_leaf_uxn_after_exit: None,
            stage1_descriptor_samples_after_exit: Vec::new(),
            stage1_walk_entries_after_exit: Vec::new(),
            stage1_executable_candidates_after_exit: Vec::new(),
            x0_after_exit: None,
            x1_after_exit: None,
            x2_after_exit: None,
            x3_after_exit: None,
            x4_after_exit: None,
            cpsr_after_exit: None,
            vbar_el1_after_exit: None,
            elr_el1_after_exit: None,
            esr_el1_after_exit: None,
            far_el1_after_exit: None,
            spsr_el1_after_exit: None,
            sctlr_el1_after_exit: None,
            tcr_el1_after_exit: None,
            ttbr0_el1_after_exit: None,
            ttbr1_el1_after_exit: None,
            mair_el1_after_exit: None,
            sp_el1_after_exit: None,
            watchdog_cancel_status: None,
            vtimer_auto_mask_get_status: None,
            vtimer_auto_mask_after_exit: None,
            vtimer_rearm_cval_value: None,
            vtimer_rearm_cval_set_status: None,
            vtimer_ppi_pending_recorded: None,
            vtimer_irq_line_assertable: None,
            vtimer_gic_group1_enabled: None,
            vtimer_gic_priority_mask: None,
            vtimer_gic_running_priority: None,
            vtimer_gic_priority_threshold: None,
            vtimer_gic_pending_intid: None,
            vtimer_pending_irq_set_status: None,
            vtimer_unmask_status: None,
            handled: false,
        }
    }

    #[test]
    fn low_vector_remap_target_requires_populated_non_diagnostic_current_el_spx_slot() {
        let recommendation = |word, base_physical_address| WindowsArmUefiVectorBaseRecommendation {
            base_virtual_address: WINDOWS_ARM_EXECUTABLE_DIAGNOSTIC_VECTOR_IPA,
            base_physical_address,
            current_el_spx_sync_instruction_word: word,
            current_el_spx_sync_instruction_hint: "unit-test",
            reason: "unit-test",
        };

        assert!(recommendation(
            Some(0xd503_207f),
            Some(WINDOWS_ARM_EXECUTABLE_DIAGNOSTIC_VECTOR_IPA)
        )
        .is_populated_low_vector_remap_target());
        assert!(
            !recommendation(Some(0), Some(WINDOWS_ARM_EXECUTABLE_DIAGNOSTIC_VECTOR_IPA))
                .is_populated_low_vector_remap_target()
        );
        assert!(!recommendation(
            Some(0xffff_ffff),
            Some(WINDOWS_ARM_EXECUTABLE_DIAGNOSTIC_VECTOR_IPA)
        )
        .is_populated_low_vector_remap_target());
        assert!(!recommendation(
            Some(AARCH64_HVC_0_INSTRUCTION),
            Some(WINDOWS_ARM_EXECUTABLE_DIAGNOSTIC_VECTOR_IPA)
        )
        .is_populated_low_vector_remap_target());
        assert!(!recommendation(
            Some(AARCH64_HVC_1_INSTRUCTION),
            Some(WINDOWS_ARM_EXECUTABLE_DIAGNOSTIC_VECTOR_IPA)
        )
        .is_populated_low_vector_remap_target());
        assert!(!recommendation(
            Some(AARCH64_ERET_INSTRUCTION),
            Some(WINDOWS_ARM_EXECUTABLE_DIAGNOSTIC_VECTOR_IPA)
        )
        .is_populated_low_vector_remap_target());
        assert!(!recommendation(Some(0xd503_207f), None).is_populated_low_vector_remap_target());
    }

    #[test]
    fn firmware_post_repair_interaction_classifier_labels_timer_and_virtio_mmio() {
        let vtimer_exit = WindowsArmUefiFirmwareRunLoopExit {
            run_status: Some(0),
            exit_reason: Some(2),
            ..test_firmware_run_loop_exit()
        };
        assert_eq!(
            windows_arm_firmware_post_repair_interaction_kind(&[], &vtimer_exit),
            "vtimer"
        );
        assert!(!windows_arm_firmware_post_repair_is_device_interaction(
            "vtimer"
        ));

        let installer_iso = PathBuf::from("/tmp/Win11_Arm64.iso");
        let block_devices = windows_arm_firmware_block_devices(Some(installer_iso), None);
        let virtio_exit = WindowsArmUefiFirmwareRunLoopExit {
            run_status: Some(0),
            exit_reason: Some(1),
            exit_syndrome: Some(0x93c0_8006),
            exit_physical_address: Some(WINDOWS_ARM_VIRTIO_INSTALLER_ISO_MMIO_IPA),
            ..test_firmware_run_loop_exit()
        };
        assert_eq!(
            windows_arm_firmware_post_repair_interaction_kind(&block_devices, &virtio_exit),
            "mmio:virtio-installer-iso"
        );
        assert!(windows_arm_firmware_post_repair_is_device_interaction(
            "mmio:virtio-installer-iso"
        ));
        assert!(windows_arm_firmware_post_repair_is_device_interaction(
            "sysreg:trap"
        ));
        assert!(!windows_arm_firmware_post_repair_is_device_interaction(
            "exception:non-mmio"
        ));
    }

    #[test]
    fn post_repair_device_interaction_skips_diagnostic_vector_continuation() {
        let mut telemetry = LowVectorPostRepairTelemetry::default();
        let diagnostic_exit = WindowsArmUefiFirmwareRunLoopExit {
            index: 4,
            run_status: Some(0),
            exit_reason: Some(1),
            exit_syndrome: Some(0x5a00_0001),
            pc_after_exit: Some(0x200204),
            ..test_firmware_run_loop_exit()
        };
        telemetry.observe_first_exit(&[], &diagnostic_exit);
        telemetry.observe_device_interaction(&[], &diagnostic_exit);

        assert!(telemetry.first_exit.observed);
        assert_eq!(telemetry.first_exit.index, Some(4));
        assert_eq!(telemetry.first_exit.interaction_kind, "exception:non-mmio");
        assert!(!telemetry.first_device_interaction.observed);

        let installer_iso = PathBuf::from("/tmp/Win11_Arm64.iso");
        let block_devices = windows_arm_firmware_block_devices(Some(installer_iso), None);
        let virtio_exit = WindowsArmUefiFirmwareRunLoopExit {
            index: 7,
            run_status: Some(0),
            exit_reason: Some(1),
            exit_syndrome: Some(0x93c0_8006),
            exit_physical_address: Some(WINDOWS_ARM_VIRTIO_INSTALLER_ISO_MMIO_IPA),
            pc_after_exit: Some(0x8001234),
            ..test_firmware_run_loop_exit()
        };
        telemetry.observe_device_interaction(&block_devices, &virtio_exit);

        assert!(telemetry.first_device_interaction.observed);
        assert_eq!(telemetry.first_device_interaction.index, Some(7));
        assert_eq!(
            telemetry.first_device_interaction.interaction_kind,
            "mmio:virtio-installer-iso"
        );
        assert_eq!(telemetry.first_device_interaction.pc, Some(0x8001234));
    }

    #[test]
    fn post_repair_exit_telemetry_records_access_metadata() {
        let installer_iso = PathBuf::from("/tmp/Win11_Arm64.iso");
        let block_devices = windows_arm_firmware_block_devices(Some(installer_iso), None);
        let virtio_exit = WindowsArmUefiFirmwareRunLoopExit {
            index: 9,
            run_status: Some(HV_SUCCESS_VALUE),
            exit_reason: Some(HV_EXIT_REASON_EXCEPTION_VALUE),
            exit_syndrome: Some(0x93c0_8006),
            exit_physical_address: Some(WINDOWS_ARM_VIRTIO_INSTALLER_ISO_MMIO_IPA + 0x10),
            pc_after_exit: Some(0x800_4321),
            ..test_firmware_run_loop_exit()
        };

        let telemetry = LowVectorPostRepairExitTelemetry::observed(&block_devices, &virtio_exit);
        assert_eq!(telemetry.interaction_kind, "mmio:virtio-installer-iso");
        assert_eq!(telemetry.access.kind, "mmio");
        assert_eq!(telemetry.access.direction, "read");
        assert_eq!(
            telemetry.access.address,
            Some(WINDOWS_ARM_VIRTIO_INSTALLER_ISO_MMIO_IPA + 0x10)
        );
        assert_eq!(telemetry.access.sysreg, None);
        assert_eq!(telemetry.access.syndrome, Some(0x93c0_8006));

        let mut output = String::new();
        append_low_vector_post_repair_exit_telemetry(
            &mut output,
            "Post-repair first device interaction",
            &telemetry,
            "Post-repair first device interaction kind",
            Some(&virtio_exit),
        );
        assert!(output.contains("Post-repair first device interaction access kind: mmio"));
        assert!(output.contains("Post-repair first device interaction access direction: read"));
        assert!(output.contains("Post-repair first device interaction access address: 0x10002010"));
        assert!(output.contains("Post-repair first device interaction access sysreg: not observed"));
        assert!(output.contains("Post-repair first device interaction access syndrome: 0x93c08006"));

        let sysreg_exit = WindowsArmUefiFirmwareRunLoopExit {
            index: 10,
            run_status: Some(HV_SUCCESS_VALUE),
            exit_reason: Some(HV_EXIT_REASON_EXCEPTION_VALUE),
            exit_syndrome: Some(sysreg_trap_syndrome(true, 2, 3, 0, 12, 12, 0)),
            pc_after_exit: Some(0x800_4567),
            ..test_firmware_run_loop_exit()
        };
        let telemetry = LowVectorPostRepairExitTelemetry::observed(&[], &sysreg_exit);
        assert_eq!(telemetry.interaction_kind, "sysreg:trap");
        assert_eq!(telemetry.access.kind, "icc-sysreg");
        assert_eq!(telemetry.access.direction, "read");
        assert_eq!(telemetry.access.address, None);
        assert_eq!(telemetry.access.sysreg, Some(ICC_IAR1_EL1_SYSREG));
        assert!(telemetry.access.syndrome.is_some());
    }

    #[test]
    fn post_repair_unhandled_access_telemetry_records_decode_metadata() {
        let installer_iso = PathBuf::from("/tmp/Win11_Arm64.iso");
        let block_devices = windows_arm_firmware_block_devices(Some(installer_iso), None);
        let virtio_exit = WindowsArmUefiFirmwareRunLoopExit {
            index: 11,
            run_status: Some(HV_SUCCESS_VALUE),
            exit_reason: Some(HV_EXIT_REASON_EXCEPTION_VALUE),
            exit_syndrome: Some(0x93c0_8006),
            exit_physical_address: Some(WINDOWS_ARM_VIRTIO_INSTALLER_ISO_MMIO_IPA + 0x10),
            pc_after_exit: Some(0x800_6789),
            ..test_firmware_run_loop_exit()
        };
        let mmio_access = decode_mmio_data_abort(virtio_exit.exit_syndrome.unwrap()).unwrap();

        let mut telemetry = LowVectorPostRepairTelemetry::default();
        telemetry.observe_unhandled_mmio_access(
            &block_devices,
            &virtio_exit,
            mmio_access,
            WINDOWS_ARM_VIRTIO_INSTALLER_ISO_MMIO_IPA + 0x10,
            None,
            "device-bus-unhandled-read",
        );

        assert!(telemetry.first_unhandled_access.observed);
        assert_eq!(telemetry.first_unhandled_access.index, Some(11));
        assert_eq!(telemetry.first_unhandled_access.kind, "mmio");
        assert_eq!(telemetry.first_unhandled_access.access, "read");
        assert_eq!(
            telemetry.first_unhandled_access.mmio_ipa,
            Some(WINDOWS_ARM_VIRTIO_INSTALLER_ISO_MMIO_IPA + 0x10)
        );
        assert_eq!(telemetry.first_unhandled_access.mmio_width, Some(8));
        assert_eq!(
            telemetry.first_unhandled_access.mmio_device_kind,
            "virtio-installer-iso"
        );
        assert_eq!(
            telemetry.first_unhandled_access.handler_result,
            "device-bus-unhandled-read"
        );

        let mut output = String::new();
        append_low_vector_post_repair_unhandled_access_telemetry(
            &mut output,
            "Post-repair first unhandled access",
            &telemetry.first_unhandled_access,
        );
        assert!(output.contains("Post-repair first unhandled access observed: true"));
        assert!(output.contains("Post-repair first unhandled access: 11"));
        assert!(output.contains("Post-repair first unhandled access kind: mmio"));
        assert!(output.contains("Post-repair first unhandled access direction: read"));
        assert!(output.contains("Post-repair first unhandled access MMIO IPA: 0x10002010"));
        assert!(output
            .contains("Post-repair first unhandled access MMIO device kind: virtio-installer-iso"));

        let sysreg_exit = WindowsArmUefiFirmwareRunLoopExit {
            index: 12,
            run_status: Some(HV_SUCCESS_VALUE),
            exit_reason: Some(HV_EXIT_REASON_EXCEPTION_VALUE),
            exit_syndrome: Some(sysreg_trap_syndrome(true, 2, 3, 0, 12, 12, 0)),
            pc_after_exit: Some(0x800_9876),
            ..test_firmware_run_loop_exit()
        };
        let sysreg_access =
            decode_system_register_trap(sysreg_exit.exit_syndrome.unwrap()).unwrap();
        let mut telemetry = LowVectorPostRepairTelemetry::default();
        telemetry.observe_unhandled_sysreg_access(
            &sysreg_exit,
            sysreg_access,
            None,
            "sysreg-unhandled",
        );

        assert!(telemetry.first_unhandled_access.observed);
        assert_eq!(telemetry.first_unhandled_access.kind, "icc-sysreg");
        assert_eq!(telemetry.first_unhandled_access.access, "read");
        assert_eq!(
            telemetry.first_unhandled_access.sysreg,
            Some(ICC_IAR1_EL1_SYSREG)
        );
        assert_eq!(telemetry.first_unhandled_access.sysreg_name, "ICC_IAR1_EL1");
        assert_eq!(
            telemetry.first_unhandled_access.handler_result,
            "sysreg-unhandled"
        );
    }

    #[test]
    fn windows_11_arm_platform_description_probe_is_fdt_first_and_metadata_safe() {
        let probe =
            probe_windows_11_arm_platform_description(WindowsArmPlatformDescriptionOptions {
                guest_ram_bytes: 8 * 1024 * 1024 * 1024,
                vcpu_count: 6,
            });
        let output = probe.render_text();

        assert!(!probe.qemu_used);
        assert!(!probe.apple_vz_used);
        assert!(!probe.hvf_entered);
        assert_eq!(probe.format, "FDT");
        assert_eq!(probe.fdt_magic, FDT_MAGIC);
        assert_eq!(read_be_u32(&probe.fdt_blob, 0), Some(FDT_MAGIC));
        assert!(probe.fdt_magic_verified);
        assert_eq!(probe.memory_node_base_ipa, Some(WINDOWS_ARM_GUEST_RAM_IPA));
        assert!(probe.memory_node_at_guest_ram_base);
        assert_eq!(probe.requested_cpu_count, 6);
        assert_eq!(probe.cpu_count, 6);
        assert!(probe.cpu_count_verified);
        assert_eq!(probe.device_mmio_start_ipa, WINDOWS_ARM_DEVICE_MMIO_IPA);
        assert_eq!(
            probe.device_mmio_end_ipa,
            WINDOWS_ARM_DEVICE_MMIO_IPA + WINDOWS_ARM_DEVICE_MMIO_BYTES
        );
        assert_eq!(probe.mmio_nodes.len(), 4);
        assert!(probe
            .mmio_nodes
            .iter()
            .all(|node| node.inside_device_window));
        assert!(probe.mmio_nodes_inside_device_window);
        assert!(probe.mmio_nodes.iter().any(|node| node.label == "PL011"
            && node.base_ipa == Some(WINDOWS_ARM_PL011_MMIO_IPA)
            && node.bytes == Some(PL011_REGISTER_WINDOW_BYTES)));
        assert!(probe.mmio_nodes.iter().any(|node| node.label == "PL031"
            && node.base_ipa == Some(WINDOWS_ARM_PL031_MMIO_IPA)
            && node.bytes == Some(PL031_REGISTER_WINDOW_BYTES)));
        assert!(probe
            .mmio_nodes
            .iter()
            .any(|node| node.label == "VirtIO-MMIO installer ISO"
                && node.node_name == "virtio_mmio@10002000"
                && node.base_ipa == Some(WINDOWS_ARM_VIRTIO_INSTALLER_ISO_MMIO_IPA)
                && node.bytes == Some(VIRTIO_MMIO_REGISTER_WINDOW_BYTES)));
        assert!(probe
            .mmio_nodes
            .iter()
            .any(|node| node.label == "VirtIO-MMIO target disk"
                && node.node_name == "virtio_mmio@10003000"
                && node.base_ipa == Some(WINDOWS_ARM_VIRTIO_TARGET_DISK_MMIO_IPA)
                && node.bytes == Some(VIRTIO_MMIO_REGISTER_WINDOW_BYTES)));
        assert_eq!(probe.root_interrupt_parent, Some(WINDOWS_ARM_GIC_PHANDLE));
        assert_eq!(probe.gic_phandle, Some(WINDOWS_ARM_GIC_PHANDLE));
        assert_eq!(
            probe.gic_distributor_base_ipa,
            Some(WINDOWS_ARM_GIC_DISTRIBUTOR_MMIO_IPA)
        );
        assert_eq!(
            probe.gic_distributor_bytes,
            Some(WINDOWS_ARM_GIC_DISTRIBUTOR_BYTES)
        );
        assert_eq!(
            probe.gic_redistributor_base_ipa,
            Some(WINDOWS_ARM_GIC_REDISTRIBUTOR_MMIO_IPA)
        );
        assert_eq!(
            probe.gic_redistributor_bytes,
            Some(windows_arm_gic_redistributor_fdt_bytes(6))
        );
        assert!(probe.gic_nodes_inside_device_window);
        assert!(probe.arch_timer_node_present);
        assert_eq!(probe.arch_timer_interrupt_count, 4);
        assert_eq!(probe.interrupt_nodes.len(), 4);
        assert!(probe.interrupt_nodes_described);
        assert!(probe
            .interrupt_nodes
            .iter()
            .any(|node| node.label == "PL011"
                && node.interrupt_type == Some(GIC_SPI)
                && node.interrupt_number == Some(WINDOWS_ARM_PL011_SPI)
                && node.trigger == Some(IRQ_TYPE_LEVEL_HIGH)
                && node.described));
        assert!(probe
            .interrupt_nodes
            .iter()
            .any(|node| node.label == "PL031"
                && node.interrupt_type == Some(GIC_SPI)
                && node.interrupt_number == Some(WINDOWS_ARM_PL031_SPI)
                && node.trigger == Some(IRQ_TYPE_LEVEL_HIGH)
                && node.described));
        assert!(probe
            .interrupt_nodes
            .iter()
            .any(|node| node.label == "VirtIO-MMIO installer ISO"
                && node.interrupt_type == Some(GIC_SPI)
                && node.interrupt_number == Some(WINDOWS_ARM_VIRTIO_INSTALLER_ISO_SPI)
                && node.trigger == Some(IRQ_TYPE_LEVEL_HIGH)
                && node.described));
        assert!(probe
            .interrupt_nodes
            .iter()
            .any(|node| node.label == "VirtIO-MMIO target disk"
                && node.interrupt_type == Some(GIC_SPI)
                && node.interrupt_number == Some(WINDOWS_ARM_VIRTIO_TARGET_DISK_SPI)
                && node.trigger == Some(IRQ_TYPE_LEVEL_HIGH)
                && node.described));
        assert!(!probe.acpi_implemented);
        assert!(!probe.fw_cfg_used);
        assert_eq!(probe.gic_status, "described/not emulated");
        assert!(!probe.gic_emulated);
        assert!(probe.blockers.is_empty());

        assert!(output.contains("Windows 11 Arm HVF platform description probe"));
        assert!(output.contains("QEMU: not used"));
        assert!(output.contains("Apple VZ: not used"));
        assert!(output.contains("HVF: not entered"));
        assert!(output.contains("Format: FDT"));
        assert!(output.contains("FDT magic: 0xd00dfeed"));
        assert!(output.contains("Memory node base: 0x40000000"));
        assert!(output.contains("Memory node at 0x40000000: true"));
        assert!(output.contains("CPU count: 6"));
        assert!(output.contains("Device MMIO window: 0x10000000..0x20000000"));
        assert!(output.contains(
            "PL011/PL031/VirtIO-MMIO installer ISO/target disk nodes inside device window: true"
        ));
        assert!(output.contains("PL011 node inside device window: true"));
        assert!(output.contains("PL031 node inside device window: true"));
        assert!(output.contains("VirtIO-MMIO installer ISO node inside device window: true"));
        assert!(output.contains("VirtIO-MMIO target disk node inside device window: true"));
        assert!(output.contains("Root interrupt-parent: 0x1"));
        assert!(output.contains("GIC phandle: 0x1"));
        assert!(output.contains("GIC distributor base: 0x10010000"));
        assert!(output.contains("GIC distributor bytes: 0x10000"));
        assert!(output.contains("GIC redistributor base: 0x10020000"));
        assert!(output.contains("GIC redistributor bytes: 0xc0000"));
        assert!(output.contains("GIC nodes inside device window: true"));
        assert!(output.contains("ARM arch timer node present: true"));
        assert!(output.contains("ARM arch timer interrupt count: 4"));
        assert!(output.contains("Interrupt nodes described: true"));
        assert!(output.contains("PL011 interrupt type: 0x0"));
        assert!(output.contains("PL011 interrupt number: 0x0"));
        assert!(output.contains("PL011 interrupt trigger: 0x4"));
        assert!(output.contains("PL031 interrupt number: 0x1"));
        assert!(output.contains("VirtIO-MMIO installer ISO interrupt number: 0x2"));
        assert!(output.contains("VirtIO-MMIO target disk interrupt number: 0x3"));
        assert!(output.contains("ACPI: not implemented"));
        assert!(output.contains("fw_cfg: not used"));
        assert!(output.contains("GIC: described/not emulated"));
        assert!(output.contains("Blockers: none"));
        assert!(!output.contains("qemu-system"));
        assert!(!output.contains('%'));
    }

    #[test]
    fn windows_11_arm_platform_description_probe_reports_zero_cpu_blocker() {
        let probe =
            probe_windows_11_arm_platform_description(WindowsArmPlatformDescriptionOptions {
                guest_ram_bytes: WINDOWS_ARM_PLATFORM_DESCRIPTION_DEFAULT_GUEST_RAM_BYTES,
                vcpu_count: 0,
            });
        let output = probe.render_text();

        assert_eq!(probe.requested_cpu_count, 0);
        assert_eq!(probe.cpu_count, 0);
        assert!(probe.cpu_count_verified);
        assert!(probe
            .blockers
            .iter()
            .any(|blocker| blocker.contains("FDT CPU count must be non-zero for Windows Arm")));
        assert!(output.contains("CPU count: 0"));
        assert!(output.contains("FDT CPU count must be non-zero for Windows Arm"));
        assert!(output.contains("QEMU: not used"));
        assert!(output.contains("Apple VZ: not used"));
        assert!(output.contains("HVF: not entered"));
        assert!(!output.contains("qemu-system"));
        assert!(!output.contains('%'));
    }

    #[test]
    fn windows_11_arm_boot_disk_layout_probe_creates_and_verifies_sparse_gpt() {
        let path = std::env::temp_dir().join(format!(
            "bridgevm-windows-arm-boot-disk-layout-{}-{}.raw",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_file(&path);

        let probe = probe_windows_11_arm_boot_disk_layout(WindowsArmBootDiskLayoutOptions {
            disk_path: path.clone(),
            size_gib: WINDOWS_ARM_BOOT_DISK_MIN_SIZE_GIB,
            create: true,
        });
        let output = probe.render_text();
        let metadata = std::fs::metadata(&path).unwrap();
        let _ = std::fs::remove_file(&path);

        assert_eq!(probe.disk_path, path);
        assert_eq!(probe.requested_size_gib, WINDOWS_ARM_BOOT_DISK_MIN_SIZE_GIB);
        assert_eq!(
            probe.disk_size_bytes,
            gib_to_bytes(WINDOWS_ARM_BOOT_DISK_MIN_SIZE_GIB)
        );
        assert!(probe.create_requested);
        assert!(probe.created);
        assert!(probe.reopened_for_verification);
        assert!(probe.protective_mbr_verified);
        assert!(probe.primary_gpt_verified);
        assert!(probe.backup_gpt_verified);
        assert!(probe.partition_entries_verified);
        assert_eq!(
            metadata.len(),
            gib_to_bytes(WINDOWS_ARM_BOOT_DISK_MIN_SIZE_GIB).unwrap()
        );
        assert_eq!(probe.partitions.len(), 3);
        assert_eq!(probe.partitions[0].name, "EFI System Partition");
        assert_eq!(probe.partitions[1].name, "Microsoft Reserved");
        assert_eq!(probe.partitions[2].name, "Windows Basic Data");
        assert!(probe.blockers.is_empty());
        assert!(output.contains("Windows 11 Arm HVF boot disk layout probe"));
        assert!(output.contains("QEMU: not used"));
        assert!(output.contains("Apple VZ: not used"));
        assert!(output.contains("HVF: not entered"));
        assert!(output.contains("Create requested: true"));
        assert!(output.contains("Created: true"));
        assert!(output.contains("Protective MBR verified: true"));
        assert!(output.contains("Primary GPT verified: true"));
        assert!(output.contains("Backup GPT verified: true"));
        assert!(output.contains("Partition entries verified: true"));
        assert!(output.contains("EFI System Partition"));
        assert!(output.contains("Microsoft Reserved"));
        assert!(output.contains("Windows Basic Data"));
        assert!(output.contains("Blockers: none"));
        assert!(!output.contains("qemu-system"));
        assert!(!output.contains('%'));
    }

    #[test]
    fn windows_11_arm_boot_disk_layout_probe_without_create_is_metadata_safe() {
        let path = std::env::temp_dir().join(format!(
            "bridgevm-windows-arm-boot-disk-layout-missing-{}-{}.raw",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_file(&path);

        let probe = probe_windows_11_arm_boot_disk_layout(WindowsArmBootDiskLayoutOptions {
            disk_path: path,
            size_gib: WINDOWS_ARM_BOOT_DISK_MIN_SIZE_GIB,
            create: false,
        });
        let output = probe.render_text();

        assert!(!probe.create_requested);
        assert!(!probe.created);
        assert!(!probe.reopened_for_verification);
        assert!(!probe.protective_mbr_verified);
        assert!(!probe.primary_gpt_verified);
        assert!(!probe.backup_gpt_verified);
        assert!(!probe.partition_entries_verified);
        assert_eq!(probe.partitions.len(), 3);
        assert!(probe
            .blockers
            .iter()
            .any(|blocker| blocker.contains("pass --create")));
        assert!(output.contains("Create requested: false"));
        assert!(output.contains("Created: false"));
        assert!(output.contains("disk file does not exist"));
        assert!(!output.contains("qemu-system"));
        assert!(!output.contains('%'));
    }

    #[test]
    fn windows_11_arm_uefi_firmware_handoff_probe_creates_and_verifies_vars() {
        let stem = format!(
            "bridgevm-windows-arm-uefi-handoff-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let firmware_path = std::env::temp_dir().join(format!("{stem}-code.fd"));
        let template_path = std::env::temp_dir().join(format!("{stem}-vars-template.fd"));
        let vars_path = std::env::temp_dir().join(format!("{stem}-vars.fd"));
        std::fs::write(&firmware_path, test_uefi_fv_bytes(128 * 1024)).unwrap();
        std::fs::write(&template_path, test_uefi_fv_bytes(64 * 1024)).unwrap();
        let _ = std::fs::remove_file(&vars_path);

        let probe =
            probe_windows_11_arm_uefi_firmware_handoff(WindowsArmUefiFirmwareHandoffOptions {
                firmware_path: firmware_path.clone(),
                vars_template_path: Some(template_path.clone()),
                vars_path: Some(vars_path.clone()),
                create_vars: true,
            });
        let output = probe.render_text();
        let vars_bytes = std::fs::read(&vars_path).unwrap();
        let template_bytes = std::fs::read(&template_path).unwrap();
        let _ = std::fs::remove_file(&firmware_path);
        let _ = std::fs::remove_file(&template_path);
        let _ = std::fs::remove_file(&vars_path);

        assert_eq!(vars_bytes, template_bytes);
        assert_eq!(probe.firmware_path, firmware_path);
        assert_eq!(probe.firmware_bytes, Some(128 * 1024));
        assert!(probe.firmware_verified);
        assert_eq!(probe.firmware_slot_ipa, WINDOWS_ARM_UEFI_CODE_IPA);
        assert_eq!(probe.firmware_slot_bytes, WINDOWS_ARM_UEFI_SLOT_BYTES);
        assert_eq!(probe.vars_template_path, Some(template_path));
        assert_eq!(probe.vars_template_bytes, Some(64 * 1024));
        assert!(probe.vars_template_verified);
        assert_eq!(probe.vars_path, Some(vars_path));
        assert_eq!(probe.vars_bytes, Some(64 * 1024));
        assert_eq!(probe.vars_slot_ipa, WINDOWS_ARM_UEFI_VARS_IPA);
        assert_eq!(probe.vars_slot_bytes, WINDOWS_ARM_UEFI_SLOT_BYTES);
        assert!(probe.vars_created);
        assert!(probe.vars_reopened_for_verification);
        assert!(probe.vars_verified);
        assert_eq!(
            probe.planned_reset_vector_ipa,
            Some(WINDOWS_ARM_UEFI_CODE_IPA)
        );
        assert!(probe
            .firmware_volume
            .as_ref()
            .is_some_and(|volume| volume.checksum_verified));
        assert!(probe
            .vars_volume
            .as_ref()
            .is_some_and(|volume| volume.checksum_verified));
        assert!(probe.blockers.is_empty());
        assert!(output.contains("Windows 11 Arm HVF UEFI firmware handoff probe"));
        assert!(output.contains("QEMU: not used"));
        assert!(output.contains("Apple VZ: not used"));
        assert!(output.contains("HVF: not entered"));
        assert!(output.contains("AArch64 UEFI firmware and vars pflash handoff"));
        assert!(output.contains("Firmware verified: true"));
        assert!(output.contains("Firmware volume detected: true"));
        assert!(output.contains("Firmware volume checksum verified: true"));
        assert!(output.contains("Vars template verified: true"));
        assert!(output.contains("Vars created: true"));
        assert!(output.contains("Vars reopened for verification: true"));
        assert!(output.contains("Vars verified: true"));
        assert!(output.contains("Vars volume checksum verified: true"));
        assert!(output.contains("Planned reset vector IPA: 0x8000000"));
        assert!(output.contains("Blockers: none"));
        assert!(!output.contains("qemu-system"));
        assert!(!output.contains('%'));
    }

    #[test]
    fn windows_11_arm_uefi_firmware_handoff_probe_requires_vars_store() {
        let firmware_path = std::env::temp_dir().join(format!(
            "bridgevm-windows-arm-uefi-handoff-missing-vars-{}-{}.fd",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::write(&firmware_path, test_uefi_fv_bytes(64 * 1024)).unwrap();

        let probe =
            probe_windows_11_arm_uefi_firmware_handoff(WindowsArmUefiFirmwareHandoffOptions {
                firmware_path: firmware_path.clone(),
                vars_template_path: None,
                vars_path: None,
                create_vars: false,
            });
        let output = probe.render_text();
        let _ = std::fs::remove_file(&firmware_path);

        assert!(probe.firmware_verified);
        assert!(!probe.vars_verified);
        assert_eq!(probe.planned_reset_vector_ipa, None);
        assert!(probe
            .blockers
            .iter()
            .any(|blocker| blocker.contains("UEFI variable store is required")));
        assert!(output.contains("Firmware verified: true"));
        assert!(output.contains("Vars verified: false"));
        assert!(output.contains("UEFI variable store is required"));
        assert!(!output.contains("qemu-system"));
        assert!(!output.contains('%'));
    }

    #[test]
    fn windows_11_arm_uefi_pflash_map_probe_loads_verified_slots() {
        let stem = format!(
            "bridgevm-windows-arm-uefi-pflash-map-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let firmware_path = std::env::temp_dir().join(format!("{stem}-code.fd"));
        let template_path = std::env::temp_dir().join(format!("{stem}-vars-template.fd"));
        let vars_path = std::env::temp_dir().join(format!("{stem}-vars.fd"));
        std::fs::write(&firmware_path, test_uefi_fv_bytes(128 * 1024)).unwrap();
        std::fs::write(&template_path, test_uefi_fv_bytes(64 * 1024)).unwrap();
        let _ = std::fs::remove_file(&vars_path);

        let probe = probe_windows_11_arm_uefi_pflash_map(WindowsArmUefiPflashMapOptions {
            firmware_path: firmware_path.clone(),
            vars_template_path: Some(template_path.clone()),
            vars_path: Some(vars_path.clone()),
            create_vars: true,
        });
        let output = probe.render_text();
        let vars_bytes = std::fs::read(&vars_path).unwrap();
        let template_bytes = std::fs::read(&template_path).unwrap();
        let _ = std::fs::remove_file(&firmware_path);
        let _ = std::fs::remove_file(&template_path);
        let _ = std::fs::remove_file(&vars_path);

        assert_eq!(vars_bytes, template_bytes);
        assert_eq!(probe.firmware_path, firmware_path);
        assert_eq!(probe.vars_path, Some(vars_path));
        assert!(probe.vars_created);
        assert!(probe.firmware_verified);
        assert!(probe.vars_verified);
        assert!(probe.pflash_slots_non_overlapping);
        assert!(probe.guest_ram_overlap_verified);
        assert!(probe.device_mmio_overlap_verified);
        assert!(probe.pflash_map_verified);
        assert_eq!(
            probe.planned_reset_vector_ipa,
            Some(WINDOWS_ARM_UEFI_CODE_IPA)
        );
        let firmware_slot = probe.firmware_slot.as_ref().unwrap();
        assert_eq!(firmware_slot.name, "code");
        assert_eq!(firmware_slot.ipa_start, WINDOWS_ARM_UEFI_CODE_IPA);
        assert_eq!(firmware_slot.ipa_end_exclusive(), WINDOWS_ARM_UEFI_VARS_IPA);
        assert_eq!(firmware_slot.source_bytes, 128 * 1024);
        assert_eq!(
            firmware_slot.zero_padding_bytes,
            WINDOWS_ARM_UEFI_SLOT_BYTES - 128 * 1024
        );
        assert!(!firmware_slot.writable);
        assert!(firmware_slot.prefix_verified);
        assert!(firmware_slot.padding_zeroed);
        let vars_slot = probe.vars_slot.as_ref().unwrap();
        assert_eq!(vars_slot.name, "vars");
        assert_eq!(vars_slot.ipa_start, WINDOWS_ARM_UEFI_VARS_IPA);
        assert_eq!(vars_slot.ipa_end_exclusive(), WINDOWS_ARM_DEVICE_MMIO_IPA);
        assert_eq!(vars_slot.source_bytes, 64 * 1024);
        assert!(vars_slot.writable);
        assert!(vars_slot.prefix_verified);
        assert!(vars_slot.padding_zeroed);
        assert!(probe.blockers.is_empty());
        assert!(output.contains("Windows 11 Arm HVF UEFI pflash map probe"));
        assert!(output.contains("QEMU: not used"));
        assert!(output.contains("Apple VZ: not used"));
        assert!(output.contains("HVF: not entered"));
        assert!(output.contains("AArch64 UEFI pflash slots loaded into memory images"));
        assert!(output.contains("Firmware pflash loaded: true"));
        assert!(output.contains("Firmware pflash IPA range: 0x8000000..0xc000000"));
        assert!(output.contains("Firmware pflash source bytes: 0x20000"));
        assert!(output.contains("Firmware pflash prefix verified: true"));
        assert!(output.contains("Firmware pflash padding zeroed: true"));
        assert!(output.contains("Vars pflash loaded: true"));
        assert!(output.contains("Vars pflash IPA range: 0xc000000..0x10000000"));
        assert!(output.contains("Vars pflash source bytes: 0x10000"));
        assert!(output.contains("Vars pflash writable: true"));
        assert!(output.contains("Pflash slots non-overlapping: true"));
        assert!(output.contains("Guest RAM overlap verified: true"));
        assert!(output.contains("Device MMIO overlap verified: true"));
        assert!(output.contains("Pflash map verified: true"));
        assert!(output.contains("Planned reset vector IPA: 0x8000000"));
        assert!(output.contains("Blockers: none"));
        assert!(!output.contains("qemu-system"));
        assert!(!output.contains('%'));
    }

    #[test]
    fn pflash_slot_load_rejects_oversized_file_before_allocation() {
        let path = std::env::temp_dir().join(format!(
            "bridgevm-oversized-pflash-{}-{}.fd",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let file = std::fs::File::create(&path).unwrap();
        file.set_len(512 * 1024 * 1024).unwrap();

        let error = load_uefi_pflash_slot("code", &path, WINDOWS_ARM_UEFI_CODE_IPA, 4096, false)
            .unwrap_err();
        let _ = std::fs::remove_file(&path);

        assert!(error.contains("536870912 bytes"), "{error}");
        assert!(error.contains("4096 byte region"), "{error}");
    }

    #[test]
    fn windows_11_arm_uefi_pflash_hvf_map_probe_defaults_to_no_live_map() {
        let stem = format!(
            "bridgevm-windows-arm-uefi-pflash-hvf-map-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let firmware_path = std::env::temp_dir().join(format!("{stem}-code.fd"));
        let template_path = std::env::temp_dir().join(format!("{stem}-vars-template.fd"));
        let vars_path = std::env::temp_dir().join(format!("{stem}-vars.fd"));
        std::fs::write(&firmware_path, test_uefi_fv_bytes(128 * 1024)).unwrap();
        std::fs::write(&template_path, test_uefi_fv_bytes(64 * 1024)).unwrap();
        let _ = std::fs::remove_file(&vars_path);

        let probe = probe_windows_11_arm_uefi_pflash_hvf_map(
            WindowsArmUefiPflashMapOptions {
                firmware_path: firmware_path.clone(),
                vars_template_path: Some(template_path.clone()),
                vars_path: Some(vars_path.clone()),
                create_vars: true,
            },
            false,
        );
        let output = probe.render_text();
        let _ = std::fs::remove_file(&firmware_path);
        let _ = std::fs::remove_file(&template_path);
        let _ = std::fs::remove_file(&vars_path);

        assert!(!probe.allowed);
        assert!(!probe.attempted);
        assert!(!probe.vm_created);
        assert!(!probe.firmware_memory_allocated);
        assert!(!probe.vars_memory_allocated);
        assert!(!probe.firmware_memory_mapped);
        assert!(!probe.vars_memory_mapped);
        assert!(!probe.vm_destroyed);
        assert!(probe.pflash_map_verified);
        assert_eq!(probe.firmware_slot_ipa, WINDOWS_ARM_UEFI_CODE_IPA);
        assert_eq!(probe.vars_slot_ipa, WINDOWS_ARM_UEFI_VARS_IPA);
        assert_eq!(probe.slot_bytes, WINDOWS_ARM_UEFI_SLOT_BYTES);
        assert_eq!(probe.firmware_source_bytes, Some(128 * 1024));
        assert_eq!(probe.vars_source_bytes, Some(64 * 1024));
        assert_eq!(probe.firmware_map_flags, "read|exec");
        assert_eq!(probe.vars_map_flags, "read|write");
        assert!(probe
            .blockers
            .iter()
            .any(|blocker| blocker.contains("BRIDGEVM_HVF_ALLOW_UEFI_PFLASH_MAP")));
        assert!(output.contains("Windows 11 Arm HVF UEFI pflash HVF map/unmap probe"));
        assert!(output.contains("QEMU: not used"));
        assert!(output.contains("Apple VZ: not used"));
        assert!(output.contains("Guest execution: not entered"));
        assert!(output.contains("Allowed: false"));
        assert!(output.contains("Attempted: false"));
        assert!(output.contains("Pflash map verified: true"));
        assert!(output.contains("Firmware slot IPA: 0x8000000"));
        assert!(output.contains("Vars slot IPA: 0xc000000"));
        assert!(output.contains("Slot bytes: 0x4000000"));
        assert!(output.contains("Firmware source bytes: 0x20000"));
        assert!(output.contains("Vars source bytes: 0x10000"));
        assert!(output.contains("Firmware map flags: read|exec"));
        assert!(output.contains("Vars map flags: read|write"));
        assert!(output.contains("BRIDGEVM_HVF_ALLOW_UEFI_PFLASH_MAP"));
        assert!(!output.contains("qemu-system"));
        assert!(!output.contains('%'));
    }

    #[test]
    fn windows_11_arm_uefi_reset_vector_entry_probe_defaults_to_no_live_entry() {
        let stem = format!(
            "bridgevm-windows-arm-uefi-reset-vector-entry-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let firmware_path = std::env::temp_dir().join(format!("{stem}-code.fd"));
        let template_path = std::env::temp_dir().join(format!("{stem}-vars-template.fd"));
        let vars_path = std::env::temp_dir().join(format!("{stem}-vars.fd"));
        std::fs::write(&firmware_path, test_uefi_fv_bytes(128 * 1024)).unwrap();
        std::fs::write(&template_path, test_uefi_fv_bytes(64 * 1024)).unwrap();
        let _ = std::fs::remove_file(&vars_path);

        let probe = probe_windows_11_arm_uefi_reset_vector_entry(
            WindowsArmUefiPflashMapOptions {
                firmware_path: firmware_path.clone(),
                vars_template_path: Some(template_path.clone()),
                vars_path: Some(vars_path.clone()),
                create_vars: true,
            },
            false,
        );
        let output = probe.render_text();
        let _ = std::fs::remove_file(&firmware_path);
        let _ = std::fs::remove_file(&template_path);
        let _ = std::fs::remove_file(&vars_path);

        assert!(!probe.allowed);
        assert!(!probe.attempted);
        assert!(!probe.vm_created);
        assert!(!probe.firmware_memory_allocated);
        assert!(!probe.vars_memory_allocated);
        assert!(!probe.firmware_memory_mapped);
        assert!(!probe.vars_memory_mapped);
        assert!(!probe.vcpu_created);
        assert!(!probe.pc_set);
        assert!(!probe.cpsr_set);
        assert!(!probe.run_attempted);
        assert!(!probe.reset_vector_entry_observed);
        assert!(!probe.firmware_progress_observed);
        assert!(!probe.vm_destroyed);
        assert!(probe.pflash_map_verified);
        assert_eq!(probe.reset_vector_ipa, WINDOWS_ARM_UEFI_CODE_IPA);
        assert_eq!(probe.firmware_slot_ipa, WINDOWS_ARM_UEFI_CODE_IPA);
        assert_eq!(probe.vars_slot_ipa, WINDOWS_ARM_UEFI_VARS_IPA);
        assert_eq!(probe.slot_bytes, WINDOWS_ARM_UEFI_SLOT_BYTES);
        assert_eq!(probe.firmware_source_bytes, Some(128 * 1024));
        assert_eq!(probe.vars_source_bytes, Some(64 * 1024));
        assert_eq!(probe.firmware_map_flags, "read|exec");
        assert_eq!(probe.vars_map_flags, "read|write");
        assert!(probe
            .blockers
            .iter()
            .any(|blocker| blocker.contains("BRIDGEVM_HVF_ALLOW_UEFI_RESET_VECTOR_ENTRY")));
        assert!(output.contains("Windows 11 Arm HVF UEFI reset-vector entry probe"));
        assert!(output.contains("QEMU: not used"));
        assert!(output.contains("Apple VZ: not used"));
        assert!(output.contains("Guest execution: UEFI reset vector entered under watchdog"));
        assert!(output.contains("Windows boot: not claimed"));
        assert!(output.contains("Allowed: false"));
        assert!(output.contains("Attempted: false"));
        assert!(output.contains("Pflash map verified: true"));
        assert!(output.contains("Reset vector IPA: 0x8000000"));
        assert!(output.contains("Firmware source bytes: 0x20000"));
        assert!(output.contains("Vars source bytes: 0x10000"));
        assert!(output.contains("VM create status name: not attempted"));
        assert!(output.contains("Run status name: not attempted"));
        assert!(output.contains("Firmware progress observed: false"));
        assert!(output.contains("Exit exception class name: not observed"));
        assert!(output.contains("BRIDGEVM_HVF_ALLOW_UEFI_RESET_VECTOR_ENTRY"));
        assert!(!output.contains("qemu-system"));
        assert!(!output.contains('%'));
    }

    #[test]
    fn windows_11_arm_uefi_firmware_run_loop_probe_defaults_to_no_live_loop() {
        let stem = format!(
            "bridgevm-windows-arm-uefi-firmware-run-loop-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let firmware_path = std::env::temp_dir().join(format!("{stem}-code.fd"));
        let template_path = std::env::temp_dir().join(format!("{stem}-vars-template.fd"));
        let vars_path = std::env::temp_dir().join(format!("{stem}-vars.fd"));
        let installer_iso_path = std::env::temp_dir().join(format!("{stem}-win11-arm.iso"));
        let writable_target_disk_path = std::env::temp_dir().join(format!("{stem}-windows.raw"));
        std::fs::write(&firmware_path, test_uefi_fv_bytes(128 * 1024)).unwrap();
        std::fs::write(&template_path, test_uefi_fv_bytes(64 * 1024)).unwrap();
        let _ = std::fs::remove_file(&vars_path);

        let probe =
            probe_windows_11_arm_uefi_firmware_run_loop(WindowsArmUefiFirmwareRunLoopOptions {
                pflash: WindowsArmUefiPflashMapOptions {
                    firmware_path: firmware_path.clone(),
                    vars_template_path: Some(template_path.clone()),
                    vars_path: Some(vars_path.clone()),
                    create_vars: true,
                },
                execution: WindowsArmUefiFirmwareRunLoopExecutionOptions {
                    allow_loop: false,
                    requested_exits: 8,
                    guest_ram_mib: 64,
                    watchdog_timeout_ms: 100,
                    map_low_pflash_alias: false,
                    seed_diagnostic_vector: false,
                    seed_guest_ram_diagnostic_vector: false,
                    seed_executable_diagnostic_vector: false,
                    try_recommended_vector_base_vbar: false,
                    continue_after_recommended_vector_base_vbar: false,
                    repair_low_vector_diagnostic_page: false,
                    remap_low_vector_to_recommended_vector: false,
                    continue_after_low_vector_repair: false,
                    restore_low_vector_slot_before_eret: false,
                    wire_interrupt_timer: false,
                    stop_at_first_post_repair_device_boundary: false,
                    installer_iso_path: Some(installer_iso_path.clone()),
                    writable_target_disk_path: Some(writable_target_disk_path.clone()),
                },
            });
        let output = probe.render_text();
        let _ = std::fs::remove_file(&firmware_path);
        let _ = std::fs::remove_file(&template_path);
        let _ = std::fs::remove_file(&vars_path);

        assert!(!probe.allowed);
        assert!(!probe.attempted);
        assert!(!probe.vm_created);
        assert!(!probe.guest_ram_memory_allocated);
        assert!(!probe.low_pflash_alias_requested);
        assert!(!probe.low_firmware_alias_mapped);
        assert!(!probe.low_vars_alias_mapped);
        assert!(!probe.guest_ram_memory_mapped);
        assert!(!probe.platform_dtb_populated);
        assert!(!probe.diagnostic_vector_seed_requested);
        assert!(!probe.diagnostic_vector_populated);
        assert!(!probe.low_vector_diagnostic_page_repair_requested);
        assert!(!probe.low_vector_diagnostic_page_repaired);
        assert!(!probe.low_vector_diagnostic_page_slot_restored);
        assert!(!probe.low_vector_diagnostic_page_restore_before_eret_requested);
        assert!(!probe.low_vector_diagnostic_page_restore_before_eret_attempted);
        assert_eq!(probe.low_vector_diagnostic_page_previous_descriptor, None);
        assert!(!probe.low_vector_diagnostic_page_repeated_fault_observed);
        assert!(!probe.low_vector_post_repair_continue_requested);
        assert!(!probe.low_vector_post_repair_continue_attempted);
        assert!(!probe.low_vector_post_repair_unsupported_exit_observed);
        assert_eq!(probe.low_vector_post_repair_unsupported_exit_reason, None);
        assert_eq!(
            probe.low_vector_post_repair_unsupported_exit_diagnosis,
            "not observed"
        );
        assert!(!probe.low_vector_post_repair_first_exit_observed);
        assert_eq!(probe.low_vector_post_repair_first_exit_index, None);
        assert_eq!(probe.low_vector_post_repair_first_exit_reason, None);
        assert_eq!(
            probe.low_vector_post_repair_first_exit_diagnosis,
            "not observed"
        );
        assert_eq!(probe.low_vector_post_repair_first_exit_pc, None);
        assert_eq!(
            probe.low_vector_post_repair_first_interaction_kind,
            "not observed"
        );
        assert_eq!(
            probe.low_vector_post_repair_first_exit_access_kind,
            "not observed"
        );
        assert_eq!(
            probe.low_vector_post_repair_first_exit_access_direction,
            "not observed"
        );
        assert_eq!(probe.low_vector_post_repair_first_exit_access_address, None);
        assert_eq!(probe.low_vector_post_repair_first_exit_access_sysreg, None);
        assert_eq!(
            probe.low_vector_post_repair_first_exit_access_syndrome,
            None
        );
        assert!(!probe.low_vector_post_repair_first_device_interaction_observed);
        assert_eq!(
            probe.low_vector_post_repair_first_device_interaction_index,
            None
        );
        assert_eq!(
            probe.low_vector_post_repair_first_device_interaction_reason,
            None
        );
        assert_eq!(
            probe.low_vector_post_repair_first_device_interaction_diagnosis,
            "not observed"
        );
        assert_eq!(
            probe.low_vector_post_repair_first_device_interaction_pc,
            None
        );
        assert_eq!(
            probe.low_vector_post_repair_first_device_interaction_kind,
            "not observed"
        );
        assert_eq!(
            probe.low_vector_post_repair_first_device_interaction_access_kind,
            "not observed"
        );
        assert_eq!(
            probe.low_vector_post_repair_first_device_interaction_access_direction,
            "not observed"
        );
        assert_eq!(
            probe.low_vector_post_repair_first_device_interaction_access_address,
            None
        );
        assert_eq!(
            probe.low_vector_post_repair_first_device_interaction_access_sysreg,
            None
        );
        assert_eq!(
            probe.low_vector_post_repair_first_device_interaction_access_syndrome,
            None
        );
        assert!(!probe.low_vector_post_repair_first_unhandled_access_observed);
        assert_eq!(
            probe.low_vector_post_repair_first_unhandled_access_index,
            None
        );
        assert_eq!(
            probe.low_vector_post_repair_first_unhandled_access_reason,
            None
        );
        assert_eq!(
            probe.low_vector_post_repair_first_unhandled_access_diagnosis,
            "not observed"
        );
        assert_eq!(probe.low_vector_post_repair_first_unhandled_access_pc, None);
        assert_eq!(
            probe.low_vector_post_repair_first_unhandled_access_syndrome,
            None
        );
        assert_eq!(
            probe.low_vector_post_repair_first_unhandled_access_kind,
            "not observed"
        );
        assert_eq!(
            probe.low_vector_post_repair_first_unhandled_access_direction,
            "not observed"
        );
        assert_eq!(
            probe.low_vector_post_repair_first_unhandled_access_register,
            None
        );
        assert_eq!(
            probe.low_vector_post_repair_first_unhandled_access_value,
            None
        );
        assert_eq!(
            probe.low_vector_post_repair_first_unhandled_access_handler_result,
            "not observed"
        );
        assert_eq!(
            probe.low_vector_post_repair_first_unhandled_access_mmio_ipa,
            None
        );
        assert_eq!(
            probe.low_vector_post_repair_first_unhandled_access_mmio_width,
            None
        );
        assert_eq!(
            probe.low_vector_post_repair_first_unhandled_access_mmio_device_kind,
            "not observed"
        );
        assert_eq!(
            probe.low_vector_post_repair_first_unhandled_access_sysreg,
            None
        );
        assert_eq!(
            probe.low_vector_post_repair_first_unhandled_access_sysreg_name,
            "not observed"
        );
        assert_eq!(
            probe.low_vector_post_repair_first_unhandled_access_sysreg_op0,
            None
        );
        assert_eq!(
            probe.low_vector_post_repair_first_unhandled_access_sysreg_op1,
            None
        );
        assert_eq!(
            probe.low_vector_post_repair_first_unhandled_access_sysreg_crn,
            None
        );
        assert_eq!(
            probe.low_vector_post_repair_first_unhandled_access_sysreg_crm,
            None
        );
        assert_eq!(
            probe.low_vector_post_repair_first_unhandled_access_sysreg_op2,
            None
        );
        assert!(!probe.low_vector_diagnostic_page_resume_attempted);
        assert!(!probe.low_vector_diagnostic_page_resume_armed);
        assert_eq!(probe.low_vector_diagnostic_page_resume_original_pc, None);
        assert_eq!(
            probe.low_vector_diagnostic_page_resume_original_elr_el1,
            None
        );
        assert_eq!(
            probe.low_vector_diagnostic_page_resume_original_esr_el1,
            None
        );
        assert_eq!(
            probe.low_vector_diagnostic_page_resume_original_far_el1,
            None
        );
        assert_eq!(
            probe.low_vector_diagnostic_page_resume_original_spsr_el1,
            None
        );
        assert_eq!(probe.low_vector_diagnostic_page_original_slot_bytes, None);
        assert_eq!(
            probe.low_vector_diagnostic_page_resume_target_instruction_before_eret,
            None
        );
        assert_eq!(
            probe.low_vector_diagnostic_page_resume_target_stage1_leaf_descriptor_before_eret,
            None
        );
        assert_eq!(
            probe.low_vector_diagnostic_page_resume_target_stage1_leaf_kind_before_eret,
            "not observed"
        );
        assert!(
            !probe.low_vector_diagnostic_page_resume_target_is_installed_diagnostic_hvc_before_eret
        );
        assert_eq!(
            probe.low_vector_diagnostic_page_resume_elr_el1_set_status,
            None
        );
        assert_eq!(
            probe.low_vector_diagnostic_page_resume_spsr_el1_set_status,
            None
        );
        assert_eq!(
            probe.low_vector_diagnostic_page_resume_cpsr_set_status,
            None
        );
        assert_eq!(probe.low_vector_diagnostic_page_resume_pc_set_status, None);
        assert!(!probe.interrupt_timer_wiring_requested);
        assert!(!probe.interrupt_timer_initialized);
        assert!(!probe.vcpu_created);
        assert!(!probe.pc_set);
        assert!(!probe.x0_dtb_ipa_set);
        assert!(!probe.cpsr_set);
        assert!(!probe.sp_el1_set);
        assert!(!probe.diagnostic_vector_vbar_el1_set);
        assert!(!probe.recommended_vector_base_vbar_requested);
        assert!(!probe.recommended_vector_base_vbar_attempted);
        assert!(!probe.recommended_vector_base_vbar_set);
        assert!(!probe.recommended_vector_base_vbar_diagnostic_vector_populated);
        assert_eq!(probe.recommended_vector_base_vbar_source_exit_index, None);
        assert_eq!(probe.recommended_vector_base_vbar_target, None);
        assert_eq!(
            probe.recommended_vector_base_vbar_target_physical_address,
            None
        );
        assert_eq!(probe.recommended_vector_base_vbar_reason, "not requested");
        assert_eq!(
            probe.recommended_vector_base_vbar_current_el_spx_sync_instruction_word,
            None
        );
        assert_eq!(
            probe.recommended_vector_base_vbar_current_el_spx_sync_instruction_hint,
            "not observed"
        );
        assert!(!probe.recommended_vector_base_vbar_followup_exit_observed);
        assert_eq!(probe.recommended_vector_base_vbar_followup_exit_index, None);
        assert_eq!(
            probe.recommended_vector_base_vbar_followup_exit_reason,
            None
        );
        assert_eq!(
            probe.recommended_vector_base_vbar_followup_exit_diagnosis,
            "not observed"
        );
        assert_eq!(probe.recommended_vector_base_vbar_followup_pc, None);
        assert_eq!(probe.recommended_vector_base_vbar_followup_vbar_el1, None);
        assert!(!probe.recommended_vector_base_vbar_followup_target_still_set);
        assert_eq!(probe.recommended_vector_base_vbar_set_status, None);
        assert!(!probe.run_loop_attempted);
        assert!(!probe.firmware_progress_observed);
        assert!(!probe.unsupported_exit_observed);
        assert_eq!(probe.requested_exits, 8);
        assert_eq!(probe.observed_exits, 0);
        assert_eq!(probe.watchdog_timeout_ms, 100);
        assert_eq!(probe.vtimer_offset_value, None);
        assert_eq!(probe.cntv_cval_value, None);
        assert_eq!(probe.cntv_ctl_value, None);
        assert_eq!(probe.vtimer_exit_count, 0);
        assert_eq!(probe.pending_irq_injected_count, 0);
        assert_eq!(probe.device_irq_injected_count, 0);
        assert_eq!(probe.device_irq_cleared_count, 0);
        assert_eq!(probe.handled_mmio_read_count, 0);
        assert_eq!(probe.handled_mmio_write_count, 0);
        assert_eq!(probe.handled_pl011_mmio_count, 0);
        assert_eq!(probe.handled_pl031_mmio_count, 0);
        assert_eq!(probe.handled_gicd_mmio_count, 0);
        assert_eq!(probe.handled_gicr_mmio_count, 0);
        assert_eq!(probe.handled_virtio_installer_iso_mmio_count, 0);
        assert_eq!(probe.handled_virtio_target_disk_mmio_count, 0);
        assert_eq!(probe.virtio_queue_notify_count, 0);
        assert_eq!(probe.virtio_request_completion_count, 0);
        assert_eq!(probe.guest_ram_ipa, WINDOWS_ARM_GUEST_RAM_IPA);
        assert_eq!(probe.platform_dtb_ipa, WINDOWS_ARM_PLATFORM_DTB_IPA);
        assert_eq!(
            probe.platform_dtb_guest_ram_offset,
            WINDOWS_ARM_PLATFORM_DTB_GUEST_RAM_OFFSET
        );
        assert_eq!(
            probe.low_firmware_alias_ipa,
            WINDOWS_ARM_UEFI_LOW_CODE_ALIAS_IPA
        );
        assert_eq!(
            probe.low_vars_alias_ipa,
            WINDOWS_ARM_UEFI_LOW_VARS_ALIAS_IPA
        );
        assert_eq!(probe.guest_ram_bytes, 64 * 1024 * 1024);
        assert!(probe.platform_dtb_bytes >= 40);
        assert_eq!(probe.platform_dtb_magic, FDT_MAGIC);
        assert!(probe.platform_dtb_magic_verified);
        assert_eq!(
            probe.sp_el1_seed_ipa,
            WINDOWS_ARM_GUEST_RAM_IPA + 64 * 1024 * 1024 - 16
        );
        assert_eq!(
            probe.diagnostic_vector_ipa,
            WINDOWS_ARM_DIAGNOSTIC_VECTOR_IPA
        );
        assert_eq!(probe.diagnostic_vector_location, "pflash");
        assert_eq!(
            probe.diagnostic_vector_bytes,
            WINDOWS_ARM_DIAGNOSTIC_VECTOR_BYTES
        );
        assert!(probe.pflash_map_verified);
        assert_eq!(probe.firmware_source_bytes, Some(128 * 1024));
        assert_eq!(probe.vars_source_bytes, Some(64 * 1024));
        assert_eq!(probe.installer_iso_path, Some(installer_iso_path.clone()));
        assert_eq!(
            probe.writable_target_disk_path,
            Some(writable_target_disk_path.clone())
        );
        assert_eq!(probe.block_devices.len(), 2);
        let installer_block = probe
            .block_devices
            .iter()
            .find(|device| device.role == "installer-iso")
            .expect("installer ISO block metadata is present");
        assert_eq!(installer_block.label, "VirtIO-MMIO installer ISO");
        assert_eq!(installer_block.node_name, "virtio_mmio@10002000");
        assert_eq!(
            installer_block.base_ipa,
            WINDOWS_ARM_VIRTIO_INSTALLER_ISO_MMIO_IPA
        );
        assert_eq!(installer_block.bytes, VIRTIO_MMIO_REGISTER_WINDOW_BYTES);
        assert!(installer_block.read_only);
        assert_eq!(installer_block.backing_kind, "host-iso-readonly");
        assert_eq!(
            installer_block.backing_path,
            Some(installer_iso_path.clone())
        );
        assert_eq!(installer_block.device_features, VIRTIO_BLK_F_RO);
        let target_block = probe
            .block_devices
            .iter()
            .find(|device| device.role == "target-disk")
            .expect("target disk block metadata is present");
        assert_eq!(target_block.label, "VirtIO-MMIO target disk");
        assert_eq!(target_block.node_name, "virtio_mmio@10003000");
        assert_eq!(
            target_block.base_ipa,
            WINDOWS_ARM_VIRTIO_TARGET_DISK_MMIO_IPA
        );
        assert_eq!(target_block.bytes, VIRTIO_MMIO_REGISTER_WINDOW_BYTES);
        assert!(!target_block.read_only);
        assert_eq!(target_block.backing_kind, "host-file-writable");
        assert_eq!(
            target_block.backing_path,
            Some(writable_target_disk_path.clone())
        );
        assert_eq!(
            target_block.device_features,
            VIRTIO_MMIO_BLOCK_DEVICE_FEATURES_VALUE
        );
        assert!(probe.exits.is_empty());
        assert!(probe
            .blockers
            .iter()
            .any(|blocker| blocker.contains("BRIDGEVM_HVF_ALLOW_UEFI_FIRMWARE_RUN_LOOP")));
        assert!(output.contains("Windows 11 Arm HVF UEFI firmware run-loop probe"));
        assert!(output.contains("QEMU: not used"));
        assert!(output.contains("Apple VZ: not used"));
        assert!(output.contains("Guest execution: bounded UEFI firmware exit classification loop"));
        assert!(output.contains("Windows boot: not claimed"));
        assert!(output.contains("Allowed: false"));
        assert!(output.contains("Attempted: false"));
        assert!(output.contains("Low firmware alias mapped: false"));
        assert!(output.contains("Low vars alias mapped: false"));
        assert!(output.contains("Low firmware alias IPA: 0x0"));
        assert!(output.contains("Low vars alias IPA: 0x4000000"));
        assert!(output.contains("Guest RAM IPA: 0x40000000"));
        assert!(output.contains("Platform DTB populated: false"));
        assert!(output.contains("X0 DTB IPA set: false"));
        assert!(output.contains("Platform DTB IPA: 0x40010000"));
        assert!(output.contains("Platform DTB guest RAM offset: 0x10000"));
        assert!(output.contains("Platform DTB bytes: 0x"));
        assert!(output.contains("Platform DTB magic: 0xd00dfeed"));
        assert!(output.contains("Platform DTB magic verified: true"));
        assert!(output.contains("SP_EL1 seed IPA: 0x43fffff0"));
        assert!(output.contains("Diagnostic vector seed requested: false"));
        assert!(output.contains("Diagnostic vector populated: false"));
        assert!(output.contains("Recommended vector-base VBAR requested: false"));
        assert!(output.contains("Recommended vector-base VBAR attempted: false"));
        assert!(output.contains("Recommended vector-base VBAR set: false"));
        assert!(output.contains("Low vector diagnostic page repair requested: false"));
        assert!(output.contains("Low vector diagnostic page repaired: false"));
        assert!(output.contains("Low vector diagnostic page slot restored: false"));
        assert!(output.contains("Low vector diagnostic page restore before ERET requested: false"));
        assert!(output.contains("Low vector diagnostic page restore before ERET attempted: false"));
        assert!(output.contains("Low vector diagnostic page previous descriptor: not observed"));
        assert!(output.contains("Low vector diagnostic page repeated fault observed: false"));
        assert!(output.contains("Continue after low-vector repair requested: false"));
        assert!(output.contains("Continue after low-vector repair attempted: false"));
        assert!(output.contains("Post-repair unsupported exit observed: false"));
        assert!(output.contains("Post-repair unsupported exit reason name: not observed"));
        assert!(output.contains("Post-repair unsupported exit classification: not observed"));
        assert!(output.contains("Post-repair first exit observed: false"));
        assert!(output.contains("Post-repair first exit: not observed"));
        assert!(output.contains("Post-repair first exit reason name: not observed"));
        assert!(output.contains("Post-repair first exit classification: not observed"));
        assert!(output.contains("Post-repair first exit PC: not observed"));
        assert!(output.contains("Post-repair first exit instruction: not observed"));
        assert!(output.contains("Post-repair first exit instruction hint: not observed"));
        assert!(output.contains("Post-repair first exit VBAR_EL1: not observed"));
        assert!(output.contains("Post-repair first exit ELR_EL1: not observed"));
        assert!(output.contains("Post-repair first exit ESR_EL1: not observed"));
        assert!(output.contains("Post-repair first exit FAR_EL1: not observed"));
        assert!(output.contains("Post-repair first exit SPSR_EL1: not observed"));
        assert!(output.contains("Post-repair first exit access kind: not observed"));
        assert!(output.contains("Post-repair first exit access direction: not observed"));
        assert!(output.contains("Post-repair first exit access address: not observed"));
        assert!(output.contains("Post-repair first exit access sysreg: not observed"));
        assert!(output.contains("Post-repair first exit access syndrome: not observed"));
        assert!(output.contains("Post-repair first interaction kind: not observed"));
        assert!(output.contains("Post-repair first device interaction observed: false"));
        assert!(output.contains("Post-repair first device interaction: not observed"));
        assert!(output.contains("Post-repair first device interaction reason name: not observed"));
        assert!(
            output.contains("Post-repair first device interaction classification: not observed")
        );
        assert!(output.contains("Post-repair first device interaction PC: not observed"));
        assert!(output.contains("Post-repair first device interaction instruction: not observed"));
        assert!(
            output.contains("Post-repair first device interaction instruction hint: not observed")
        );
        assert!(output.contains("Post-repair first device interaction VBAR_EL1: not observed"));
        assert!(output.contains("Post-repair first device interaction ELR_EL1: not observed"));
        assert!(output.contains("Post-repair first device interaction ESR_EL1: not observed"));
        assert!(output.contains("Post-repair first device interaction FAR_EL1: not observed"));
        assert!(output.contains("Post-repair first device interaction SPSR_EL1: not observed"));
        assert!(output.contains("Post-repair first device interaction access kind: not observed"));
        assert!(
            output.contains("Post-repair first device interaction access direction: not observed")
        );
        assert!(
            output.contains("Post-repair first device interaction access address: not observed")
        );
        assert!(output.contains("Post-repair first device interaction access sysreg: not observed"));
        assert!(
            output.contains("Post-repair first device interaction access syndrome: not observed")
        );
        assert!(output.contains("Post-repair first device interaction kind: not observed"));
        assert!(output.contains("Post-repair first unhandled access observed: false"));
        assert!(output.contains("Post-repair first unhandled access: not observed"));
        assert!(output.contains("Post-repair first unhandled access reason name: not observed"));
        assert!(output.contains("Post-repair first unhandled access classification: not observed"));
        assert!(output.contains("Post-repair first unhandled access PC: not observed"));
        assert!(output.contains("Post-repair first unhandled access syndrome: not observed"));
        assert!(output.contains("Post-repair first unhandled access kind: not observed"));
        assert!(output.contains("Post-repair first unhandled access direction: not observed"));
        assert!(output.contains("Post-repair first unhandled access register: not observed"));
        assert!(output.contains("Post-repair first unhandled access value: not observed"));
        assert!(output.contains("Post-repair first unhandled access handler result: not observed"));
        assert!(output.contains("Post-repair first unhandled access MMIO IPA: not observed"));
        assert!(output.contains("Post-repair first unhandled access MMIO width: not observed"));
        assert!(
            output.contains("Post-repair first unhandled access MMIO device kind: not observed")
        );
        assert!(output.contains("Post-repair first unhandled access sysreg: not observed"));
        assert!(output.contains("Post-repair first unhandled access sysreg name: not observed"));
        assert!(output.contains("Post-repair first unhandled access sysreg op0: not observed"));
        assert!(output.contains("Post-repair first unhandled access sysreg op1: not observed"));
        assert!(output.contains("Post-repair first unhandled access sysreg crn: not observed"));
        assert!(output.contains("Post-repair first unhandled access sysreg crm: not observed"));
        assert!(output.contains("Post-repair first unhandled access sysreg op2: not observed"));
        assert!(output.contains("Low vector diagnostic page resume attempted: false"));
        assert!(output.contains("Low vector diagnostic page resume armed: false"));
        assert!(output.contains("Low vector diagnostic page resume original PC: not observed"));
        assert!(output.contains("Low vector diagnostic page resume original ELR_EL1: not observed"));
        assert!(output.contains("Low vector diagnostic page resume original ESR_EL1: not observed"));
        assert!(output.contains("Low vector diagnostic page resume original FAR_EL1: not observed"));
        assert!(
            output.contains("Low vector diagnostic page resume original SPSR_EL1: not observed")
        );
        assert!(output.contains("Diagnostic vector VBAR_EL1 set: false"));
        assert!(output.contains("Interrupt/timer wiring requested: false"));
        assert!(output.contains("Interrupt/timer initialized: false"));
        assert!(output.contains("Diagnostic vector location: pflash"));
        assert!(output.contains("Diagnostic vector IPA: 0x8000000"));
        assert!(output.contains("Diagnostic vector bytes: 0x800"));
        assert!(output.contains("Recommended vector-base VBAR source exit: not observed"));
        assert!(output.contains("Recommended vector-base VBAR target: not observed"));
        assert!(output.contains("Recommended vector-base VBAR target PA: not observed"));
        assert!(output.contains("Recommended vector-base VBAR reason: not requested"));
        assert!(output.contains(
            "Recommended vector-base VBAR current EL/SPx sync instruction: not observed"
        ));
        assert!(
            output.contains("Recommended vector-base VBAR current EL/SPx sync hint: not observed")
        );
        assert!(output.contains("Recommended vector-base VBAR follow-up exit observed: false"));
        assert!(output.contains("Recommended vector-base VBAR follow-up exit: not observed"));
        assert!(output
            .contains("Recommended vector-base VBAR follow-up exit reason name: not observed"));
        assert!(
            output.contains("Recommended vector-base VBAR follow-up classification: not observed")
        );
        assert!(output.contains("Recommended vector-base VBAR follow-up PC: not observed"));
        assert!(output.contains("Recommended vector-base VBAR follow-up VBAR_EL1: not observed"));
        assert!(output.contains("Recommended vector-base VBAR follow-up target still set: false"));
        assert!(output.contains("Low firmware alias map flags: read|exec"));
        assert!(output.contains("Low vars alias map flags: read|write"));
        assert!(output.contains("Low pflash alias requested: false"));
        assert!(output.contains("Low firmware alias map status name: not attempted"));
        assert!(output.contains("Low vars alias map status name: not attempted"));
        assert!(output.contains("Guest RAM bytes: 0x4000000"));
        assert!(output.contains("Requested exits: 8"));
        assert!(output.contains("Observed exits: 0"));
        assert!(output.contains("Watchdog timeout ms: 100"));
        assert!(output.contains("VTimer offset value: not observed"));
        assert!(output.contains("CNTV_CVAL_EL0 value: not observed"));
        assert!(output.contains("CNTV_CTL_EL0 value: not observed"));
        assert!(output.contains("VTimer exit count: 0"));
        assert!(output.contains("Pending IRQ injected count: 0"));
        assert!(output.contains("Device IRQ line asserted count: 0"));
        assert!(output.contains("Device IRQ line deasserted count: 0"));
        assert!(output.contains("Handled MMIO read count: 0"));
        assert!(output.contains("Handled MMIO write count: 0"));
        assert!(output.contains("Handled PL011 MMIO count: 0"));
        assert!(output.contains("Handled PL031 MMIO count: 0"));
        assert!(output.contains("Handled GICD MMIO count: 0"));
        assert!(output.contains("Handled GICR MMIO count: 0"));
        assert!(output.contains("Handled VirtIO installer ISO MMIO count: 0"));
        assert!(output.contains("Handled VirtIO target disk MMIO count: 0"));
        assert!(output.contains("VirtIO queue_notify count: 0"));
        assert!(output.contains("VirtIO request completion count: 0"));
        assert!(output.contains("Handled ICC read count: 0"));
        assert!(output.contains("Handled ICC write count: 0"));
        assert!(output.contains("Handled ICC_IAR1 read count: 0"));
        assert!(output.contains("Handled ICC_EOIR1 write count: 0"));
        assert!(output.contains("Handled ICC_DIR write count: 0"));
        assert!(output.contains("Last ICC_IAR1 INTID: not observed"));
        assert!(output.contains("Last ICC_EOIR1 INTID: not observed"));
        assert!(output.contains("Last ICC_DIR INTID: not observed"));
        assert!(output.contains(&format!(
            "Installer ISO path: {}",
            installer_iso_path.display()
        )));
        assert!(output.contains(&format!(
            "Writable target disk path: {}",
            writable_target_disk_path.display()
        )));
        assert!(output.contains("Firmware block devices:"));
        assert!(output.contains(&format!(
            "- role=installer-iso, label=VirtIO-MMIO installer ISO, node=virtio_mmio@10002000, base=0x10002000, bytes=0x1000, read_only=true, backing_kind=host-iso-readonly, backing_path={}, device_features=0x20",
            installer_iso_path.display()
        )));
        assert!(output.contains(&format!(
            "- role=target-disk, label=VirtIO-MMIO target disk, node=virtio_mmio@10003000, base=0x10003000, bytes=0x1000, read_only=false, backing_kind=host-file-writable, backing_path={}, device_features=0x0",
            writable_target_disk_path.display()
        )));
        assert!(output.contains("VTimer offset set status name: not attempted"));
        assert!(output.contains("Recommended vector-base VBAR set status name: not attempted"));
        assert!(output.contains("Recommended vector-base VBAR resume requested: false"));
        assert!(output.contains("Recommended vector-base VBAR resume attempted: false"));
        assert!(output.contains("Recommended vector-base VBAR resume armed: false"));
        assert!(output.contains("Recommended vector-base VBAR resume original PC: not observed"));
        assert!(
            output.contains("Recommended vector-base VBAR resume original ELR_EL1: not observed")
        );
        assert!(
            output.contains("Recommended vector-base VBAR resume original ESR_EL1: not observed")
        );
        assert!(
            output.contains("Recommended vector-base VBAR resume original FAR_EL1: not observed")
        );
        assert!(
            output.contains("Recommended vector-base VBAR resume original SPSR_EL1: not observed")
        );
        assert!(output.contains(
            "Recommended vector-base VBAR resume ELR_EL1 set status name: not attempted"
        ));
        assert!(output.contains(
            "Recommended vector-base VBAR resume VBAR_EL1 set status name: not attempted"
        ));
        assert!(output.contains(
            "Recommended vector-base VBAR resume SPSR_EL1 set status name: not attempted"
        ));
        assert!(output
            .contains("Recommended vector-base VBAR resume PC set status name: not attempted"));
        assert!(output.contains("X0 DTB IPA set status name: not attempted"));
        assert!(output.contains("CNTV_CVAL_EL0 set status name: not attempted"));
        assert!(output.contains("CNTV_CTL_EL0 set status name: not attempted"));
        assert!(output
            .contains("Low vector diagnostic page resume ELR_EL1 set status name: not attempted"));
        assert!(output
            .contains("Low vector diagnostic page resume SPSR_EL1 set status name: not attempted"));
        assert!(output
            .contains("Low vector diagnostic page resume CPSR set status name: not attempted"));
        assert!(
            output.contains("Low vector diagnostic page resume PC set status name: not attempted")
        );
        assert!(output.contains("Low vector diagnostic page original slot bytes: not observed"));
        assert!(
            output.contains("Low vector diagnostic page original sync instruction: not observed")
        );
        assert!(output.contains("Low vector diagnostic page original sync hint: not observed"));
        assert!(output.contains(
            "Low vector diagnostic page resume target instruction before ERET: not observed"
        ));
        assert!(output
            .contains("Low vector diagnostic page resume target hint before ERET: not observed"));
        assert!(output.contains(
            "Low vector diagnostic page resume target stage-1 descriptor before ERET: not observed"
        ));
        assert!(output.contains(
            "Low vector diagnostic page resume target stage-1 kind before ERET: not observed"
        ));
        assert!(output.contains(
            "Low vector diagnostic page resume target is installed diagnostic HVC before ERET: false"
        ));
        assert!(output.contains("VTimer initial unmask status name: not attempted"));
        assert!(output.contains("Last pending IRQ set status name: not attempted"));
        assert!(output.contains("Last device IRQ line assert status name: not attempted"));
        assert!(output.contains("Last device IRQ line deassert status name: not attempted"));
        assert!(output.contains("Last VTimer unmask status name: not attempted"));
        assert!(output.contains("Run-loop exits:"));
        assert!(output.contains("- none"));
        assert!(output.contains("BRIDGEVM_HVF_ALLOW_UEFI_FIRMWARE_RUN_LOOP"));
        assert!(!output.contains("qemu-system"));
        assert!(!output.contains('%'));
    }

    #[test]
    fn windows_11_arm_uefi_firmware_device_discovery_probe_defaults_to_not_reached() {
        let stem = format!(
            "bridgevm-windows-arm-uefi-firmware-device-discovery-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let firmware_path = std::env::temp_dir().join(format!("{stem}-code.fd"));
        let template_path = std::env::temp_dir().join(format!("{stem}-vars-template.fd"));
        let vars_path = std::env::temp_dir().join(format!("{stem}-vars.fd"));
        let installer_iso_path = std::env::temp_dir().join(format!("{stem}-win11-arm.iso"));
        let writable_target_disk_path = std::env::temp_dir().join(format!("{stem}-windows.raw"));
        std::fs::write(&firmware_path, test_uefi_fv_bytes(128 * 1024)).unwrap();
        std::fs::write(&template_path, test_uefi_fv_bytes(64 * 1024)).unwrap();
        let _ = std::fs::remove_file(&vars_path);

        let probe = probe_windows_11_arm_uefi_firmware_device_discovery(
            WindowsArmUefiFirmwareRunLoopOptions {
                pflash: WindowsArmUefiPflashMapOptions {
                    firmware_path: firmware_path.clone(),
                    vars_template_path: Some(template_path.clone()),
                    vars_path: Some(vars_path.clone()),
                    create_vars: true,
                },
                execution: WindowsArmUefiFirmwareRunLoopExecutionOptions {
                    allow_loop: false,
                    requested_exits: 8,
                    guest_ram_mib: 64,
                    watchdog_timeout_ms: 100,
                    map_low_pflash_alias: false,
                    seed_diagnostic_vector: false,
                    seed_guest_ram_diagnostic_vector: false,
                    seed_executable_diagnostic_vector: false,
                    try_recommended_vector_base_vbar: false,
                    continue_after_recommended_vector_base_vbar: false,
                    repair_low_vector_diagnostic_page: false,
                    remap_low_vector_to_recommended_vector: false,
                    continue_after_low_vector_repair: false,
                    restore_low_vector_slot_before_eret: false,
                    wire_interrupt_timer: false,
                    stop_at_first_post_repair_device_boundary: false,
                    installer_iso_path: Some(installer_iso_path.clone()),
                    writable_target_disk_path: Some(writable_target_disk_path.clone()),
                },
            },
        );
        let output = probe.render_text();
        let _ = std::fs::remove_file(&firmware_path);
        let _ = std::fs::remove_file(&template_path);
        let _ = std::fs::remove_file(&vars_path);

        assert!(!probe.device_boundary_reached());
        assert_eq!(probe.boundary_status(), "not reached");
        assert!(!probe.device_discovery_ready());
        assert!(!probe.run_loop.allowed);
        assert!(!probe.run_loop.attempted);
        assert!(probe.run_loop.low_pflash_alias_requested);
        assert!(probe.run_loop.low_vector_diagnostic_page_repair_requested);
        assert!(probe.run_loop.low_vector_post_repair_continue_requested);
        assert!(probe.run_loop.interrupt_timer_wiring_requested);
        assert!(
            probe
                .run_loop
                .stop_at_first_post_repair_device_boundary_requested
        );
        assert_eq!(probe.run_loop.handled_mmio_read_count, 0);
        assert_eq!(probe.run_loop.handled_mmio_write_count, 0);
        assert_eq!(probe.run_loop.handled_icc_read_count, 0);
        assert_eq!(probe.run_loop.handled_icc_write_count, 0);
        assert!(output.contains("Windows 11 Arm HVF UEFI firmware device-discovery probe"));
        assert!(output.contains("QEMU: not used"));
        assert!(output.contains("Apple VZ: not used"));
        assert!(output.contains("Windows boot: not claimed"));
        assert!(output.contains("Underlying probe: windows-firmware-run-loop-probe"));
        assert!(output.contains("Device discovery boundary reached: false"));
        assert!(output.contains("Device discovery boundary status: not reached"));
        assert!(output.contains("Device discovery ready: false"));
        assert!(output.contains(
            "Device discovery blocker: firmware has not reached a non-diagnostic MMIO/sysreg boundary yet"
        ));
        assert!(output.contains("Handled MMIO access count: 0"));
        assert!(output.contains("Handled ICC access count: 0"));
        assert!(output.contains("Low pflash alias requested: true"));
        assert!(output.contains("Low vector diagnostic page repair requested: true"));
        assert!(output.contains("Continue after low-vector repair requested: true"));
        assert!(output.contains("Interrupt/timer wiring requested: true"));
        assert!(output.contains("Stop at first post-repair device boundary requested: true"));
        assert!(output.contains(&format!(
            "Installer ISO path: {}",
            installer_iso_path.display()
        )));
        assert!(output.contains(&format!(
            "Writable target disk path: {}",
            writable_target_disk_path.display()
        )));
        assert!(output.contains("BRIDGEVM_HVF_ALLOW_UEFI_FIRMWARE_RUN_LOOP"));
        assert!(!output.contains("qemu-system"));
        assert!(!output.contains('%'));
    }

    #[test]
    fn windows_11_arm_uefi_firmware_run_loop_no_live_loop_reports_restore_before_eret_request() {
        let stem = format!(
            "bridgevm-windows-arm-uefi-firmware-run-loop-restore-before-eret-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let firmware_path = std::env::temp_dir().join(format!("{stem}-code.fd"));
        let template_path = std::env::temp_dir().join(format!("{stem}-vars-template.fd"));
        let vars_path = std::env::temp_dir().join(format!("{stem}-vars.fd"));
        std::fs::write(&firmware_path, test_uefi_fv_bytes(128 * 1024)).unwrap();
        std::fs::write(&template_path, test_uefi_fv_bytes(64 * 1024)).unwrap();
        let _ = std::fs::remove_file(&vars_path);

        let probe =
            probe_windows_11_arm_uefi_firmware_run_loop(WindowsArmUefiFirmwareRunLoopOptions {
                pflash: WindowsArmUefiPflashMapOptions {
                    firmware_path: firmware_path.clone(),
                    vars_template_path: Some(template_path.clone()),
                    vars_path: Some(vars_path.clone()),
                    create_vars: true,
                },
                execution: WindowsArmUefiFirmwareRunLoopExecutionOptions {
                    allow_loop: false,
                    requested_exits: 8,
                    guest_ram_mib: 64,
                    watchdog_timeout_ms: 100,
                    map_low_pflash_alias: true,
                    seed_diagnostic_vector: false,
                    seed_guest_ram_diagnostic_vector: false,
                    seed_executable_diagnostic_vector: false,
                    try_recommended_vector_base_vbar: false,
                    continue_after_recommended_vector_base_vbar: false,
                    repair_low_vector_diagnostic_page: true,
                    remap_low_vector_to_recommended_vector: false,
                    continue_after_low_vector_repair: true,
                    restore_low_vector_slot_before_eret: true,
                    wire_interrupt_timer: false,
                    stop_at_first_post_repair_device_boundary: false,
                    installer_iso_path: None,
                    writable_target_disk_path: None,
                },
            });
        let output = probe.render_text();
        let _ = std::fs::remove_file(&firmware_path);
        let _ = std::fs::remove_file(&template_path);
        let _ = std::fs::remove_file(&vars_path);

        assert!(!probe.allowed);
        assert!(!probe.attempted);
        assert!(probe.low_vector_diagnostic_page_repair_requested);
        assert!(probe.low_vector_post_repair_continue_requested);
        assert!(probe.low_vector_diagnostic_page_restore_before_eret_requested);
        assert!(!probe.low_vector_diagnostic_page_restore_before_eret_attempted);
        assert!(!probe.low_vector_diagnostic_page_slot_restored);
        assert!(output.contains("Low vector diagnostic page repair requested: true"));
        assert!(output.contains("Continue after low-vector repair requested: true"));
        assert!(output.contains("Low vector diagnostic page restore before ERET requested: true"));
        assert!(output.contains("Low vector diagnostic page restore before ERET attempted: false"));
        assert!(output.contains("Low vector diagnostic page slot restored: false"));
        assert!(output.contains("BRIDGEVM_HVF_ALLOW_UEFI_FIRMWARE_RUN_LOOP"));
        assert!(!output.contains("qemu-system"));
        assert!(!output.contains('%'));
    }

    #[test]
    fn windows_11_arm_uefi_firmware_run_loop_no_live_loop_reports_executable_vector_request() {
        let stem = format!(
            "bridgevm-windows-arm-uefi-firmware-run-loop-exec-vector-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let firmware_path = std::env::temp_dir().join(format!("{stem}-code.fd"));
        let template_path = std::env::temp_dir().join(format!("{stem}-vars-template.fd"));
        let vars_path = std::env::temp_dir().join(format!("{stem}-vars.fd"));
        std::fs::write(&firmware_path, test_uefi_fv_bytes(128 * 1024)).unwrap();
        std::fs::write(&template_path, test_uefi_fv_bytes(64 * 1024)).unwrap();
        let _ = std::fs::remove_file(&vars_path);

        let probe =
            probe_windows_11_arm_uefi_firmware_run_loop(WindowsArmUefiFirmwareRunLoopOptions {
                pflash: WindowsArmUefiPflashMapOptions {
                    firmware_path: firmware_path.clone(),
                    vars_template_path: Some(template_path.clone()),
                    vars_path: Some(vars_path.clone()),
                    create_vars: true,
                },
                execution: WindowsArmUefiFirmwareRunLoopExecutionOptions {
                    allow_loop: false,
                    requested_exits: 8,
                    guest_ram_mib: 64,
                    watchdog_timeout_ms: 100,
                    map_low_pflash_alias: true,
                    seed_diagnostic_vector: false,
                    seed_guest_ram_diagnostic_vector: false,
                    seed_executable_diagnostic_vector: true,
                    try_recommended_vector_base_vbar: false,
                    continue_after_recommended_vector_base_vbar: false,
                    repair_low_vector_diagnostic_page: false,
                    remap_low_vector_to_recommended_vector: false,
                    continue_after_low_vector_repair: false,
                    restore_low_vector_slot_before_eret: false,
                    wire_interrupt_timer: false,
                    stop_at_first_post_repair_device_boundary: false,
                    installer_iso_path: None,
                    writable_target_disk_path: None,
                },
            });
        let output = probe.render_text();
        let _ = std::fs::remove_file(&firmware_path);
        let _ = std::fs::remove_file(&template_path);
        let _ = std::fs::remove_file(&vars_path);

        assert!(!probe.allowed);
        assert!(!probe.attempted);
        assert!(probe.diagnostic_vector_seed_requested);
        assert!(!probe.diagnostic_vector_populated);
        assert!(probe.low_pflash_alias_requested);
        assert_eq!(
            probe.diagnostic_vector_ipa,
            WINDOWS_ARM_EXECUTABLE_DIAGNOSTIC_VECTOR_IPA
        );
        assert_eq!(
            probe.diagnostic_vector_location,
            "low-pflash-executable-candidate"
        );
        assert!(probe.exits.is_empty());
        assert!(output.contains("Diagnostic vector seed requested: true"));
        assert!(output.contains("Diagnostic vector location: low-pflash-executable-candidate"));
        assert!(output.contains("Diagnostic vector IPA: 0x200000"));
        assert!(output.contains("Low pflash alias requested: true"));
        assert!(output.contains("Observed exits: 0"));
        assert!(output.contains("BRIDGEVM_HVF_ALLOW_UEFI_FIRMWARE_RUN_LOOP"));
        assert!(!output.contains("qemu-system"));
        assert!(!output.contains('%'));
    }

    #[test]
    fn windows_11_arm_uefi_firmware_run_loop_render_records_vtimer_auto_mask() {
        let stem = format!(
            "bridgevm-windows-arm-uefi-firmware-run-loop-vtimer-exit-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let firmware_path = std::env::temp_dir().join(format!("{stem}-code.fd"));
        let template_path = std::env::temp_dir().join(format!("{stem}-vars-template.fd"));
        let vars_path = std::env::temp_dir().join(format!("{stem}-vars.fd"));
        std::fs::write(&firmware_path, test_uefi_fv_bytes(128 * 1024)).unwrap();
        std::fs::write(&template_path, test_uefi_fv_bytes(64 * 1024)).unwrap();
        let _ = std::fs::remove_file(&vars_path);

        let mut probe =
            probe_windows_11_arm_uefi_firmware_run_loop(WindowsArmUefiFirmwareRunLoopOptions {
                pflash: WindowsArmUefiPflashMapOptions {
                    firmware_path: firmware_path.clone(),
                    vars_template_path: Some(template_path.clone()),
                    vars_path: Some(vars_path.clone()),
                    create_vars: true,
                },
                execution: WindowsArmUefiFirmwareRunLoopExecutionOptions {
                    allow_loop: false,
                    requested_exits: 8,
                    guest_ram_mib: 64,
                    watchdog_timeout_ms: 100,
                    map_low_pflash_alias: false,
                    seed_diagnostic_vector: false,
                    seed_guest_ram_diagnostic_vector: false,
                    seed_executable_diagnostic_vector: false,
                    try_recommended_vector_base_vbar: false,
                    continue_after_recommended_vector_base_vbar: false,
                    repair_low_vector_diagnostic_page: false,
                    remap_low_vector_to_recommended_vector: false,
                    continue_after_low_vector_repair: false,
                    restore_low_vector_slot_before_eret: false,
                    wire_interrupt_timer: true,
                    stop_at_first_post_repair_device_boundary: false,
                    installer_iso_path: None,
                    writable_target_disk_path: None,
                },
            });
        let _ = std::fs::remove_file(&firmware_path);
        let _ = std::fs::remove_file(&template_path);
        let _ = std::fs::remove_file(&vars_path);

        probe.vtimer_exit_count = 1;
        probe.pending_irq_injected_count = 1;
        probe.device_irq_injected_count = 1;
        probe.device_irq_cleared_count = 1;
        probe.last_device_irq_set_status = Some(0);
        probe.last_device_irq_clear_status = Some(0);
        probe.exits = vec![WindowsArmUefiFirmwareRunLoopExit {
            index: 1,
            run_status: Some(0),
            exit_reason: Some(2),
            exit_syndrome: None,
            exit_exception_class: None,
            exit_virtual_address: None,
            exit_physical_address: None,
            pc_after_exit_status: Some(0),
            pc_after_exit: Some(WINDOWS_ARM_UEFI_CODE_IPA),
            instruction_word_after_exit: None,
            instruction_hint_after_exit: "not observed",
            pc_stage1_leaf_level_after_exit: None,
            pc_stage1_leaf_descriptor_after_exit: None,
            pc_stage1_leaf_descriptor_kind_after_exit: "not observed",
            pc_stage1_leaf_pxn_after_exit: None,
            pc_stage1_leaf_uxn_after_exit: None,
            stage1_descriptor_samples_after_exit: Vec::new(),
            stage1_walk_entries_after_exit: Vec::new(),
            stage1_executable_candidates_after_exit: Vec::new(),
            x0_after_exit: None,
            x1_after_exit: None,
            x2_after_exit: None,
            x3_after_exit: None,
            x4_after_exit: None,
            cpsr_after_exit: None,
            vbar_el1_after_exit: None,
            elr_el1_after_exit: None,
            esr_el1_after_exit: None,
            far_el1_after_exit: None,
            spsr_el1_after_exit: None,
            sctlr_el1_after_exit: None,
            tcr_el1_after_exit: None,
            ttbr0_el1_after_exit: None,
            ttbr1_el1_after_exit: None,
            mair_el1_after_exit: None,
            sp_el1_after_exit: None,
            watchdog_cancel_status: None,
            vtimer_auto_mask_get_status: Some(0),
            vtimer_auto_mask_after_exit: Some(true),
            vtimer_rearm_cval_value: Some(0x1234),
            vtimer_rearm_cval_set_status: Some(0),
            vtimer_ppi_pending_recorded: Some(true),
            vtimer_irq_line_assertable: Some(true),
            vtimer_gic_group1_enabled: Some(true),
            vtimer_gic_priority_mask: Some(0xff),
            vtimer_gic_running_priority: Some(0xff),
            vtimer_gic_priority_threshold: Some(0xff),
            vtimer_gic_pending_intid: Some(WINDOWS_ARM_VIRTUAL_TIMER_INTERRUPT_ID),
            vtimer_pending_irq_set_status: Some(0),
            vtimer_unmask_status: Some(0),
            handled: true,
        }];

        let output = probe.render_text();

        assert!(output.contains("VTimer exit count: 1"));
        assert!(output.contains("Pending IRQ injected count: 1"));
        assert!(output.contains("Device IRQ line asserted count: 1"));
        assert!(output.contains("Device IRQ line deasserted count: 1"));
        assert!(output.contains("Handled MMIO read count: 0"));
        assert!(output.contains("Handled MMIO write count: 0"));
        assert!(output.contains("VirtIO queue_notify count: 0"));
        assert!(output.contains("VirtIO request completion count: 0"));
        assert!(output.contains("Handled ICC read count: 0"));
        assert!(output.contains("Handled ICC write count: 0"));
        assert!(output.contains("Last device IRQ line assert status name: HV_SUCCESS"));
        assert!(output.contains("Last device IRQ line deassert status name: HV_SUCCESS"));
        assert!(output.contains("CNTV_CVAL_EL0 value: 0x0"));
        assert!(output.contains("CNTV_CTL_EL0 value: 0x1"));
        assert!(output.contains("reason=HV_EXIT_REASON_VTIMER_ACTIVATED"));
        assert!(output.contains("vtimer_auto_mask=true"));
        assert!(output.contains("vtimer_auto_mask_get=HV_SUCCESS"));
        assert!(output.contains("vtimer_rearm_cval=0x1234"));
        assert!(output.contains("vtimer_rearm_cval_set=HV_SUCCESS"));
        assert!(output.contains("vtimer_ppi_pending_recorded=true"));
        assert!(output.contains("vtimer_irq_line_assertable=true"));
        assert!(output.contains("vtimer_gic_group1_enabled=true"));
        assert!(output.contains("vtimer_gic_priority_mask=0xff"));
        assert!(output.contains("vtimer_gic_running_priority=0xff"));
        assert!(output.contains("vtimer_gic_priority_threshold=0xff"));
        assert!(output.contains("vtimer_gic_pending_intid=27"));
        assert!(output.contains("vtimer_pending_irq=HV_SUCCESS"));
        assert!(output.contains("vtimer_unmask=HV_SUCCESS"));
        assert!(output.contains("handled=true"));
        assert!(!output.contains("qemu-system"));
        assert!(!output.contains('%'));
    }

    #[test]
    fn firmware_run_loop_diagnoses_empty_recommended_vector_base_sync_slot() {
        let mut exit = test_firmware_run_loop_exit();
        exit.vbar_el1_after_exit = Some(WINDOWS_ARM_EXECUTABLE_DIAGNOSTIC_VECTOR_IPA);
        exit.pc_after_exit = Some(
            WINDOWS_ARM_EXECUTABLE_DIAGNOSTIC_VECTOR_IPA
                + WINDOWS_ARM_DIAGNOSTIC_VECTOR_CURRENT_EL_SPX_SYNC_OFFSET as u64,
        );
        exit.instruction_word_after_exit = Some(0);
        exit.pc_stage1_leaf_level_after_exit = Some(2);
        exit.pc_stage1_leaf_descriptor_after_exit = Some(0x200f8d);
        exit.pc_stage1_leaf_descriptor_kind_after_exit = "block";
        exit.pc_stage1_leaf_pxn_after_exit = Some(false);
        exit.pc_stage1_leaf_uxn_after_exit = Some(false);

        assert_eq!(
            windows_arm_firmware_run_loop_exit_diagnosis_kind(&exit),
            WindowsArmFirmwareRunLoopDiagnosis::RecommendedVectorBaseEmptySyncSlot
        );
        assert_eq!(
            windows_arm_firmware_run_loop_exit_diagnosis(&exit),
            "recommended-vector-base-empty-sync-slot"
        );
    }

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

    fn test_uefi_fv_bytes(len: usize) -> Vec<u8> {
        assert!(len >= UEFI_FV_MIN_HEADER_BYTES);
        let header_length = 0x48_u16;
        let mut bytes = vec![0_u8; len];
        bytes[16..32].copy_from_slice(&[
            0x8c, 0x8c, 0xf9, 0x61, 0xd2, 0x4b, 0x2c, 0x4f, 0x8a, 0x89, 0x22, 0x4d, 0xaf, 0xdc,
            0xf1, 0x6f,
        ]);
        bytes[UEFI_FV_LENGTH_OFFSET..UEFI_FV_LENGTH_OFFSET + 8]
            .copy_from_slice(&(len as u64).to_le_bytes());
        bytes[UEFI_FV_SIGNATURE_OFFSET..UEFI_FV_SIGNATURE_OFFSET + 4]
            .copy_from_slice(UEFI_FV_SIGNATURE);
        bytes[0x2c..0x30].copy_from_slice(&0x0004_feff_u32.to_le_bytes());
        bytes[UEFI_FV_HEADER_LENGTH_OFFSET..UEFI_FV_HEADER_LENGTH_OFFSET + 2]
            .copy_from_slice(&header_length.to_le_bytes());
        bytes[0x34..0x36].copy_from_slice(&0_u16.to_le_bytes());
        bytes[0x36] = 0;
        bytes[0x37] = 2;
        bytes[0x38..0x3c].copy_from_slice(&1_u32.to_le_bytes());
        bytes[0x3c..0x40].copy_from_slice(&(len as u32).to_le_bytes());
        bytes[0x40..0x44].copy_from_slice(&0_u32.to_le_bytes());
        bytes[0x44..0x48].copy_from_slice(&0_u32.to_le_bytes());
        let checksum = 0_u16.wrapping_sub(uefi_checksum16(&bytes[..usize::from(header_length)]));
        bytes[0x32..0x34].copy_from_slice(&checksum.to_le_bytes());
        bytes
    }

    #[test]
    fn vm_create_probe_is_opt_in_and_qemu_free() {
        let probe = probe_hvf_vm_create(false);
        let output = probe.render_text();

        assert!(!probe.allowed);
        assert!(!probe.attempted);
        assert!(!probe.created);
        assert!(!probe.destroyed);
        assert!(output.contains("HVF VM create/destroy probe"));
        assert!(output.contains("QEMU: not used"));
        assert!(output.contains("Apple VZ: not used"));
        assert!(output.contains("Allowed: false"));
        assert!(output.contains("Attempted: false"));
        assert!(output.contains("Created: false"));
        assert!(output.contains("Destroyed: false"));
        assert!(output.contains("Create status: not attempted"));
        assert!(output.contains("Create status name: not attempted"));
        assert!(output.contains("Destroy status: not attempted"));
        assert!(output.contains("Destroy status name: not attempted"));
        assert!(!output.contains("qemu-system"));
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
    fn vcpu_create_probe_is_opt_in_and_qemu_free() {
        let probe = probe_hvf_vcpu_create(false);
        let output = probe.render_text();

        assert!(!probe.allowed);
        assert!(!probe.attempted);
        assert!(!probe.vm_created);
        assert!(!probe.vcpu_created);
        assert!(!probe.vcpu_destroyed);
        assert!(!probe.vm_destroyed);
        assert!(output.contains("HVF vCPU create/destroy probe"));
        assert!(output.contains("QEMU: not used"));
        assert!(output.contains("Apple VZ: not used"));
        assert!(output.contains("Allowed: false"));
        assert!(output.contains("Attempted: false"));
        assert!(output.contains("VM created: false"));
        assert!(output.contains("vCPU created: false"));
        assert!(output.contains("vCPU destroyed: false"));
        assert!(output.contains("VM destroyed: false"));
        assert!(output.contains("VM create status name: not attempted"));
        assert!(output.contains("vCPU create status name: not attempted"));
        assert!(!output.contains("qemu-system"));
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
    fn vcpu_run_probe_is_opt_in_and_qemu_free() {
        let probe = probe_hvf_vcpu_run(false);
        let output = probe.render_text();

        assert!(!probe.allowed);
        assert!(!probe.attempted);
        assert!(!probe.vm_created);
        assert!(!probe.vcpu_created);
        assert!(!probe.cancel_requested);
        assert!(!probe.run_attempted);
        assert!(!probe.run_boundary_observed);
        assert!(!probe.vcpu_destroyed);
        assert!(!probe.vm_destroyed);
        assert!(output.contains("HVF vCPU run/cancel probe"));
        assert!(output.contains("QEMU: not used"));
        assert!(output.contains("Apple VZ: not used"));
        assert!(output.contains("Guest execution: pre-canceled before entry"));
        assert!(output.contains("Allowed: false"));
        assert!(output.contains("Attempted: false"));
        assert!(output.contains("Cancel status name: not attempted"));
        assert!(output.contains("Run status name: not attempted"));
        assert!(output.contains("Exit reason name: not observed"));
        assert!(!output.contains("qemu-system"));
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
    fn interrupt_timer_probe_is_opt_in_and_qemu_free() {
        let probe = probe_hvf_interrupt_timer(false);
        let output = probe.render_text();

        assert!(!probe.allowed);
        assert!(!probe.attempted);
        assert!(!probe.vm_created);
        assert!(!probe.vcpu_created);
        assert!(!probe.pending_irq_set);
        assert!(!probe.pending_irq_cleared);
        assert!(!probe.vtimer_masked);
        assert!(!probe.vtimer_unmasked);
        assert!(!probe.vtimer_offset_set);
        assert!(!probe.boundary_observed);
        assert!(output.contains("HVF interrupt/timer probe"));
        assert!(output.contains("QEMU: not used"));
        assert!(output.contains("Apple VZ: not used"));
        assert!(output.contains("Guest execution: not entered"));
        assert!(output.contains("Allowed: false"));
        assert!(output.contains("Attempted: false"));
        assert!(output.contains("Pending IRQ after set: not observed"));
        assert!(output.contains("VTimer offset after set: not observed"));
        assert!(output.contains("Interrupt/timer boundary observed: false"));
        assert!(output.contains("IRQ set status name: not attempted"));
        assert!(output.contains("VTimer offset get status name: not attempted"));
        assert!(!output.contains("qemu-system"));
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
    fn vtimer_exit_probe_is_opt_in_and_qemu_free() {
        let probe = probe_hvf_vtimer_exit(false);
        let output = probe.render_text();

        assert!(!probe.allowed);
        assert!(!probe.attempted);
        assert!(!probe.vm_created);
        assert!(!probe.memory_mapped);
        assert!(!probe.vcpu_created);
        assert!(!probe.vtimer_offset_set);
        assert!(!probe.cntv_cval_set);
        assert!(!probe.cntv_ctl_set);
        assert!(!probe.vtimer_unmasked);
        assert!(!probe.run_attempted);
        assert!(!probe.vtimer_exit_observed);
        assert!(!probe.pending_irq_injected);
        assert_eq!(probe.vtimer_mask_observed_after_exit, None);
        assert!(!probe.vtimer_unmasked_after_exit);
        assert!(output.contains("HVF VTimer exit probe"));
        assert!(output.contains("QEMU: not used"));
        assert!(output.contains("Apple VZ: not used"));
        assert!(
            output.contains("Guest execution: WFI wait loop with host-programmed virtual timer")
        );
        assert!(output.contains("Allowed: false"));
        assert!(output.contains("Attempted: false"));
        assert!(output.contains("VTimer exit observed: false"));
        assert!(output.contains("Pending IRQ injected: false"));
        assert!(output.contains("VTimer mask observed after exit: not observed"));
        assert!(output.contains("Instructions: WFI; HVC #0"));
        assert!(output.contains("CNTV_CTL_EL0 requested: 0x1"));
        assert!(output.contains("Run status name: not attempted"));
        assert!(output.contains("Exit reason name: not observed"));
        assert!(output.contains("BRIDGEVM_HVF_ALLOW_VTIMER_EXIT"));
        assert!(!output.contains("qemu-system"));
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
    fn memory_map_probe_is_opt_in_and_qemu_free() {
        let probe = probe_hvf_memory_map(false);
        let output = probe.render_text();

        assert!(!probe.allowed);
        assert!(!probe.attempted);
        assert!(!probe.vm_created);
        assert!(!probe.memory_allocated);
        assert!(!probe.memory_mapped);
        assert!(!probe.memory_unmapped);
        assert!(!probe.memory_deallocated);
        assert!(!probe.vm_destroyed);
        assert!(output.contains("HVF memory map/unmap probe"));
        assert!(output.contains("QEMU: not used"));
        assert!(output.contains("Apple VZ: not used"));
        assert!(output.contains("Guest execution: not entered"));
        assert!(output.contains("Allowed: false"));
        assert!(output.contains("Attempted: false"));
        assert!(output.contains("Guest IPA start: 0x40000000"));
        assert!(output.contains("Bytes: 16384"));
        assert!(output.contains("Map status name: not attempted"));
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
    fn guest_entry_probe_is_opt_in_and_qemu_free() {
        let probe = probe_hvf_guest_entry(false);
        let output = probe.render_text();

        assert!(!probe.allowed);
        assert!(!probe.attempted);
        assert!(!probe.vm_created);
        assert!(!probe.memory_mapped);
        assert!(!probe.vcpu_created);
        assert!(!probe.pc_set);
        assert!(!probe.cpsr_set);
        assert!(!probe.run_attempted);
        assert!(!probe.entry_boundary_observed);
        assert!(output.contains("HVF guest entry probe"));
        assert!(output.contains("QEMU: not used"));
        assert!(output.contains("Apple VZ: not used"));
        assert!(output.contains("Guest execution: one HVC instruction with watchdog"));
        assert!(output.contains("Allowed: false"));
        assert!(output.contains("Attempted: false"));
        assert!(output.contains("Instruction: HVC #0"));
        assert!(output.contains("Run status name: not attempted"));
        assert!(output.contains("Exit reason name: not observed"));
        assert!(!output.contains("qemu-system"));
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
    fn guest_exit_loop_probe_is_opt_in_and_qemu_free() {
        let probe = probe_hvf_guest_exit_loop(false);
        let output = probe.render_text();

        assert!(!probe.allowed);
        assert!(!probe.attempted);
        assert!(!probe.vm_created);
        assert!(!probe.memory_mapped);
        assert!(!probe.vcpu_created);
        assert!(!probe.first_run_attempted);
        assert!(!probe.first_exit_observed);
        assert!(!probe.pc_advanced);
        assert!(!probe.second_run_attempted);
        assert!(!probe.second_exit_observed);
        assert!(!probe.exit_loop_observed);
        assert!(output.contains("HVF guest exit loop probe"));
        assert!(output.contains("QEMU: not used"));
        assert!(output.contains("Apple VZ: not used"));
        assert!(output.contains("Guest execution: two HVC instructions with PC advance watchdog"));
        assert!(output.contains("Allowed: false"));
        assert!(output.contains("Attempted: false"));
        assert!(output.contains("Instructions: HVC #0; HVC #1"));
        assert!(output.contains("First run status name: not attempted"));
        assert!(output.contains("Second run status name: not attempted"));
        assert!(!output.contains("qemu-system"));
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
    fn mmio_read_probe_is_opt_in_and_qemu_free() {
        let probe = probe_hvf_mmio_read_exit(false);
        let output = probe.render_text();

        assert!(!probe.allowed);
        assert!(!probe.attempted);
        assert!(!probe.vm_created);
        assert!(!probe.memory_mapped);
        assert!(!probe.vcpu_created);
        assert!(!probe.address_register_set);
        assert!(!probe.run_attempted);
        assert!(!probe.mmio_exit_observed);
        assert!(output.contains("HVF MMIO read exit probe"));
        assert!(output.contains("QEMU: not used"));
        assert!(output.contains("Apple VZ: not used"));
        assert!(output.contains("Guest execution: one unmapped LDR read with watchdog"));
        assert!(output.contains("Allowed: false"));
        assert!(output.contains("Attempted: false"));
        assert!(output.contains("MMIO IPA: 0x50000000"));
        assert!(output.contains("Instruction: LDR X0, [X1]"));
        assert!(output.contains("Run status name: not attempted"));
        assert!(!output.contains("qemu-system"));
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
    fn mmio_read_emulation_probe_is_opt_in_and_qemu_free() {
        let probe = probe_hvf_mmio_read_emulation(false);
        let output = probe.render_text();

        assert!(!probe.allowed);
        assert!(!probe.attempted);
        assert!(!probe.vm_created);
        assert!(!probe.memory_mapped);
        assert!(!probe.vcpu_created);
        assert!(!probe.first_run_attempted);
        assert!(!probe.mmio_exit_observed);
        assert!(!probe.emulated_value_injected);
        assert!(!probe.pc_advanced);
        assert!(!probe.second_run_attempted);
        assert!(!probe.continuation_exit_observed);
        assert!(!probe.emulated_value_preserved);
        assert!(output.contains("HVF MMIO read emulation probe"));
        assert!(output.contains("QEMU: not used"));
        assert!(output.contains("Apple VZ: not used"));
        assert!(output.contains("Guest execution: unmapped LDR, injected read value, then HVC"));
        assert!(output.contains("Allowed: false"));
        assert!(output.contains("Attempted: false"));
        assert!(output.contains("Instructions: LDR X0, [X1]; HVC #0"));
        assert!(output.contains("Emulated value: 0x123456789abcdef0"));
        assert!(output.contains("First run status name: not attempted"));
        assert!(output.contains("Second run status name: not attempted"));
        assert!(!output.contains("qemu-system"));
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
    fn mmio_write_emulation_probe_is_opt_in_and_qemu_free() {
        let probe = probe_hvf_mmio_write_emulation(false);
        let output = probe.render_text();

        assert!(!probe.allowed);
        assert!(!probe.attempted);
        assert!(!probe.vm_created);
        assert!(!probe.memory_mapped);
        assert!(!probe.vcpu_created);
        assert!(!probe.first_run_attempted);
        assert!(!probe.mmio_exit_observed);
        assert!(!probe.write_value_captured);
        assert!(!probe.pc_advanced);
        assert!(!probe.second_run_attempted);
        assert!(!probe.continuation_exit_observed);
        assert!(!probe.write_value_preserved);
        assert!(output.contains("HVF MMIO write emulation probe"));
        assert!(output.contains("QEMU: not used"));
        assert!(output.contains("Apple VZ: not used"));
        assert!(output.contains("Guest execution: unmapped STR, captured write value, then HVC"));
        assert!(output.contains("Allowed: false"));
        assert!(output.contains("Attempted: false"));
        assert!(output.contains("Instructions: STR X0, [X1]; HVC #0"));
        assert!(output.contains("Write value: 0xfedcba987654321"));
        assert!(output.contains("First run status name: not attempted"));
        assert!(output.contains("Second run status name: not attempted"));
        assert!(!output.contains("qemu-system"));
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

    #[test]
    fn firmware_mmio_data_abort_decoder_handles_aarch64_loads_and_stores() {
        let read = decode_mmio_data_abort(0x93c0_8006).expect("read data abort decodes");
        assert!(!read.is_write);
        assert_eq!(read.access_name(), "read");
        assert_eq!(read.register, 0);
        assert_eq!(read.width, 8);

        let write = decode_mmio_data_abort(0x93c0_8046).expect("write data abort decodes");
        assert!(write.is_write);
        assert_eq!(write.access_name(), "write");
        assert_eq!(write.register, 0);
        assert_eq!(write.width, 8);

        assert_eq!(decode_mmio_data_abort(0x92c0_8006), None);
        assert_eq!(decode_mmio_data_abort(0x93df_8006), None);
    }

    #[test]
    fn firmware_mmio_bus_uses_windows_device_window_layout() {
        let mut bus = windows_arm_firmware_mmio_bus();

        assert_eq!(bus.device_count(), 6);
        assert!(windows_arm_device_mmio_contains(WINDOWS_ARM_PL011_MMIO_IPA));
        assert!(windows_arm_device_mmio_contains(WINDOWS_ARM_PL031_MMIO_IPA));
        assert!(windows_arm_device_mmio_contains(
            WINDOWS_ARM_GIC_DISTRIBUTOR_MMIO_IPA
        ));
        assert!(windows_arm_device_mmio_contains(
            WINDOWS_ARM_GIC_REDISTRIBUTOR_MMIO_IPA
        ));
        assert!(windows_arm_device_mmio_contains(
            WINDOWS_ARM_VIRTIO_INSTALLER_ISO_MMIO_IPA
        ));
        assert!(windows_arm_device_mmio_contains(
            WINDOWS_ARM_VIRTIO_TARGET_DISK_MMIO_IPA
        ));
        assert!(!windows_arm_device_mmio_contains(WINDOWS_ARM_GUEST_RAM_IPA));
        assert_eq!(
            bus.dispatch(MmioAccess::read(
                WINDOWS_ARM_PL011_MMIO_IPA + PL011_FR_OFFSET,
                4
            )),
            MmioAction::ReadValue(WINDOWS_ARM_PL011_FLAG_VALUE)
        );
        assert_eq!(
            bus.dispatch(MmioAccess::write(WINDOWS_ARM_PL011_MMIO_IPA, 0x141, 4)),
            MmioAction::WriteAccepted {
                value: 0x141,
                byte: 0x41
            }
        );
        assert_eq!(
            bus.dispatch(MmioAccess::read(WINDOWS_ARM_PL031_MMIO_IPA, 4)),
            MmioAction::ReadValue(WINDOWS_ARM_PL031_READ_VALUE)
        );
        assert_eq!(
            bus.dispatch(MmioAccess::read(
                WINDOWS_ARM_GIC_DISTRIBUTOR_MMIO_IPA + GICD_TYPER_OFFSET,
                4
            )),
            MmioAction::ReadValue(GICD_TYPER_VALUE)
        );
        assert_eq!(
            bus.dispatch(MmioAccess::read(
                WINDOWS_ARM_GIC_DISTRIBUTOR_MMIO_IPA + GICD_IIDR_OFFSET,
                4
            )),
            MmioAction::ReadValue(GICV3_IIDR_VALUE)
        );
        assert_eq!(
            bus.dispatch(MmioAccess::read(
                WINDOWS_ARM_GIC_REDISTRIBUTOR_MMIO_IPA + GICR_TYPER_OFFSET,
                4
            )),
            MmioAction::ReadValue(GICR_TYPER_VALUE & 0xffff_ffff)
        );
        assert_eq!(
            bus.dispatch(MmioAccess::read(
                WINDOWS_ARM_GIC_REDISTRIBUTOR_MMIO_IPA + GICR_IIDR_OFFSET,
                4
            )),
            MmioAction::ReadValue(GICV3_IIDR_VALUE)
        );
        assert_eq!(
            bus.dispatch(MmioAccess::read(
                WINDOWS_ARM_VIRTIO_INSTALLER_ISO_MMIO_IPA + VIRTIO_MMIO_MAGIC_VALUE_OFFSET,
                4
            )),
            MmioAction::ReadValue(VIRTIO_MMIO_MAGIC_VALUE)
        );
        assert_eq!(
            bus.dispatch(MmioAccess::read(
                WINDOWS_ARM_VIRTIO_INSTALLER_ISO_MMIO_IPA + VIRTIO_MMIO_DEVICE_FEATURES_OFFSET,
                4
            )),
            MmioAction::ReadValue(VIRTIO_BLK_F_RO)
        );
        assert_eq!(
            bus.dispatch(MmioAccess::read(
                WINDOWS_ARM_VIRTIO_TARGET_DISK_MMIO_IPA + VIRTIO_MMIO_MAGIC_VALUE_OFFSET,
                4
            )),
            MmioAction::ReadValue(VIRTIO_MMIO_MAGIC_VALUE)
        );
        assert_eq!(
            bus.dispatch(MmioAccess::read(
                WINDOWS_ARM_VIRTIO_TARGET_DISK_MMIO_IPA + VIRTIO_MMIO_DEVICE_FEATURES_OFFSET,
                4
            )),
            MmioAction::ReadValue(VIRTIO_MMIO_BLOCK_DEVICE_FEATURES_VALUE)
        );
    }

    #[test]
    fn gicv3_distributor_mmio_skeleton_tracks_common_firmware_registers() {
        let mut gic = GicV3DistributorDevice::new(WINDOWS_ARM_GIC_DISTRIBUTOR_MMIO_IPA);
        let base = WINDOWS_ARM_GIC_DISTRIBUTOR_MMIO_IPA;

        assert_eq!(
            gic.handle(MmioAccess::read(base + GICD_TYPER_OFFSET, 4)),
            MmioAction::ReadValue(GICD_TYPER_VALUE)
        );
        assert_eq!(
            gic.handle(MmioAccess::read(base + GICD_IIDR_OFFSET, 4)),
            MmioAction::ReadValue(GICV3_IIDR_VALUE)
        );
        assert_eq!(
            gic.handle(MmioAccess::read(base + GICD_STATUSR_OFFSET, 4)),
            MmioAction::ReadValue(0)
        );
        assert_eq!(
            gic.handle(MmioAccess::write(base + GICD_STATUSR_OFFSET, 0xff, 4)),
            MmioAction::WriteAccepted {
                value: 0xff,
                byte: 0xff
            }
        );
        assert_eq!(
            gic.handle(MmioAccess::read(base + GICD_STATUSR_OFFSET, 4)),
            MmioAction::ReadValue(0)
        );
        assert_eq!(
            gic.handle(MmioAccess::write(base + GICD_CTLR_OFFSET, 0x13, 4)),
            MmioAction::WriteAccepted {
                value: 0x13,
                byte: 0x13
            }
        );
        assert_eq!(
            gic.handle(MmioAccess::read(base + GICD_CTLR_OFFSET, 4)),
            MmioAction::ReadValue(0x13)
        );

        let spi_enable_offset = GICD_ISENABLER_BASE_OFFSET + 4;
        let spi_clear_offset = GICD_ICENABLER_BASE_OFFSET + 4;
        let spi_group_offset = GICD_IGROUPR_BASE_OFFSET + 4;
        let spi_group_modifier_offset = GICD_IGRPMODR_BASE_OFFSET + 4;
        assert_eq!(
            gic.handle(MmioAccess::write(base + spi_group_modifier_offset, 0x4, 4)),
            MmioAction::WriteAccepted {
                value: 0x4,
                byte: 0x4
            }
        );
        assert_eq!(
            gic.handle(MmioAccess::read(base + spi_group_modifier_offset, 4)),
            MmioAction::ReadValue(0x4)
        );
        assert_eq!(
            gic.handle(MmioAccess::write(base + spi_enable_offset, 0x9, 4)),
            MmioAction::WriteAccepted {
                value: 0x9,
                byte: 0x9
            }
        );
        assert_eq!(
            gic.handle(MmioAccess::read(base + spi_enable_offset, 4)),
            MmioAction::ReadValue(0x9)
        );
        assert_eq!(
            gic.handle(MmioAccess::write(base + spi_clear_offset, 0x1, 4)),
            MmioAction::WriteAccepted {
                value: 0x1,
                byte: 0x1
            }
        );
        assert_eq!(
            gic.handle(MmioAccess::read(base + spi_enable_offset, 4)),
            MmioAction::ReadValue(0x8)
        );
        assert_eq!(
            GicV3DistributorDevice::spi_interrupt_id(WINDOWS_ARM_VIRTIO_INSTALLER_ISO_SPI),
            Some(34)
        );
        assert_eq!(GicV3DistributorDevice::interrupt_bit(34), Some((1, 0x4)));
        assert!(!gic.spi_irq_line_assertable(WINDOWS_ARM_VIRTIO_INSTALLER_ISO_SPI));
        assert_eq!(
            gic.handle(MmioAccess::write(base + spi_enable_offset, 0x4, 4)),
            MmioAction::WriteAccepted {
                value: 0x4,
                byte: 0x4
            }
        );
        assert_eq!(
            gic.handle(MmioAccess::read(base + spi_enable_offset, 4)),
            MmioAction::ReadValue(0xc)
        );
        assert_eq!(
            gic.set_spi_pending(WINDOWS_ARM_VIRTIO_INSTALLER_ISO_SPI, true),
            Some(())
        );
        assert!(!gic.spi_irq_line_assertable(WINDOWS_ARM_VIRTIO_INSTALLER_ISO_SPI));
        assert_eq!(
            gic.handle(MmioAccess::write(base + spi_group_offset, 0x4, 4)),
            MmioAction::WriteAccepted {
                value: 0x4,
                byte: 0x4
            }
        );
        assert!(gic.spi_irq_line_assertable(WINDOWS_ARM_VIRTIO_INSTALLER_ISO_SPI));
        assert_eq!(
            gic.set_spi_pending(WINDOWS_ARM_VIRTIO_INSTALLER_ISO_SPI, false),
            Some(())
        );
        assert!(!gic.spi_irq_line_assertable(WINDOWS_ARM_VIRTIO_INSTALLER_ISO_SPI));

        let spi_pending_set_offset = GICD_ISPENDR_BASE_OFFSET + 4;
        let spi_pending_clear_offset = GICD_ICPENDR_BASE_OFFSET + 4;
        assert_eq!(
            gic.handle(MmioAccess::write(base + spi_pending_set_offset, 0x2, 4)),
            MmioAction::WriteAccepted {
                value: 0x2,
                byte: 0x2
            }
        );
        assert_eq!(
            gic.handle(MmioAccess::read(base + spi_pending_set_offset, 4)),
            MmioAction::ReadValue(0x2)
        );
        assert_eq!(
            gic.handle(MmioAccess::write(base + spi_pending_clear_offset, 0x2, 4)),
            MmioAction::WriteAccepted {
                value: 0x2,
                byte: 0x2
            }
        );
        assert_eq!(
            gic.handle(MmioAccess::read(base + spi_pending_set_offset, 4)),
            MmioAction::ReadValue(0)
        );

        let priority_byte_offset = GICD_IPRIORITYR_BASE_OFFSET + 35;
        assert_eq!(
            gic.handle(MmioAccess::write(base + priority_byte_offset, 0x44, 1)),
            MmioAction::WriteAccepted {
                value: 0x44,
                byte: 0x44
            }
        );
        assert_eq!(
            gic.handle(MmioAccess::read(base + priority_byte_offset, 1)),
            MmioAction::ReadValue(0x44)
        );
        assert_eq!(
            gic.handle(MmioAccess::read(base + GICD_IPRIORITYR_BASE_OFFSET + 32, 4)),
            MmioAction::ReadValue(0x44a0_a0a0)
        );

        let router32 = GICD_IROUTER_BASE_OFFSET + (32 * 8);
        assert_eq!(
            gic.handle(MmioAccess::write(base + router32, 0x1122_3344, 4)),
            MmioAction::WriteAccepted {
                value: 0x1122_3344,
                byte: 0x44
            }
        );
        assert_eq!(
            gic.handle(MmioAccess::write(base + router32 + 4, 0x5566_7788, 4)),
            MmioAction::WriteAccepted {
                value: 0x5566_7788,
                byte: 0x88
            }
        );
        assert_eq!(
            gic.handle(MmioAccess::read(base + router32, 4)),
            MmioAction::ReadValue(0x1122_3344)
        );
        assert_eq!(
            gic.handle(MmioAccess::read(base + router32 + 4, 4)),
            MmioAction::ReadValue(0x5566_7788)
        );
    }

    #[test]
    fn gicv3_distributor_selects_pending_spi_by_priority_not_lowest_intid() {
        let mut gic = GicV3DistributorDevice::new(WINDOWS_ARM_GIC_DISTRIBUTOR_MMIO_IPA);
        let base = WINDOWS_ARM_GIC_DISTRIBUTOR_MMIO_IPA;

        assert_eq!(
            gic.handle(MmioAccess::write(
                base + GICD_CTLR_OFFSET,
                u64::from(GICD_CTLR_ENABLE_GRP1NS),
                4,
            )),
            MmioAction::WriteAccepted {
                value: u64::from(GICD_CTLR_ENABLE_GRP1NS),
                byte: GICD_CTLR_ENABLE_GRP1NS as u8,
            }
        );
        for register_base in [
            GICD_IGROUPR_BASE_OFFSET,
            GICD_ISENABLER_BASE_OFFSET,
            GICD_ISPENDR_BASE_OFFSET,
        ] {
            assert_eq!(
                gic.handle(MmioAccess::write(base + register_base + 4, 0xc, 4)),
                MmioAction::WriteAccepted {
                    value: 0xc,
                    byte: 0xc,
                }
            );
        }
        assert_eq!(
            gic.handle(MmioAccess::write(
                base + GICD_IPRIORITYR_BASE_OFFSET + 34,
                0xa0,
                1,
            )),
            MmioAction::WriteAccepted {
                value: 0xa0,
                byte: 0xa0,
            }
        );
        assert_eq!(
            gic.handle(MmioAccess::write(
                base + GICD_IPRIORITYR_BASE_OFFSET + 35,
                0x20,
                1,
            )),
            MmioAction::WriteAccepted {
                value: 0x20,
                byte: 0x20,
            }
        );

        assert_eq!(gic.pending_interrupt_id_for_cpu(0xff), Some(35));
        assert_eq!(gic.acknowledge_pending_interrupt(0xff), 35);
        assert_eq!(
            gic.handle(MmioAccess::read(base + GICD_ISACTIVER_BASE_OFFSET + 4, 4)),
            MmioAction::ReadValue(0x8)
        );
        assert_eq!(gic.pending_interrupt_id_for_cpu(0xff), Some(34));
    }

    #[test]
    fn gicv3_redistributor_mmio_skeleton_tracks_waker_and_ppi_state() {
        let mut gicr = GicV3RedistributorDevice::new(WINDOWS_ARM_GIC_REDISTRIBUTOR_MMIO_IPA);
        let base = WINDOWS_ARM_GIC_REDISTRIBUTOR_MMIO_IPA;

        assert_eq!(
            gicr.handle(MmioAccess::read(base + GICR_TYPER_OFFSET, 4)),
            MmioAction::ReadValue(GICR_TYPER_VALUE & 0xffff_ffff)
        );
        assert_eq!(
            gicr.handle(MmioAccess::read(base + GICR_IIDR_OFFSET, 4)),
            MmioAction::ReadValue(GICV3_IIDR_VALUE)
        );
        assert_eq!(
            gicr.handle(MmioAccess::read(base + GICR_STATUSR_OFFSET, 4)),
            MmioAction::ReadValue(0)
        );
        assert_eq!(
            gicr.handle(MmioAccess::write(base + GICR_STATUSR_OFFSET, 0x80, 4)),
            MmioAction::WriteAccepted {
                value: 0x80,
                byte: 0x80
            }
        );
        assert_eq!(
            gicr.handle(MmioAccess::read(base + GICR_WAKER_OFFSET, 4)),
            MmioAction::ReadValue(0)
        );
        assert_eq!(
            gicr.handle(MmioAccess::write(
                base + GICR_WAKER_OFFSET,
                GICR_WAKER_PROCESSOR_SLEEP,
                4
            )),
            MmioAction::WriteAccepted {
                value: GICR_WAKER_PROCESSOR_SLEEP,
                byte: GICR_WAKER_PROCESSOR_SLEEP as u8
            }
        );
        assert_eq!(
            gicr.handle(MmioAccess::read(base + GICR_WAKER_OFFSET, 4)),
            MmioAction::ReadValue(GICR_WAKER_PROCESSOR_SLEEP | GICR_WAKER_CHILDREN_ASLEEP)
        );
        assert_eq!(
            gicr.handle(MmioAccess::write(base + GICR_WAKER_OFFSET, 0, 4)),
            MmioAction::WriteAccepted { value: 0, byte: 0 }
        );
        assert_eq!(
            gicr.handle(MmioAccess::read(base + GICR_WAKER_OFFSET, 4)),
            MmioAction::ReadValue(0)
        );

        assert_eq!(
            gicr.handle(MmioAccess::write(
                base + GICR_SGI_ISENABLER0_OFFSET,
                1 << 13,
                4
            )),
            MmioAction::WriteAccepted {
                value: 1 << 13,
                byte: 0
            }
        );
        assert_eq!(
            gicr.handle(MmioAccess::read(base + GICR_SGI_ISENABLER0_OFFSET, 4)),
            MmioAction::ReadValue(1 << 13)
        );
        assert_eq!(
            gicr.handle(MmioAccess::write(
                base + GICR_SGI_ICENABLER0_OFFSET,
                1 << 13,
                4
            )),
            MmioAction::WriteAccepted {
                value: 1 << 13,
                byte: 0
            }
        );
        assert_eq!(
            gicr.handle(MmioAccess::read(base + GICR_SGI_ISENABLER0_OFFSET, 4)),
            MmioAction::ReadValue(0)
        );
        assert_eq!(
            gicr.handle(MmioAccess::write(
                base + GICR_SGI_IGRPMODR0_OFFSET,
                1 << 13,
                4
            )),
            MmioAction::WriteAccepted {
                value: 1 << 13,
                byte: 0
            }
        );
        assert_eq!(
            gicr.handle(MmioAccess::read(base + GICR_SGI_IGRPMODR0_OFFSET, 4)),
            MmioAction::ReadValue(1 << 13)
        );

        let priority_byte_offset = GICR_SGI_IPRIORITYR_BASE_OFFSET + 13;
        assert_eq!(
            gicr.handle(MmioAccess::write(base + priority_byte_offset, 0x55, 1)),
            MmioAction::WriteAccepted {
                value: 0x55,
                byte: 0x55
            }
        );
        assert_eq!(
            gicr.handle(MmioAccess::read(base + priority_byte_offset, 1)),
            MmioAction::ReadValue(0x55)
        );
        assert_eq!(
            gicr.handle(MmioAccess::read(
                base + GICR_SGI_IPRIORITYR_BASE_OFFSET + 12,
                4
            )),
            MmioAction::ReadValue(0xa0a0_55a0)
        );

        assert_eq!(
            gicr.handle(MmioAccess::write(
                base + GICR_PROPBASER_OFFSET,
                0x4000_0000,
                4
            )),
            MmioAction::WriteAccepted {
                value: 0x4000_0000,
                byte: 0
            }
        );
        assert_eq!(
            gicr.handle(MmioAccess::read(base + GICR_PROPBASER_OFFSET, 4)),
            MmioAction::ReadValue(0x4000_0000)
        );
    }

    #[test]
    fn gicv3_redistributor_tracks_virtual_timer_ppi_delivery_state() {
        let mut gicr = GicV3RedistributorDevice::new(WINDOWS_ARM_GIC_REDISTRIBUTOR_MMIO_IPA);
        let base = WINDOWS_ARM_GIC_REDISTRIBUTOR_MMIO_IPA;
        let timer_bit = 1_u32 << WINDOWS_ARM_VIRTUAL_TIMER_INTERRUPT_ID;
        let priority_byte_offset =
            GICR_SGI_IPRIORITYR_BASE_OFFSET + u64::from(WINDOWS_ARM_VIRTUAL_TIMER_INTERRUPT_ID);

        assert_eq!(WINDOWS_ARM_VIRTUAL_TIMER_INTERRUPT_ID, 27);
        assert_eq!(
            gicr.handle(MmioAccess::write(
                base + GICR_SGI_ISENABLER0_OFFSET,
                u64::from(timer_bit),
                4,
            )),
            MmioAction::WriteAccepted {
                value: u64::from(timer_bit),
                byte: 0,
            }
        );
        assert_eq!(
            gicr.handle(MmioAccess::write(
                base + GICR_SGI_ISPENDR0_OFFSET,
                u64::from(timer_bit),
                4,
            )),
            MmioAction::WriteAccepted {
                value: u64::from(timer_bit),
                byte: 0,
            }
        );
        assert_eq!(
            gicr.handle(MmioAccess::read(base + GICR_SGI_ISPENDR0_OFFSET, 4)),
            MmioAction::ReadValue(u64::from(timer_bit))
        );
        assert_eq!(
            gicr.handle(MmioAccess::write(base + priority_byte_offset, 0x40, 1)),
            MmioAction::WriteAccepted {
                value: 0x40,
                byte: 0x40,
            }
        );
        assert_eq!(
            gicr.handle(MmioAccess::read(base + priority_byte_offset, 1)),
            MmioAction::ReadValue(0x40)
        );
        assert_eq!(
            gicr.acknowledge_pending_interrupt(0xff),
            GICV3_SPURIOUS_INTERRUPT_ID
        );
        assert_eq!(
            gicr.handle(MmioAccess::write(
                base + GICR_SGI_IGROUPR0_OFFSET,
                u64::from(timer_bit),
                4,
            )),
            MmioAction::WriteAccepted {
                value: u64::from(timer_bit),
                byte: 0,
            }
        );
        assert_eq!(gicr.acknowledge_pending_interrupt(0xff), 27);
        assert_eq!(
            gicr.handle(MmioAccess::read(base + GICR_SGI_ISPENDR0_OFFSET, 4)),
            MmioAction::ReadValue(0)
        );
        assert_eq!(
            gicr.handle(MmioAccess::read(base + GICR_SGI_ISACTIVER0_OFFSET, 4)),
            MmioAction::ReadValue(u64::from(timer_bit))
        );
        assert!(gicr.end_interrupt(WINDOWS_ARM_VIRTUAL_TIMER_INTERRUPT_ID));
        assert_eq!(
            gicr.handle(MmioAccess::read(base + GICR_SGI_ISACTIVER0_OFFSET, 4)),
            MmioAction::ReadValue(0)
        );
        assert!(gicr.set_fdt_ppi_pending(WINDOWS_ARM_VIRTUAL_TIMER_PPI, true));
        assert_eq!(
            gicr.handle(MmioAccess::write(
                base + GICR_SGI_ICPENDR0_OFFSET,
                u64::from(timer_bit),
                4,
            )),
            MmioAction::WriteAccepted {
                value: u64::from(timer_bit),
                byte: 0,
            }
        );
        assert_eq!(
            gicr.handle(MmioAccess::read(base + GICR_SGI_ISPENDR0_OFFSET, 4)),
            MmioAction::ReadValue(0)
        );
    }

    #[test]
    fn gicv3_redistributor_selects_pending_ppi_by_priority_not_lowest_intid() {
        let mut gicr = GicV3RedistributorDevice::new(WINDOWS_ARM_GIC_REDISTRIBUTOR_MMIO_IPA);
        let base = WINDOWS_ARM_GIC_REDISTRIBUTOR_MMIO_IPA;
        let timer_bit = 1_u32 << WINDOWS_ARM_VIRTUAL_TIMER_INTERRUPT_ID;
        let other_ppi_interrupt_id = WINDOWS_ARM_VIRTUAL_TIMER_INTERRUPT_ID + 1;
        let other_ppi_bit = 1_u32 << other_ppi_interrupt_id;
        let both_bits = timer_bit | other_ppi_bit;

        assert_eq!(
            gicr.handle(MmioAccess::write(
                base + GICR_SGI_IGROUPR0_OFFSET,
                u64::from(both_bits),
                4,
            )),
            MmioAction::WriteAccepted {
                value: u64::from(both_bits),
                byte: 0,
            }
        );
        assert_eq!(
            gicr.handle(MmioAccess::write(
                base + GICR_SGI_ISENABLER0_OFFSET,
                u64::from(both_bits),
                4,
            )),
            MmioAction::WriteAccepted {
                value: u64::from(both_bits),
                byte: 0,
            }
        );
        assert_eq!(
            gicr.handle(MmioAccess::write(
                base + GICR_SGI_ISPENDR0_OFFSET,
                u64::from(both_bits),
                4,
            )),
            MmioAction::WriteAccepted {
                value: u64::from(both_bits),
                byte: 0,
            }
        );
        assert_eq!(
            gicr.handle(MmioAccess::write(
                base + GICR_SGI_IPRIORITYR_BASE_OFFSET
                    + u64::from(WINDOWS_ARM_VIRTUAL_TIMER_INTERRUPT_ID),
                0xa0,
                1,
            )),
            MmioAction::WriteAccepted {
                value: 0xa0,
                byte: 0xa0,
            }
        );
        assert_eq!(
            gicr.handle(MmioAccess::write(
                base + GICR_SGI_IPRIORITYR_BASE_OFFSET + u64::from(other_ppi_interrupt_id),
                0x20,
                1,
            )),
            MmioAction::WriteAccepted {
                value: 0x20,
                byte: 0x20,
            }
        );

        assert_eq!(
            gicr.pending_interrupt_id_for_cpu(0xff),
            Some(other_ppi_interrupt_id)
        );
        assert_eq!(
            gicr.acknowledge_pending_interrupt(0xff),
            other_ppi_interrupt_id
        );
        assert_eq!(
            gicr.handle(MmioAccess::read(base + GICR_SGI_ISACTIVER0_OFFSET, 4)),
            MmioAction::ReadValue(u64::from(other_ppi_bit))
        );
        assert_eq!(
            gicr.pending_interrupt_id_for_cpu(0xff),
            Some(WINDOWS_ARM_VIRTUAL_TIMER_INTERRUPT_ID)
        );
    }

    fn configure_virtio_block_queue_on_bus(bus: &mut MmioBus, block_base: u64) {
        for (register, offset, value) in [
            (
                "queue_num",
                VIRTIO_MMIO_QUEUE_NUM_OFFSET,
                VIRTIO_MMIO_BLOCK_QUEUE_NUM_VALUE,
            ),
            (
                "queue_desc_low",
                VIRTIO_MMIO_QUEUE_DESC_LOW_OFFSET,
                VIRTIO_MMIO_BLOCK_QUEUE_DESC_ADDRESS & 0xffff_ffff,
            ),
            (
                "queue_desc_high",
                VIRTIO_MMIO_QUEUE_DESC_HIGH_OFFSET,
                VIRTIO_MMIO_BLOCK_QUEUE_DESC_ADDRESS >> 32,
            ),
            (
                "queue_driver_low",
                VIRTIO_MMIO_QUEUE_DRIVER_LOW_OFFSET,
                VIRTIO_MMIO_BLOCK_QUEUE_DRIVER_ADDRESS & 0xffff_ffff,
            ),
            (
                "queue_driver_high",
                VIRTIO_MMIO_QUEUE_DRIVER_HIGH_OFFSET,
                VIRTIO_MMIO_BLOCK_QUEUE_DRIVER_ADDRESS >> 32,
            ),
            (
                "queue_device_low",
                VIRTIO_MMIO_QUEUE_DEVICE_LOW_OFFSET,
                VIRTIO_MMIO_BLOCK_QUEUE_DEVICE_ADDRESS & 0xffff_ffff,
            ),
            (
                "queue_device_high",
                VIRTIO_MMIO_QUEUE_DEVICE_HIGH_OFFSET,
                VIRTIO_MMIO_BLOCK_QUEUE_DEVICE_ADDRESS >> 32,
            ),
            (
                "queue_ready",
                VIRTIO_MMIO_QUEUE_READY_OFFSET,
                VIRTIO_MMIO_BLOCK_QUEUE_READY_VALUE,
            ),
        ] {
            write_virtio_block_mmio_bus(bus, block_base, register, offset, value).unwrap();
        }
    }

    fn sysreg_trap_syndrome(
        is_read: bool,
        register: u8,
        op0: u8,
        op1: u8,
        crn: u8,
        crm: u8,
        op2: u8,
    ) -> u64 {
        (AARCH64_SYSREG_TRAP_EXCEPTION_CLASS << 26)
            | (u64::from(op0) << 20)
            | (u64::from(op2) << 17)
            | (u64::from(op1) << 14)
            | (u64::from(crn) << 10)
            | (u64::from(register) << 5)
            | (u64::from(crm) << 1)
            | u64::from(is_read as u8)
    }

    #[test]
    fn firmware_system_register_trap_decoder_handles_gic_cpu_interface_regs() {
        let iar = decode_system_register_trap(sysreg_trap_syndrome(true, 2, 3, 0, 12, 12, 0))
            .expect("ICC_IAR1_EL1 trap decodes");
        assert!(iar.is_read);
        assert_eq!(iar.access_name(), "read");
        assert_eq!(iar.register, 2);
        assert_eq!(iar.sys_reg, ICC_IAR1_EL1_SYSREG);

        let eoir = decode_system_register_trap(sysreg_trap_syndrome(false, 4, 3, 0, 12, 12, 1))
            .expect("ICC_EOIR1_EL1 trap decodes");
        assert!(!eoir.is_read);
        assert_eq!(eoir.access_name(), "write");
        assert_eq!(eoir.register, 4);
        assert_eq!(eoir.sys_reg, ICC_EOIR1_EL1_SYSREG);

        assert_eq!(aarch64_sys_reg_encoding(3, 0, 4, 6, 0), ICC_PMR_EL1_SYSREG);
        assert_eq!(
            aarch64_sys_reg_encoding(3, 0, 12, 8, 4),
            ICC_AP0R0_EL1_SYSREG
        );
        assert_eq!(
            aarch64_sys_reg_encoding(3, 0, 12, 9, 0),
            ICC_AP1R0_EL1_SYSREG
        );
        assert_eq!(
            aarch64_sys_reg_encoding(3, 0, 12, 11, 3),
            ICC_RPR_EL1_SYSREG
        );
        assert_eq!(
            aarch64_sys_reg_encoding(3, 0, 12, 12, 5),
            ICC_SRE_EL1_SYSREG
        );
        assert_eq!(
            aarch64_sys_reg_encoding(3, 0, 12, 12, 6),
            ICC_IGRPEN0_EL1_SYSREG
        );
        assert_eq!(decode_system_register_trap(0x93c0_8006), None);
    }

    fn gic_cpu_write(
        cpu: &mut GicV3CpuInterfaceState,
        bus: &mut MmioBus,
        sys_reg: u16,
        value: u64,
    ) -> Option<GicV3CpuInterfaceAction> {
        cpu.handle_system_register_access(
            bus,
            DecodedSystemRegisterAccess {
                is_read: false,
                register: 0,
                sys_reg,
                op0: 3,
                op1: 0,
                crn: 0,
                crm: 0,
                op2: 0,
            },
            Some(value),
        )
    }

    fn gic_cpu_read(
        cpu: &mut GicV3CpuInterfaceState,
        bus: &mut MmioBus,
        sys_reg: u16,
    ) -> Option<GicV3CpuInterfaceAction> {
        cpu.handle_system_register_access(
            bus,
            DecodedSystemRegisterAccess {
                is_read: true,
                register: 1,
                sys_reg,
                op0: 3,
                op1: 0,
                crn: 0,
                crm: 0,
                op2: 0,
            },
            None,
        )
    }

    #[test]
    fn gicv3_cpu_interface_accepts_group0_and_active_priority_registers() {
        let block_devices = windows_arm_firmware_block_devices(None, None);
        let mut bus = windows_arm_firmware_mmio_bus_with_block_devices(&block_devices);
        let mut cpu = GicV3CpuInterfaceState::new();

        assert_eq!(
            gic_cpu_read(&mut cpu, &mut bus, ICC_RPR_EL1_SYSREG),
            Some(GicV3CpuInterfaceAction::Read(0xff))
        );
        assert_eq!(
            gic_cpu_write(&mut cpu, &mut bus, ICC_BPR0_EL1_SYSREG, 0x9),
            Some(GicV3CpuInterfaceAction::Write {
                refresh_level_sources: false,
            })
        );
        assert_eq!(
            gic_cpu_read(&mut cpu, &mut bus, ICC_BPR0_EL1_SYSREG),
            Some(GicV3CpuInterfaceAction::Read(1))
        );
        assert_eq!(
            gic_cpu_write(&mut cpu, &mut bus, ICC_IGRPEN0_EL1_SYSREG, 1),
            Some(GicV3CpuInterfaceAction::Write {
                refresh_level_sources: false,
            })
        );
        assert_eq!(
            gic_cpu_read(&mut cpu, &mut bus, ICC_IGRPEN0_EL1_SYSREG),
            Some(GicV3CpuInterfaceAction::Read(1))
        );
        assert_eq!(
            gic_cpu_read(&mut cpu, &mut bus, ICC_HPPIR0_EL1_SYSREG),
            Some(GicV3CpuInterfaceAction::Read(u64::from(
                GICV3_SPURIOUS_INTERRUPT_ID
            )))
        );
        assert_eq!(
            gic_cpu_read(&mut cpu, &mut bus, ICC_IAR0_EL1_SYSREG),
            Some(GicV3CpuInterfaceAction::Read(u64::from(
                GICV3_SPURIOUS_INTERRUPT_ID
            )))
        );
        assert_eq!(
            gic_cpu_write(&mut cpu, &mut bus, ICC_EOIR0_EL1_SYSREG, 0),
            Some(GicV3CpuInterfaceAction::Write {
                refresh_level_sources: false,
            })
        );
        assert_eq!(
            gic_cpu_write(&mut cpu, &mut bus, ICC_AP0R0_EL1_SYSREG, 0x1234),
            Some(GicV3CpuInterfaceAction::Write {
                refresh_level_sources: false,
            })
        );
        assert_eq!(
            gic_cpu_read(&mut cpu, &mut bus, ICC_AP0R0_EL1_SYSREG),
            Some(GicV3CpuInterfaceAction::Read(0x1234))
        );
        assert_eq!(
            gic_cpu_write(&mut cpu, &mut bus, ICC_AP1R0_EL1_SYSREG, 0x5678),
            Some(GicV3CpuInterfaceAction::Write {
                refresh_level_sources: false,
            })
        );
        assert_eq!(
            gic_cpu_read(&mut cpu, &mut bus, ICC_AP1R0_EL1_SYSREG),
            Some(GicV3CpuInterfaceAction::Read(0x5678))
        );
        assert_eq!(
            gic_cpu_write(&mut cpu, &mut bus, ICC_SGI1R_EL1_SYSREG, 0),
            Some(GicV3CpuInterfaceAction::Write {
                refresh_level_sources: false,
            })
        );
    }

    #[test]
    fn gicv3_cpu_interface_acknowledges_and_eois_pending_device_spis() {
        let block_devices = windows_arm_firmware_block_devices(None, None);
        let mut bus = windows_arm_firmware_mmio_bus_with_block_devices(&block_devices);
        let mut cpu = GicV3CpuInterfaceState::new();
        let base = WINDOWS_ARM_GIC_DISTRIBUTOR_MMIO_IPA;

        assert_eq!(
            bus.dispatch(MmioAccess::write(
                base + GICD_CTLR_OFFSET,
                u64::from(GICD_CTLR_ENABLE_GRP1NS),
                4,
            )),
            MmioAction::WriteAccepted {
                value: u64::from(GICD_CTLR_ENABLE_GRP1NS),
                byte: GICD_CTLR_ENABLE_GRP1NS as u8,
            }
        );
        assert_eq!(
            bus.dispatch(MmioAccess::write(
                base + GICD_ISENABLER_BASE_OFFSET + 4,
                0x4,
                4,
            )),
            MmioAction::WriteAccepted {
                value: 0x4,
                byte: 0x4,
            }
        );
        assert_eq!(
            bus.dispatch(MmioAccess::write(
                base + GICD_ISPENDR_BASE_OFFSET + 4,
                0x4,
                4,
            )),
            MmioAction::WriteAccepted {
                value: 0x4,
                byte: 0x4,
            }
        );
        assert!(!cpu.irq_line_should_assert(&mut bus));
        assert_eq!(
            cpu.handle_system_register_access(
                &mut bus,
                DecodedSystemRegisterAccess {
                    is_read: false,
                    register: 0,
                    sys_reg: ICC_IGRPEN1_EL1_SYSREG,
                    op0: 3,
                    op1: 0,
                    crn: 12,
                    crm: 12,
                    op2: 7,
                },
                Some(1),
            ),
            Some(GicV3CpuInterfaceAction::Write {
                refresh_level_sources: false,
            })
        );
        assert!(!cpu.irq_line_should_assert(&mut bus));
        assert_eq!(
            bus.dispatch(MmioAccess::write(
                base + GICD_IGROUPR_BASE_OFFSET + 4,
                0x4,
                4,
            )),
            MmioAction::WriteAccepted {
                value: 0x4,
                byte: 0x4,
            }
        );
        assert!(cpu.irq_line_should_assert(&mut bus));
        assert_eq!(
            cpu.handle_system_register_access(
                &mut bus,
                DecodedSystemRegisterAccess {
                    is_read: false,
                    register: 0,
                    sys_reg: ICC_PMR_EL1_SYSREG,
                    op0: 3,
                    op1: 0,
                    crn: 4,
                    crm: 6,
                    op2: 0,
                },
                Some(0xa0),
            ),
            Some(GicV3CpuInterfaceAction::Write {
                refresh_level_sources: false,
            })
        );
        assert!(!cpu.irq_line_should_assert(&mut bus));
        assert_eq!(
            cpu.handle_system_register_access(
                &mut bus,
                DecodedSystemRegisterAccess {
                    is_read: false,
                    register: 0,
                    sys_reg: ICC_PMR_EL1_SYSREG,
                    op0: 3,
                    op1: 0,
                    crn: 4,
                    crm: 6,
                    op2: 0,
                },
                Some(0xff),
            ),
            Some(GicV3CpuInterfaceAction::Write {
                refresh_level_sources: false,
            })
        );
        assert!(cpu.irq_line_should_assert(&mut bus));
        assert_eq!(
            cpu.handle_system_register_access(
                &mut bus,
                DecodedSystemRegisterAccess {
                    is_read: true,
                    register: 1,
                    sys_reg: ICC_HPPIR1_EL1_SYSREG,
                    op0: 3,
                    op1: 0,
                    crn: 12,
                    crm: 12,
                    op2: 2,
                },
                None,
            ),
            Some(GicV3CpuInterfaceAction::Read(34))
        );
        assert_eq!(
            gic_cpu_read(&mut cpu, &mut bus, ICC_RPR_EL1_SYSREG),
            Some(GicV3CpuInterfaceAction::Read(0xff))
        );
        assert_eq!(
            cpu.handle_system_register_access(
                &mut bus,
                DecodedSystemRegisterAccess {
                    is_read: true,
                    register: 1,
                    sys_reg: ICC_IAR1_EL1_SYSREG,
                    op0: 3,
                    op1: 0,
                    crn: 12,
                    crm: 12,
                    op2: 0,
                },
                None,
            ),
            Some(GicV3CpuInterfaceAction::Read(34))
        );
        assert_eq!(
            gic_cpu_read(&mut cpu, &mut bus, ICC_RPR_EL1_SYSREG),
            Some(GicV3CpuInterfaceAction::Read(0xa0))
        );
        assert!(!cpu.irq_line_should_assert(&mut bus));
        assert_eq!(
            bus.dispatch(MmioAccess::read(base + GICD_ISPENDR_BASE_OFFSET + 4, 4)),
            MmioAction::ReadValue(0)
        );
        assert_eq!(
            bus.dispatch(MmioAccess::read(base + GICD_ISACTIVER_BASE_OFFSET + 4, 4)),
            MmioAction::ReadValue(0x4)
        );
        assert_eq!(
            cpu.handle_system_register_access(
                &mut bus,
                DecodedSystemRegisterAccess {
                    is_read: false,
                    register: 1,
                    sys_reg: ICC_EOIR1_EL1_SYSREG,
                    op0: 3,
                    op1: 0,
                    crn: 12,
                    crm: 12,
                    op2: 1,
                },
                Some(34),
            ),
            Some(GicV3CpuInterfaceAction::Write {
                refresh_level_sources: true,
            })
        );
        assert_eq!(
            bus.dispatch(MmioAccess::read(base + GICD_ISACTIVER_BASE_OFFSET + 4, 4)),
            MmioAction::ReadValue(0)
        );
        assert_eq!(
            gic_cpu_read(&mut cpu, &mut bus, ICC_RPR_EL1_SYSREG),
            Some(GicV3CpuInterfaceAction::Read(0xff))
        );
    }

    #[test]
    fn gicv3_cpu_interface_acknowledges_and_eois_timer_ppi() {
        let block_devices = windows_arm_firmware_block_devices(None, None);
        let mut bus = windows_arm_firmware_mmio_bus_with_block_devices(&block_devices);
        let mut cpu = GicV3CpuInterfaceState::new();
        let gicr_base = WINDOWS_ARM_GIC_REDISTRIBUTOR_MMIO_IPA;
        let timer_bit = 1_u32 << WINDOWS_ARM_VIRTUAL_TIMER_INTERRUPT_ID;

        assert_eq!(
            bus.dispatch(MmioAccess::write(
                gicr_base + GICR_SGI_ISENABLER0_OFFSET,
                u64::from(timer_bit),
                4,
            )),
            MmioAction::WriteAccepted {
                value: u64::from(timer_bit),
                byte: 0,
            }
        );
        assert!(set_windows_arm_firmware_vtimer_ppi_pending(&mut bus, true));
        assert!(!cpu.irq_line_should_assert(&mut bus));
        assert_eq!(
            gic_cpu_write(&mut cpu, &mut bus, ICC_IGRPEN1_EL1_SYSREG, 1),
            Some(GicV3CpuInterfaceAction::Write {
                refresh_level_sources: false,
            })
        );
        assert!(!cpu.irq_line_should_assert(&mut bus));
        assert_eq!(
            bus.dispatch(MmioAccess::write(
                gicr_base + GICR_SGI_IGROUPR0_OFFSET,
                u64::from(timer_bit),
                4,
            )),
            MmioAction::WriteAccepted {
                value: u64::from(timer_bit),
                byte: 0,
            }
        );
        assert!(cpu.irq_line_should_assert(&mut bus));
        assert_eq!(
            gic_cpu_write(&mut cpu, &mut bus, ICC_PMR_EL1_SYSREG, 0xa0),
            Some(GicV3CpuInterfaceAction::Write {
                refresh_level_sources: false,
            })
        );
        assert!(!cpu.irq_line_should_assert(&mut bus));
        assert_eq!(
            gic_cpu_write(&mut cpu, &mut bus, ICC_PMR_EL1_SYSREG, 0xff),
            Some(GicV3CpuInterfaceAction::Write {
                refresh_level_sources: false,
            })
        );
        assert_eq!(
            gic_cpu_read(&mut cpu, &mut bus, ICC_HPPIR1_EL1_SYSREG),
            Some(GicV3CpuInterfaceAction::Read(u64::from(
                WINDOWS_ARM_VIRTUAL_TIMER_INTERRUPT_ID
            )))
        );
        assert_eq!(
            gic_cpu_read(&mut cpu, &mut bus, ICC_IAR1_EL1_SYSREG),
            Some(GicV3CpuInterfaceAction::Read(u64::from(
                WINDOWS_ARM_VIRTUAL_TIMER_INTERRUPT_ID
            )))
        );
        assert!(!cpu.irq_line_should_assert(&mut bus));
        assert_eq!(
            bus.dispatch(MmioAccess::read(gicr_base + GICR_SGI_ISPENDR0_OFFSET, 4)),
            MmioAction::ReadValue(0)
        );
        assert_eq!(
            bus.dispatch(MmioAccess::read(gicr_base + GICR_SGI_ISACTIVER0_OFFSET, 4)),
            MmioAction::ReadValue(u64::from(timer_bit))
        );
        assert_eq!(
            gic_cpu_write(
                &mut cpu,
                &mut bus,
                ICC_EOIR1_EL1_SYSREG,
                u64::from(WINDOWS_ARM_VIRTUAL_TIMER_INTERRUPT_ID),
            ),
            Some(GicV3CpuInterfaceAction::Write {
                refresh_level_sources: true,
            })
        );
        assert_eq!(
            bus.dispatch(MmioAccess::read(gicr_base + GICR_SGI_ISACTIVER0_OFFSET, 4)),
            MmioAction::ReadValue(0)
        );
    }

    #[test]
    fn gicv3_cpu_interface_irq_line_snapshot_reports_timer_ppi_gates() {
        let block_devices = windows_arm_firmware_block_devices(None, None);
        let mut bus = windows_arm_firmware_mmio_bus_with_block_devices(&block_devices);
        let mut cpu = GicV3CpuInterfaceState::new();
        let gicr_base = WINDOWS_ARM_GIC_REDISTRIBUTOR_MMIO_IPA;
        let timer_bit = 1_u32 << WINDOWS_ARM_VIRTUAL_TIMER_INTERRUPT_ID;

        assert_eq!(
            bus.dispatch(MmioAccess::write(
                gicr_base + GICR_SGI_ISENABLER0_OFFSET,
                u64::from(timer_bit),
                4,
            )),
            MmioAction::WriteAccepted {
                value: u64::from(timer_bit),
                byte: 0,
            }
        );
        assert!(set_windows_arm_firmware_vtimer_ppi_pending(&mut bus, true));

        let snapshot = cpu.irq_line_snapshot(&mut bus);
        assert!(!snapshot.group1_enabled);
        assert_eq!(snapshot.priority_mask, 0xff);
        assert_eq!(snapshot.running_priority, 0xff);
        assert_eq!(snapshot.priority_threshold, 0xff);
        assert_eq!(snapshot.pending_intid, GICV3_SPURIOUS_INTERRUPT_ID);
        assert!(!snapshot.irq_line_should_assert);

        assert_eq!(
            gic_cpu_write(&mut cpu, &mut bus, ICC_IGRPEN1_EL1_SYSREG, 1),
            Some(GicV3CpuInterfaceAction::Write {
                refresh_level_sources: false,
            })
        );
        let snapshot = cpu.irq_line_snapshot(&mut bus);
        assert!(snapshot.group1_enabled);
        assert_eq!(snapshot.pending_intid, GICV3_SPURIOUS_INTERRUPT_ID);
        assert!(!snapshot.irq_line_should_assert);

        assert_eq!(
            bus.dispatch(MmioAccess::write(
                gicr_base + GICR_SGI_IGROUPR0_OFFSET,
                u64::from(timer_bit),
                4,
            )),
            MmioAction::WriteAccepted {
                value: u64::from(timer_bit),
                byte: 0,
            }
        );
        let snapshot = cpu.irq_line_snapshot(&mut bus);
        assert_eq!(
            snapshot.pending_intid,
            WINDOWS_ARM_VIRTUAL_TIMER_INTERRUPT_ID
        );
        assert!(snapshot.irq_line_should_assert);

        assert_eq!(
            gic_cpu_write(&mut cpu, &mut bus, ICC_PMR_EL1_SYSREG, 0xa0),
            Some(GicV3CpuInterfaceAction::Write {
                refresh_level_sources: false,
            })
        );
        let snapshot = cpu.irq_line_snapshot(&mut bus);
        assert_eq!(snapshot.priority_mask, 0xa0);
        assert_eq!(snapshot.priority_threshold, 0xa0);
        assert_eq!(snapshot.pending_intid, GICV3_SPURIOUS_INTERRUPT_ID);
        assert!(!snapshot.irq_line_should_assert);
    }

    #[test]
    fn gicv3_cpu_interface_timer_ppi_does_not_clear_pending_spi_line() {
        let block_devices = windows_arm_firmware_block_devices(None, None);
        let mut bus = windows_arm_firmware_mmio_bus_with_block_devices(&block_devices);
        let mut cpu = GicV3CpuInterfaceState::new();
        let gicd_base = WINDOWS_ARM_GIC_DISTRIBUTOR_MMIO_IPA;
        let gicr_base = WINDOWS_ARM_GIC_REDISTRIBUTOR_MMIO_IPA;
        let timer_bit = 1_u32 << WINDOWS_ARM_VIRTUAL_TIMER_INTERRUPT_ID;

        assert_eq!(
            bus.dispatch(MmioAccess::write(
                gicd_base + GICD_CTLR_OFFSET,
                u64::from(GICD_CTLR_ENABLE_GRP1NS),
                4,
            )),
            MmioAction::WriteAccepted {
                value: u64::from(GICD_CTLR_ENABLE_GRP1NS),
                byte: GICD_CTLR_ENABLE_GRP1NS as u8,
            }
        );
        assert_eq!(
            bus.dispatch(MmioAccess::write(
                gicd_base + GICD_IGROUPR_BASE_OFFSET + 4,
                0x4,
                4,
            )),
            MmioAction::WriteAccepted {
                value: 0x4,
                byte: 0x4,
            }
        );
        assert_eq!(
            bus.dispatch(MmioAccess::write(
                gicd_base + GICD_ISENABLER_BASE_OFFSET + 4,
                0x4,
                4,
            )),
            MmioAction::WriteAccepted {
                value: 0x4,
                byte: 0x4,
            }
        );
        assert_eq!(
            bus.dispatch(MmioAccess::write(
                gicd_base + GICD_ISPENDR_BASE_OFFSET + 4,
                0x4,
                4,
            )),
            MmioAction::WriteAccepted {
                value: 0x4,
                byte: 0x4,
            }
        );
        assert_eq!(
            bus.dispatch(MmioAccess::write(
                gicr_base + GICR_SGI_ISENABLER0_OFFSET,
                u64::from(timer_bit),
                4,
            )),
            MmioAction::WriteAccepted {
                value: u64::from(timer_bit),
                byte: 0,
            }
        );
        assert_eq!(
            bus.dispatch(MmioAccess::write(
                gicr_base + GICR_SGI_IGROUPR0_OFFSET,
                u64::from(timer_bit),
                4,
            )),
            MmioAction::WriteAccepted {
                value: u64::from(timer_bit),
                byte: 0,
            }
        );
        assert!(set_windows_arm_firmware_vtimer_ppi_pending(&mut bus, true));
        assert_eq!(
            gic_cpu_write(&mut cpu, &mut bus, ICC_IGRPEN1_EL1_SYSREG, 1),
            Some(GicV3CpuInterfaceAction::Write {
                refresh_level_sources: false,
            })
        );

        assert_eq!(
            gic_cpu_read(&mut cpu, &mut bus, ICC_IAR1_EL1_SYSREG),
            Some(GicV3CpuInterfaceAction::Read(u64::from(
                WINDOWS_ARM_VIRTUAL_TIMER_INTERRUPT_ID
            )))
        );
        assert!(!cpu.irq_line_should_assert(&mut bus));
        assert_eq!(
            gic_cpu_read(&mut cpu, &mut bus, ICC_HPPIR1_EL1_SYSREG),
            Some(GicV3CpuInterfaceAction::Read(u64::from(
                GICV3_SPURIOUS_INTERRUPT_ID
            )))
        );
        assert_eq!(
            gic_cpu_write(
                &mut cpu,
                &mut bus,
                ICC_EOIR1_EL1_SYSREG,
                u64::from(WINDOWS_ARM_VIRTUAL_TIMER_INTERRUPT_ID),
            ),
            Some(GicV3CpuInterfaceAction::Write {
                refresh_level_sources: true,
            })
        );
        assert!(cpu.irq_line_should_assert(&mut bus));
        assert_eq!(
            gic_cpu_read(&mut cpu, &mut bus, ICC_HPPIR1_EL1_SYSREG),
            Some(GicV3CpuInterfaceAction::Read(34))
        );
        assert_eq!(
            gic_cpu_read(&mut cpu, &mut bus, ICC_IAR1_EL1_SYSREG),
            Some(GicV3CpuInterfaceAction::Read(34))
        );
        assert!(!cpu.irq_line_should_assert(&mut bus));
    }

    #[test]
    fn gicv3_cpu_interface_selects_highest_priority_pending_across_ppi_and_spi() {
        let block_devices = windows_arm_firmware_block_devices(None, None);
        let mut bus = windows_arm_firmware_mmio_bus_with_block_devices(&block_devices);
        let mut cpu = GicV3CpuInterfaceState::new();
        let gicd_base = WINDOWS_ARM_GIC_DISTRIBUTOR_MMIO_IPA;
        let gicr_base = WINDOWS_ARM_GIC_REDISTRIBUTOR_MMIO_IPA;
        let timer_bit = 1_u32 << WINDOWS_ARM_VIRTUAL_TIMER_INTERRUPT_ID;

        assert_eq!(
            bus.dispatch(MmioAccess::write(
                gicd_base + GICD_CTLR_OFFSET,
                u64::from(GICD_CTLR_ENABLE_GRP1NS),
                4,
            )),
            MmioAction::WriteAccepted {
                value: u64::from(GICD_CTLR_ENABLE_GRP1NS),
                byte: GICD_CTLR_ENABLE_GRP1NS as u8,
            }
        );
        for register_base in [
            GICD_IGROUPR_BASE_OFFSET,
            GICD_ISENABLER_BASE_OFFSET,
            GICD_ISPENDR_BASE_OFFSET,
        ] {
            assert_eq!(
                bus.dispatch(MmioAccess::write(gicd_base + register_base + 4, 0x4, 4,)),
                MmioAction::WriteAccepted {
                    value: 0x4,
                    byte: 0x4,
                }
            );
        }
        assert_eq!(
            bus.dispatch(MmioAccess::write(
                gicd_base + GICD_IPRIORITYR_BASE_OFFSET + 34,
                0x20,
                1,
            )),
            MmioAction::WriteAccepted {
                value: 0x20,
                byte: 0x20,
            }
        );
        assert_eq!(
            bus.dispatch(MmioAccess::write(
                gicr_base + GICR_SGI_IGROUPR0_OFFSET,
                u64::from(timer_bit),
                4,
            )),
            MmioAction::WriteAccepted {
                value: u64::from(timer_bit),
                byte: 0,
            }
        );
        assert_eq!(
            bus.dispatch(MmioAccess::write(
                gicr_base + GICR_SGI_ISENABLER0_OFFSET,
                u64::from(timer_bit),
                4,
            )),
            MmioAction::WriteAccepted {
                value: u64::from(timer_bit),
                byte: 0,
            }
        );
        assert!(set_windows_arm_firmware_vtimer_ppi_pending(&mut bus, true));
        assert_eq!(
            gic_cpu_write(&mut cpu, &mut bus, ICC_IGRPEN1_EL1_SYSREG, 1),
            Some(GicV3CpuInterfaceAction::Write {
                refresh_level_sources: false,
            })
        );

        assert_eq!(
            gic_cpu_read(&mut cpu, &mut bus, ICC_HPPIR1_EL1_SYSREG),
            Some(GicV3CpuInterfaceAction::Read(34))
        );
        assert_eq!(
            gic_cpu_read(&mut cpu, &mut bus, ICC_IAR1_EL1_SYSREG),
            Some(GicV3CpuInterfaceAction::Read(34))
        );
        assert_eq!(
            gic_cpu_read(&mut cpu, &mut bus, ICC_RPR_EL1_SYSREG),
            Some(GicV3CpuInterfaceAction::Read(0x20))
        );
        assert_eq!(
            bus.dispatch(MmioAccess::read(gicr_base + GICR_SGI_ISPENDR0_OFFSET, 4)),
            MmioAction::ReadValue(u64::from(timer_bit))
        );
    }

    #[test]
    fn gicv3_cpu_interface_eoi_mode_requires_dir_to_deactivate_spi() {
        let block_devices = windows_arm_firmware_block_devices(None, None);
        let mut bus = windows_arm_firmware_mmio_bus_with_block_devices(&block_devices);
        let mut cpu = GicV3CpuInterfaceState::new();
        let base = WINDOWS_ARM_GIC_DISTRIBUTOR_MMIO_IPA;

        assert_eq!(
            bus.dispatch(MmioAccess::write(
                base + GICD_CTLR_OFFSET,
                u64::from(GICD_CTLR_ENABLE_GRP1NS),
                4,
            )),
            MmioAction::WriteAccepted {
                value: u64::from(GICD_CTLR_ENABLE_GRP1NS),
                byte: GICD_CTLR_ENABLE_GRP1NS as u8,
            }
        );
        for register_base in [
            GICD_IGROUPR_BASE_OFFSET,
            GICD_ISENABLER_BASE_OFFSET,
            GICD_ISPENDR_BASE_OFFSET,
        ] {
            assert_eq!(
                bus.dispatch(MmioAccess::write(base + register_base + 4, 0x4, 4)),
                MmioAction::WriteAccepted {
                    value: 0x4,
                    byte: 0x4,
                }
            );
        }
        assert_eq!(
            gic_cpu_write(
                &mut cpu,
                &mut bus,
                ICC_CTLR_EL1_SYSREG,
                ICC_CTLR_EL1_EOIMODE
            ),
            Some(GicV3CpuInterfaceAction::Write {
                refresh_level_sources: false,
            })
        );
        assert_eq!(
            gic_cpu_write(&mut cpu, &mut bus, ICC_IGRPEN1_EL1_SYSREG, 1),
            Some(GicV3CpuInterfaceAction::Write {
                refresh_level_sources: false,
            })
        );
        assert_eq!(
            gic_cpu_read(&mut cpu, &mut bus, ICC_IAR1_EL1_SYSREG),
            Some(GicV3CpuInterfaceAction::Read(34))
        );
        assert_eq!(
            bus.dispatch(MmioAccess::read(base + GICD_ISACTIVER_BASE_OFFSET + 4, 4)),
            MmioAction::ReadValue(0x4)
        );
        assert_eq!(
            gic_cpu_write(&mut cpu, &mut bus, ICC_EOIR1_EL1_SYSREG, 34),
            Some(GicV3CpuInterfaceAction::Write {
                refresh_level_sources: false,
            })
        );
        assert_eq!(
            gic_cpu_read(&mut cpu, &mut bus, ICC_RPR_EL1_SYSREG),
            Some(GicV3CpuInterfaceAction::Read(0xff))
        );
        assert_eq!(
            bus.dispatch(MmioAccess::read(base + GICD_ISACTIVER_BASE_OFFSET + 4, 4)),
            MmioAction::ReadValue(0x4)
        );
        assert_eq!(
            gic_cpu_write(&mut cpu, &mut bus, ICC_DIR_EL1_SYSREG, 34),
            Some(GicV3CpuInterfaceAction::Write {
                refresh_level_sources: true,
            })
        );
        assert_eq!(
            bus.dispatch(MmioAccess::read(base + GICD_ISACTIVER_BASE_OFFSET + 4, 4)),
            MmioAction::ReadValue(0)
        );
    }

    #[test]
    fn gicv3_cpu_interface_eoi_mode_requires_dir_to_deactivate_timer_ppi() {
        let block_devices = windows_arm_firmware_block_devices(None, None);
        let mut bus = windows_arm_firmware_mmio_bus_with_block_devices(&block_devices);
        let mut cpu = GicV3CpuInterfaceState::new();
        let gicr_base = WINDOWS_ARM_GIC_REDISTRIBUTOR_MMIO_IPA;
        let timer_bit = 1_u32 << WINDOWS_ARM_VIRTUAL_TIMER_INTERRUPT_ID;

        assert_eq!(
            bus.dispatch(MmioAccess::write(
                gicr_base + GICR_SGI_IGROUPR0_OFFSET,
                u64::from(timer_bit),
                4,
            )),
            MmioAction::WriteAccepted {
                value: u64::from(timer_bit),
                byte: 0,
            }
        );
        assert_eq!(
            bus.dispatch(MmioAccess::write(
                gicr_base + GICR_SGI_ISENABLER0_OFFSET,
                u64::from(timer_bit),
                4,
            )),
            MmioAction::WriteAccepted {
                value: u64::from(timer_bit),
                byte: 0,
            }
        );
        assert!(set_windows_arm_firmware_vtimer_ppi_pending(&mut bus, true));
        assert_eq!(
            gic_cpu_write(
                &mut cpu,
                &mut bus,
                ICC_CTLR_EL1_SYSREG,
                ICC_CTLR_EL1_EOIMODE
            ),
            Some(GicV3CpuInterfaceAction::Write {
                refresh_level_sources: false,
            })
        );
        assert_eq!(
            gic_cpu_write(&mut cpu, &mut bus, ICC_IGRPEN1_EL1_SYSREG, 1),
            Some(GicV3CpuInterfaceAction::Write {
                refresh_level_sources: false,
            })
        );
        assert_eq!(
            gic_cpu_read(&mut cpu, &mut bus, ICC_IAR1_EL1_SYSREG),
            Some(GicV3CpuInterfaceAction::Read(u64::from(
                WINDOWS_ARM_VIRTUAL_TIMER_INTERRUPT_ID
            )))
        );
        assert_eq!(
            bus.dispatch(MmioAccess::read(gicr_base + GICR_SGI_ISACTIVER0_OFFSET, 4)),
            MmioAction::ReadValue(u64::from(timer_bit))
        );
        assert_eq!(
            gic_cpu_write(
                &mut cpu,
                &mut bus,
                ICC_EOIR1_EL1_SYSREG,
                u64::from(WINDOWS_ARM_VIRTUAL_TIMER_INTERRUPT_ID),
            ),
            Some(GicV3CpuInterfaceAction::Write {
                refresh_level_sources: false,
            })
        );
        assert_eq!(
            gic_cpu_read(&mut cpu, &mut bus, ICC_RPR_EL1_SYSREG),
            Some(GicV3CpuInterfaceAction::Read(0xff))
        );
        assert_eq!(
            bus.dispatch(MmioAccess::read(gicr_base + GICR_SGI_ISACTIVER0_OFFSET, 4)),
            MmioAction::ReadValue(u64::from(timer_bit))
        );
        assert_eq!(
            gic_cpu_write(
                &mut cpu,
                &mut bus,
                ICC_DIR_EL1_SYSREG,
                u64::from(WINDOWS_ARM_VIRTUAL_TIMER_INTERRUPT_ID),
            ),
            Some(GicV3CpuInterfaceAction::Write {
                refresh_level_sources: true,
            })
        );
        assert_eq!(
            bus.dispatch(MmioAccess::read(gicr_base + GICR_SGI_ISACTIVER0_OFFSET, 4)),
            MmioAction::ReadValue(0)
        );
    }

    #[test]
    fn firmware_block_queue_notify_selects_backing_by_mmio_ipa() {
        let stem = format!(
            "bridgevm-hvf-firmware-block-queue-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let iso_path = std::env::temp_dir().join(format!("{stem}.iso"));
        let disk_path = std::env::temp_dir().join(format!("{stem}.raw"));
        let sector_start =
            (VIRTIO_BLOCK_SYNTHETIC_REQUEST_SECTOR * VIRTIO_BLOCK_SECTOR_BYTES) as usize;
        let mut iso = vec![0_u8; (VIRTIO_BLOCK_SECTOR_BYTES as usize) * 16];
        let mut disk = vec![0_u8; (VIRTIO_BLOCK_SECTOR_BYTES as usize) * 32];
        for offset in 0..VIRTIO_BLOCK_SYNTHETIC_REQUEST_DATA_BYTES as usize {
            iso[sector_start + offset] = 0xc0_u8.wrapping_add(offset as u8);
            disk[sector_start + offset] = 0xa0_u8.wrapping_add(offset as u8);
        }
        std::fs::write(&iso_path, &iso).unwrap();
        std::fs::write(&disk_path, &disk).unwrap();

        let block_devices =
            windows_arm_firmware_block_devices(Some(iso_path.clone()), Some(disk_path.clone()));
        let mut bus = windows_arm_firmware_mmio_bus_with_block_devices(&block_devices);
        assert_eq!(block_devices[0].capacity_sectors, 16);
        assert_eq!(block_devices[1].capacity_sectors, 32);
        assert_eq!(
            bus.dispatch(MmioAccess::read(
                WINDOWS_ARM_VIRTIO_INSTALLER_ISO_MMIO_IPA + VIRTIO_MMIO_BLOCK_CAPACITY_LOW_OFFSET,
                4,
            )),
            MmioAction::ReadValue(16)
        );
        assert_eq!(
            bus.dispatch(MmioAccess::read(
                WINDOWS_ARM_VIRTIO_TARGET_DISK_MMIO_IPA + VIRTIO_MMIO_BLOCK_CAPACITY_LOW_OFFSET,
                4,
            )),
            MmioAction::ReadValue(32)
        );

        let installer_notify_ipa =
            WINDOWS_ARM_VIRTIO_INSTALLER_ISO_MMIO_IPA + VIRTIO_MMIO_QUEUE_NOTIFY_OFFSET;
        let mut installer_backing = vec![0_u8; 16 * 1024];
        let mut installer_memory =
            VirtioGuestMemory::new(WINDOWS_ARM_GUEST_RAM_IPA, &mut installer_backing);
        configure_virtio_block_queue_on_bus(&mut bus, WINDOWS_ARM_VIRTIO_INSTALLER_ISO_MMIO_IPA);
        seed_synthetic_virtio_block_read_request(&mut installer_memory).unwrap();
        let installer_status_ipa =
            WINDOWS_ARM_VIRTIO_INSTALLER_ISO_MMIO_IPA + VIRTIO_MMIO_STATUS_OFFSET;
        let gicd_spi_pending_clear_ipa =
            WINDOWS_ARM_GIC_DISTRIBUTOR_MMIO_IPA + GICD_ICPENDR_BASE_OFFSET + 4;
        let gicd_spi_pending_set_ipa =
            WINDOWS_ARM_GIC_DISTRIBUTOR_MMIO_IPA + GICD_ISPENDR_BASE_OFFSET + 4;
        assert!(windows_arm_firmware_block_irq_source_may_change(
            &block_devices,
            installer_status_ipa,
            0,
        ));
        assert!(!windows_arm_firmware_block_irq_source_may_change(
            &block_devices,
            installer_status_ipa,
            VIRTIO_MMIO_BLOCK_STATUS_FEATURES_OK_VALUE,
        ));
        assert!(
            windows_arm_firmware_gicd_pending_clear_may_need_source_refresh(
                gicd_spi_pending_clear_ipa,
                0x4,
                4,
            )
        );
        assert!(
            !windows_arm_firmware_gicd_pending_clear_may_need_source_refresh(
                gicd_spi_pending_set_ipa,
                0x4,
                4,
            )
        );
        assert!(
            !windows_arm_firmware_gicd_pending_clear_may_need_source_refresh(
                gicd_spi_pending_clear_ipa,
                0,
                4,
            )
        );
        assert_eq!(
            complete_windows_arm_firmware_block_queue_notify(
                &mut bus,
                &mut installer_memory,
                &block_devices,
                installer_status_ipa,
                VIRTIO_MMIO_BLOCK_QUEUE_NOTIFY_VALUE,
            ),
            Err(VirtioBlockRequestError::UnexpectedQueueNotifyIpa {
                role: "installer-iso",
                ipa: installer_status_ipa,
            })
        );
        assert_eq!(
            bus.dispatch(MmioAccess::write(installer_notify_ipa, 1, 4)),
            MmioAction::WriteAccepted { value: 1, byte: 1 }
        );
        assert_eq!(
            complete_windows_arm_firmware_block_queue_notify(
                &mut bus,
                &mut installer_memory,
                &block_devices,
                installer_notify_ipa,
                1,
            ),
            Err(VirtioBlockRequestError::UnsupportedQueueNotifyValue {
                role: "installer-iso",
                value: 1,
            })
        );
        assert_eq!(
            bus.dispatch(MmioAccess::write(
                installer_notify_ipa,
                VIRTIO_MMIO_BLOCK_QUEUE_NOTIFY_VALUE,
                4,
            )),
            MmioAction::WriteAccepted {
                value: VIRTIO_MMIO_BLOCK_QUEUE_NOTIFY_VALUE,
                byte: VIRTIO_MMIO_BLOCK_QUEUE_NOTIFY_VALUE as u8,
            }
        );
        let installer_completion = complete_windows_arm_firmware_block_queue_notify(
            &mut bus,
            &mut installer_memory,
            &block_devices,
            installer_notify_ipa,
            VIRTIO_MMIO_BLOCK_QUEUE_NOTIFY_VALUE,
        )
        .unwrap();
        assert_eq!(installer_completion.role, "installer-iso");
        assert_eq!(installer_completion.backing_kind, "host-iso-readonly");
        assert_eq!(
            installer_completion.base_ipa,
            WINDOWS_ARM_VIRTIO_INSTALLER_ISO_MMIO_IPA
        );
        assert_eq!(
            installer_completion.completion.request_type,
            VIRTIO_BLK_T_IN
        );
        assert_eq!(installer_completion.completion.status, VIRTIO_BLK_S_OK);
        assert_eq!(installer_completion.byte_offset, 0xe00);
        assert_eq!(installer_completion.used_len, 513);
        assert_eq!(
            installer_memory
                .read_bytes(VIRTIO_BLOCK_SYNTHETIC_REQUEST_DATA_ADDRESS, 8)
                .unwrap(),
            vec![0xc0, 0xc1, 0xc2, 0xc3, 0xc4, 0xc5, 0xc6, 0xc7]
        );
        assert_eq!(
            bus.dispatch(MmioAccess::read(
                WINDOWS_ARM_VIRTIO_INSTALLER_ISO_MMIO_IPA + VIRTIO_MMIO_INTERRUPT_STATUS_OFFSET,
                4,
            )),
            MmioAction::ReadValue(VIRTIO_MMIO_INTERRUPT_USED_BUFFER_VALUE)
        );
        assert!(!refresh_windows_arm_firmware_device_irq_pending(
            &mut bus,
            &block_devices
        ));
        assert_eq!(
            bus.dispatch(MmioAccess::read(
                WINDOWS_ARM_GIC_DISTRIBUTOR_MMIO_IPA + GICD_ISPENDR_BASE_OFFSET + 4,
                4,
            )),
            MmioAction::ReadValue(0x4)
        );
        assert_eq!(
            bus.dispatch(MmioAccess::write(
                WINDOWS_ARM_GIC_DISTRIBUTOR_MMIO_IPA + GICD_CTLR_OFFSET,
                u64::from(GICD_CTLR_ENABLE_GRP1NS),
                4,
            )),
            MmioAction::WriteAccepted {
                value: u64::from(GICD_CTLR_ENABLE_GRP1NS),
                byte: GICD_CTLR_ENABLE_GRP1NS as u8,
            }
        );
        assert_eq!(
            bus.dispatch(MmioAccess::write(
                WINDOWS_ARM_GIC_DISTRIBUTOR_MMIO_IPA + GICD_IGROUPR_BASE_OFFSET + 4,
                0x4,
                4,
            )),
            MmioAction::WriteAccepted {
                value: 0x4,
                byte: 0x4,
            }
        );
        assert_eq!(
            bus.dispatch(MmioAccess::write(
                WINDOWS_ARM_GIC_DISTRIBUTOR_MMIO_IPA + GICD_ISENABLER_BASE_OFFSET + 4,
                0x4,
                4,
            )),
            MmioAction::WriteAccepted {
                value: 0x4,
                byte: 0x4,
            }
        );
        assert!(windows_arm_firmware_device_irq_line_assertable(
            &mut bus,
            &block_devices
        ));
        assert_eq!(
            bus.dispatch(MmioAccess::write(gicd_spi_pending_clear_ipa, 0x4, 4)),
            MmioAction::WriteAccepted {
                value: 0x4,
                byte: 0x4,
            }
        );
        assert!(!windows_arm_firmware_device_irq_line_assertable(
            &mut bus,
            &block_devices
        ));
        assert!(refresh_windows_arm_firmware_device_irq_pending(
            &mut bus,
            &block_devices
        ));
        assert_eq!(
            bus.dispatch(MmioAccess::read(gicd_spi_pending_set_ipa, 4)),
            MmioAction::ReadValue(0x4)
        );
        assert_eq!(
            bus.dispatch(MmioAccess::write(
                WINDOWS_ARM_VIRTIO_INSTALLER_ISO_MMIO_IPA + VIRTIO_MMIO_INTERRUPT_ACK_OFFSET,
                VIRTIO_MMIO_INTERRUPT_USED_BUFFER_VALUE,
                4,
            )),
            MmioAction::WriteAccepted {
                value: VIRTIO_MMIO_INTERRUPT_USED_BUFFER_VALUE,
                byte: VIRTIO_MMIO_INTERRUPT_USED_BUFFER_VALUE as u8,
            }
        );
        assert!(!refresh_windows_arm_firmware_device_irq_pending(
            &mut bus,
            &block_devices
        ));
        assert_eq!(
            bus.dispatch(MmioAccess::read(
                WINDOWS_ARM_GIC_DISTRIBUTOR_MMIO_IPA + GICD_ISPENDR_BASE_OFFSET + 4,
                4,
            )),
            MmioAction::ReadValue(0)
        );
        assert_eq!(
            bus.dispatch(MmioAccess::write(
                WINDOWS_ARM_GIC_DISTRIBUTOR_MMIO_IPA + GICD_ISPENDR_BASE_OFFSET + 4,
                0x4,
                4,
            )),
            MmioAction::WriteAccepted {
                value: 0x4,
                byte: 0x4,
            }
        );
        assert!(windows_arm_firmware_device_irq_line_assertable(
            &mut bus,
            &block_devices
        ));
        assert_eq!(
            bus.dispatch(MmioAccess::write(
                WINDOWS_ARM_GIC_DISTRIBUTOR_MMIO_IPA + GICD_ICPENDR_BASE_OFFSET + 4,
                0x4,
                4,
            )),
            MmioAction::WriteAccepted {
                value: 0x4,
                byte: 0x4,
            }
        );
        assert!(!windows_arm_firmware_device_irq_line_assertable(
            &mut bus,
            &block_devices
        ));

        let target_notify_ipa =
            WINDOWS_ARM_VIRTIO_TARGET_DISK_MMIO_IPA + VIRTIO_MMIO_QUEUE_NOTIFY_OFFSET;
        let mut target_backing = vec![0_u8; 16 * 1024];
        let mut target_memory =
            VirtioGuestMemory::new(WINDOWS_ARM_GUEST_RAM_IPA, &mut target_backing);
        configure_virtio_block_queue_on_bus(&mut bus, WINDOWS_ARM_VIRTIO_TARGET_DISK_MMIO_IPA);
        seed_synthetic_virtio_block_write_request_as_first(&mut target_memory).unwrap();
        assert_eq!(
            bus.dispatch(MmioAccess::write(
                target_notify_ipa,
                VIRTIO_MMIO_BLOCK_QUEUE_NOTIFY_VALUE,
                4,
            )),
            MmioAction::WriteAccepted {
                value: VIRTIO_MMIO_BLOCK_QUEUE_NOTIFY_VALUE,
                byte: VIRTIO_MMIO_BLOCK_QUEUE_NOTIFY_VALUE as u8,
            }
        );
        let target_completion = complete_windows_arm_firmware_block_queue_notify(
            &mut bus,
            &mut target_memory,
            &block_devices,
            target_notify_ipa,
            VIRTIO_MMIO_BLOCK_QUEUE_NOTIFY_VALUE,
        )
        .unwrap();
        assert_eq!(target_completion.role, "target-disk");
        assert_eq!(target_completion.backing_kind, "host-file-writable");
        assert_eq!(
            target_completion.base_ipa,
            WINDOWS_ARM_VIRTIO_TARGET_DISK_MMIO_IPA
        );
        assert_eq!(target_completion.completion.request_type, VIRTIO_BLK_T_OUT);
        assert_eq!(target_completion.completion.status, VIRTIO_BLK_S_OK);
        assert_eq!(target_completion.byte_offset, 0xe00);
        assert_eq!(target_completion.used_len, VIRTIO_BLOCK_STATUS_BYTES);
        assert_eq!(
            bus.dispatch(MmioAccess::read(
                WINDOWS_ARM_VIRTIO_TARGET_DISK_MMIO_IPA + VIRTIO_MMIO_INTERRUPT_STATUS_OFFSET,
                4,
            )),
            MmioAction::ReadValue(VIRTIO_MMIO_INTERRUPT_USED_BUFFER_VALUE)
        );
        assert!(!refresh_windows_arm_firmware_device_irq_pending(
            &mut bus,
            &block_devices
        ));
        assert_eq!(
            bus.dispatch(MmioAccess::read(
                WINDOWS_ARM_GIC_DISTRIBUTOR_MMIO_IPA + GICD_ISPENDR_BASE_OFFSET + 4,
                4,
            )),
            MmioAction::ReadValue(0x8)
        );
        assert_eq!(
            bus.dispatch(MmioAccess::write(
                WINDOWS_ARM_GIC_DISTRIBUTOR_MMIO_IPA + GICD_IGROUPR_BASE_OFFSET + 4,
                0xc,
                4,
            )),
            MmioAction::WriteAccepted {
                value: 0xc,
                byte: 0xc,
            }
        );
        assert_eq!(
            bus.dispatch(MmioAccess::write(
                WINDOWS_ARM_GIC_DISTRIBUTOR_MMIO_IPA + GICD_ISENABLER_BASE_OFFSET + 4,
                0x8,
                4,
            )),
            MmioAction::WriteAccepted {
                value: 0x8,
                byte: 0x8,
            }
        );
        assert!(windows_arm_firmware_device_irq_line_assertable(
            &mut bus,
            &block_devices
        ));
        assert!(refresh_windows_arm_firmware_device_irq_pending(
            &mut bus,
            &block_devices
        ));
        assert_eq!(
            bus.dispatch(MmioAccess::write(
                WINDOWS_ARM_VIRTIO_TARGET_DISK_MMIO_IPA + VIRTIO_MMIO_INTERRUPT_ACK_OFFSET,
                VIRTIO_MMIO_INTERRUPT_USED_BUFFER_VALUE,
                4,
            )),
            MmioAction::WriteAccepted {
                value: VIRTIO_MMIO_INTERRUPT_USED_BUFFER_VALUE,
                byte: VIRTIO_MMIO_INTERRUPT_USED_BUFFER_VALUE as u8,
            }
        );
        assert!(!refresh_windows_arm_firmware_device_irq_pending(
            &mut bus,
            &block_devices
        ));
        assert_eq!(
            bus.dispatch(MmioAccess::read(
                WINDOWS_ARM_GIC_DISTRIBUTOR_MMIO_IPA + GICD_ISPENDR_BASE_OFFSET + 4,
                4,
            )),
            MmioAction::ReadValue(0)
        );
        let persisted = std::fs::read(&disk_path).unwrap();
        assert_eq!(
            &persisted[sector_start..sector_start + 8],
            &[0xe0, 0xe1, 0xe2, 0xe3, 0xe4, 0xe5, 0xe6, 0xe7]
        );

        let _ = std::fs::remove_file(&iso_path);
        let _ = std::fs::remove_file(&disk_path);
    }

    #[test]
    fn mmio_serial_device_probe_is_opt_in_and_qemu_free() {
        let probe = probe_hvf_mmio_serial_device(false);
        let output = probe.render_text();

        assert!(!probe.allowed);
        assert!(!probe.attempted);
        assert!(!probe.vm_created);
        assert!(!probe.memory_mapped);
        assert!(!probe.vcpu_created);
        assert!(!probe.write_run_attempted);
        assert!(!probe.write_exit_observed);
        assert!(!probe.device_bus_created);
        assert_eq!(probe.device_bus_device_count, 0);
        assert!(!probe.write_handled_by_device);
        assert!(!probe.write_value_captured);
        assert!(!probe.status_run_attempted);
        assert!(!probe.status_exit_observed);
        assert!(!probe.status_handled_by_device);
        assert!(!probe.status_value_injected);
        assert!(!probe.continuation_run_attempted);
        assert!(!probe.continuation_exit_observed);
        assert!(output.contains("HVF MMIO serial device probe"));
        assert!(output.contains("QEMU: not used"));
        assert!(output.contains("Apple VZ: not used"));
        assert!(
            output.contains("Guest execution: STR data register, LDR status register, then HVC")
        );
        assert!(output.contains("Device model: PL011 UART skeleton"));
        assert!(output.contains("Allowed: false"));
        assert!(output.contains("Attempted: false"));
        assert!(output.contains("Device bus created: false"));
        assert!(output.contains("Device bus device count: 0"));
        assert!(output.contains("Write handled by device: false"));
        assert!(output.contains("Status handled by device: false"));
        assert!(output.contains("Serial data IPA: 0x50000000"));
        assert!(output.contains("Serial status IPA: 0x50000018"));
        assert!(output.contains("Instructions: STR X0, [X1]; LDR X0, [X2]; HVC #0"));
        assert!(output.contains("Serial write value: 0x41"));
        assert!(output.contains("Serial status value: 0x90"));
        assert!(output.contains("Write run status name: not attempted"));
        assert!(output.contains("Status run status name: not attempted"));
        assert!(output.contains("Continuation run status name: not attempted"));
        assert!(!output.contains("qemu-system"));
        assert!(!output.contains('%'));
    }

    #[test]
    fn mmio_serial_device_probe_render_records_three_exit_device_loop() {
        let probe = HvfMmioSerialDeviceProbe {
            allowed: true,
            attempted: true,
            vm_created: true,
            memory_allocated: true,
            memory_mapped: true,
            vcpu_created: true,
            pc_set: true,
            cpsr_set: true,
            write_value_register_set: true,
            data_address_register_set: true,
            status_address_register_set: true,
            device_bus_created: true,
            device_bus_device_count: 1,
            write_run_attempted: true,
            write_exit_observed: true,
            write_handled_by_device: true,
            write_value_captured: true,
            pc_advanced_after_write: true,
            status_run_attempted: true,
            status_exit_observed: true,
            status_handled_by_device: true,
            status_value_injected: true,
            pc_advanced_after_status: true,
            continuation_run_attempted: true,
            continuation_exit_observed: true,
            status_value_preserved: true,
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
            device_model: PL011_UART_MODEL,
            code_ipa_start: 0x4000_0000,
            data_ipa: 0x5000_0000,
            status_ipa: 0x5000_0018,
            bytes: 16 * 1024,
            instructions: "STR X0, [X1]; LDR X0, [X2]; HVC #0",
            serial_write_value: 0x41,
            serial_status_value: 0x90,
            captured_write_value: Some(0x41),
            captured_byte: Some(0x41),
            vm_create_status: Some(0),
            allocate_status: Some(0),
            map_status: Some(0),
            vcpu_create_status: Some(0),
            pc_set_status: Some(0),
            cpsr_set_status: Some(0),
            write_value_register_set_status: Some(0),
            data_address_register_set_status: Some(0),
            status_address_register_set_status: Some(0),
            write_run_status: Some(0),
            write_exit_reason: Some(1),
            write_exit_syndrome: Some(0x93c0_8046),
            write_exit_virtual_address: Some(0x5000_0000),
            write_exit_physical_address: Some(0x5000_0000),
            write_watchdog_cancel_status: None,
            write_value_capture_status: Some(0),
            pc_read_after_write_status: Some(0),
            pc_after_write_exit: Some(0x4000_0000),
            pc_advance_after_write_status: Some(0),
            status_run_status: Some(0),
            status_exit_reason: Some(1),
            status_exit_syndrome: Some(0x93c0_8006),
            status_exit_virtual_address: Some(0x5000_0018),
            status_exit_physical_address: Some(0x5000_0018),
            status_watchdog_cancel_status: None,
            status_value_set_status: Some(0),
            pc_read_after_status_status: Some(0),
            pc_after_status_exit: Some(0x4000_0004),
            pc_advance_after_status_status: Some(0),
            continuation_run_status: Some(0),
            continuation_exit_reason: Some(1),
            continuation_exit_syndrome: Some(0x5a00_0000),
            continuation_exit_virtual_address: Some(0),
            continuation_exit_physical_address: Some(0),
            continuation_watchdog_cancel_status: None,
            status_value_after_continue_status: Some(0),
            status_value_after_continue: Some(0x90),
            vcpu_destroy_status: Some(0),
            unmap_status: Some(0),
            vm_destroy_status: Some(0),
            deallocate_status: Some(0),
            blockers: Vec::new(),
        };
        let output = probe.render_text();

        assert!(output.contains("Allowed: true"));
        assert!(output.contains("Attempted: true"));
        assert!(output.contains("Device model: PL011 UART skeleton"));
        assert!(output.contains("Device bus created: true"));
        assert!(output.contains("Device bus device count: 1"));
        assert!(output.contains("Write exit observed: true"));
        assert!(output.contains("Write handled by device: true"));
        assert!(output.contains("Write value captured: true"));
        assert!(output.contains("PC advanced after write: true"));
        assert!(output.contains("Status exit observed: true"));
        assert!(output.contains("Status handled by device: true"));
        assert!(output.contains("Status value injected: true"));
        assert!(output.contains("PC advanced after status: true"));
        assert!(output.contains("Continuation exit observed: true"));
        assert!(output.contains("Status value preserved: true"));
        assert!(output.contains("Captured write value: 0x41"));
        assert!(output.contains("Captured byte: 0x41"));
        assert!(output.contains("Write exit syndrome: 0x93c08046"));
        assert!(output.contains("Write exit virtual address: 0x50000000"));
        assert!(output.contains("Status exit syndrome: 0x93c08006"));
        assert!(output.contains("Status exit virtual address: 0x50000018"));
        assert!(output.contains("Continuation exit syndrome: 0x5a000000"));
        assert!(output.contains("Status value after continue: 0x90"));
        assert!(output.contains("Blockers: none"));
        assert!(!output.contains('%'));
    }

    #[test]
    fn mmio_bus_routes_probe_serial_data_write() {
        let mut bus = MmioBus::default();
        bus.attach(Box::new(Pl011UartDevice::new(0x5000_0000, 0x90)));

        assert_eq!(bus.device_count(), 1);
        assert_eq!(
            bus.dispatch(MmioAccess::write(0x5000_0000, 0x141, 8)),
            MmioAction::WriteAccepted {
                value: 0x141,
                byte: 0x41,
            }
        );
    }

    #[test]
    fn mmio_bus_routes_probe_serial_status_read() {
        let mut bus = MmioBus::default();
        bus.attach(Box::new(Pl011UartDevice::new(0x5000_0000, 0x90)));

        assert_eq!(
            bus.dispatch(MmioAccess::read(0x5000_0018, 8)),
            MmioAction::ReadValue(0x90)
        );
    }

    #[test]
    fn mmio_bus_routes_pl031_rtc_read_after_uart_window() {
        let mut bus = MmioBus::default();
        bus.attach(Box::new(Pl011UartDevice::new(0x5000_0000, 0x90)));
        bus.attach(Box::new(Pl031RtcDevice::new(0x5000_1000, 0x2026_0618)));

        assert_eq!(bus.device_count(), 2);
        assert_eq!(
            bus.dispatch(MmioAccess::read(0x5000_1000, 8)),
            MmioAction::ReadValue(0x2026_0618)
        );
    }

    #[test]
    fn mmio_bus_routes_virtio_block_identity_registers_after_boot_devices() {
        let mut bus = MmioBus::default();
        let block_base = 0x5000_2000;
        let block = VirtioMmioBlockDevice::new(0x5000_2000);
        let magic_ipa = block_base + VIRTIO_MMIO_MAGIC_VALUE_OFFSET;
        let version_ipa = block_base + VIRTIO_MMIO_VERSION_OFFSET;
        let device_id_ipa = block_base + VIRTIO_MMIO_DEVICE_ID_OFFSET;
        let vendor_id_ipa = block_base + VIRTIO_MMIO_VENDOR_ID_OFFSET;

        bus.attach(Box::new(Pl011UartDevice::new(0x5000_0000, 0x90)));
        bus.attach(Box::new(Pl031RtcDevice::new(0x5000_1000, 0x2026_0618)));
        bus.attach(Box::new(block));

        assert_eq!(bus.device_count(), 3);
        assert_eq!(
            bus.dispatch(MmioAccess::read(magic_ipa, 4)),
            MmioAction::ReadValue(0x7472_6976)
        );
        assert_eq!(
            bus.dispatch(MmioAccess::read(version_ipa, 4)),
            MmioAction::ReadValue(2)
        );
        assert_eq!(
            bus.dispatch(MmioAccess::read(device_id_ipa, 4)),
            MmioAction::ReadValue(2)
        );
        assert_eq!(
            bus.dispatch(MmioAccess::read(vendor_id_ipa, 4)),
            MmioAction::ReadValue(0x4252_564d)
        );
    }

    #[test]
    fn mmio_bus_typed_lookup_skips_overlapping_wrong_type() {
        let mut bus = MmioBus::default();
        let block_base = 0x5000_2000;
        bus.attach(Box::new(Pl011UartDevice::new(block_base, 0x90)));
        bus.attach(Box::new(VirtioMmioBlockDevice::new(block_base)));

        assert!(bus
            .find_device_mut_at::<VirtioMmioBlockDevice>(block_base)
            .is_some());
    }

    #[test]
    fn mmio_bus_routes_virtio_block_queue_and_config_registers() {
        let mut bus = MmioBus::default();
        let block_base = 0x5000_2000;
        bus.attach(Box::new(VirtioMmioBlockDevice::new(block_base)));

        let writes = [
            (
                VIRTIO_MMIO_DRIVER_FEATURES_OFFSET,
                VIRTIO_MMIO_BLOCK_DRIVER_FEATURES_VALUE,
            ),
            (
                VIRTIO_MMIO_STATUS_OFFSET,
                VIRTIO_MMIO_BLOCK_STATUS_ACK_VALUE,
            ),
            (
                VIRTIO_MMIO_STATUS_OFFSET,
                VIRTIO_MMIO_BLOCK_STATUS_DRIVER_VALUE,
            ),
            (
                VIRTIO_MMIO_STATUS_OFFSET,
                VIRTIO_MMIO_BLOCK_STATUS_FEATURES_OK_VALUE,
            ),
            (
                VIRTIO_MMIO_QUEUE_SEL_OFFSET,
                VIRTIO_MMIO_BLOCK_QUEUE_SEL_VALUE,
            ),
            (
                VIRTIO_MMIO_QUEUE_NUM_OFFSET,
                VIRTIO_MMIO_BLOCK_QUEUE_NUM_VALUE,
            ),
            (
                VIRTIO_MMIO_QUEUE_DESC_LOW_OFFSET,
                VIRTIO_MMIO_BLOCK_QUEUE_DESC_ADDRESS & 0xffff_ffff,
            ),
            (
                VIRTIO_MMIO_QUEUE_DESC_HIGH_OFFSET,
                VIRTIO_MMIO_BLOCK_QUEUE_DESC_ADDRESS >> 32,
            ),
            (
                VIRTIO_MMIO_QUEUE_DRIVER_LOW_OFFSET,
                VIRTIO_MMIO_BLOCK_QUEUE_DRIVER_ADDRESS & 0xffff_ffff,
            ),
            (
                VIRTIO_MMIO_QUEUE_DRIVER_HIGH_OFFSET,
                VIRTIO_MMIO_BLOCK_QUEUE_DRIVER_ADDRESS >> 32,
            ),
            (
                VIRTIO_MMIO_QUEUE_DEVICE_LOW_OFFSET,
                VIRTIO_MMIO_BLOCK_QUEUE_DEVICE_ADDRESS & 0xffff_ffff,
            ),
            (
                VIRTIO_MMIO_QUEUE_DEVICE_HIGH_OFFSET,
                VIRTIO_MMIO_BLOCK_QUEUE_DEVICE_ADDRESS >> 32,
            ),
            (
                VIRTIO_MMIO_QUEUE_READY_OFFSET,
                VIRTIO_MMIO_BLOCK_QUEUE_READY_VALUE,
            ),
            (VIRTIO_MMIO_STATUS_OFFSET, VIRTIO_MMIO_BLOCK_STATUS_VALUE),
            (
                VIRTIO_MMIO_QUEUE_NOTIFY_OFFSET,
                VIRTIO_MMIO_BLOCK_QUEUE_NOTIFY_VALUE,
            ),
        ];

        assert_eq!(
            bus.dispatch(MmioAccess::read(
                block_base + VIRTIO_MMIO_DEVICE_FEATURES_OFFSET,
                4
            )),
            MmioAction::ReadValue(VIRTIO_MMIO_BLOCK_DEVICE_FEATURES_VALUE)
        );

        for (offset, value) in writes {
            assert_eq!(
                bus.dispatch(MmioAccess::write(block_base + offset, value, 4)),
                MmioAction::WriteAccepted {
                    value,
                    byte: (value & 0xff) as u8,
                }
            );
        }

        let reads = [
            (
                VIRTIO_MMIO_QUEUE_NUM_MAX_OFFSET,
                VIRTIO_MMIO_BLOCK_QUEUE_NUM_MAX_VALUE,
            ),
            (
                VIRTIO_MMIO_QUEUE_READY_OFFSET,
                VIRTIO_MMIO_BLOCK_QUEUE_READY_VALUE,
            ),
            (
                VIRTIO_MMIO_QUEUE_DESC_LOW_OFFSET,
                VIRTIO_MMIO_BLOCK_QUEUE_DESC_ADDRESS & 0xffff_ffff,
            ),
            (
                VIRTIO_MMIO_QUEUE_DESC_HIGH_OFFSET,
                VIRTIO_MMIO_BLOCK_QUEUE_DESC_ADDRESS >> 32,
            ),
            (
                VIRTIO_MMIO_QUEUE_DRIVER_LOW_OFFSET,
                VIRTIO_MMIO_BLOCK_QUEUE_DRIVER_ADDRESS & 0xffff_ffff,
            ),
            (
                VIRTIO_MMIO_QUEUE_DRIVER_HIGH_OFFSET,
                VIRTIO_MMIO_BLOCK_QUEUE_DRIVER_ADDRESS >> 32,
            ),
            (
                VIRTIO_MMIO_QUEUE_DEVICE_LOW_OFFSET,
                VIRTIO_MMIO_BLOCK_QUEUE_DEVICE_ADDRESS & 0xffff_ffff,
            ),
            (
                VIRTIO_MMIO_QUEUE_DEVICE_HIGH_OFFSET,
                VIRTIO_MMIO_BLOCK_QUEUE_DEVICE_ADDRESS >> 32,
            ),
            (
                VIRTIO_MMIO_INTERRUPT_STATUS_OFFSET,
                VIRTIO_MMIO_BLOCK_INTERRUPT_STATUS_VALUE,
            ),
            (VIRTIO_MMIO_STATUS_OFFSET, VIRTIO_MMIO_BLOCK_STATUS_VALUE),
            (
                VIRTIO_MMIO_CONFIG_GENERATION_OFFSET,
                VIRTIO_MMIO_BLOCK_CONFIG_GENERATION_VALUE,
            ),
            (
                VIRTIO_MMIO_BLOCK_CAPACITY_LOW_OFFSET,
                VIRTIO_MMIO_BLOCK_CAPACITY_SECTORS & 0xffff_ffff,
            ),
            (
                VIRTIO_MMIO_BLOCK_CAPACITY_HIGH_OFFSET,
                VIRTIO_MMIO_BLOCK_CAPACITY_SECTORS >> 32,
            ),
        ];

        for (offset, expected) in reads {
            assert_eq!(
                bus.dispatch(MmioAccess::read(block_base + offset, 4)),
                MmioAction::ReadValue(expected)
            );
        }
    }

    #[test]
    fn virtio_block_status_zero_resets_queue_state() {
        let mut bus = MmioBus::default();
        let block_base = 0x5000_2000;
        bus.attach(Box::new(VirtioMmioBlockDevice::new(block_base)));
        let mut backing = vec![0_u8; 16 * 1024];
        let mut memory = VirtioGuestMemory::new(WINDOWS_ARM_GUEST_RAM_IPA, &mut backing);

        configure_virtio_block_queue_on_bus(&mut bus, block_base);
        seed_synthetic_virtio_block_read_request(&mut memory).unwrap();
        assert_eq!(
            bus.dispatch(MmioAccess::write(
                block_base + VIRTIO_MMIO_QUEUE_NOTIFY_OFFSET,
                VIRTIO_MMIO_BLOCK_QUEUE_NOTIFY_VALUE,
                4,
            )),
            MmioAction::WriteAccepted {
                value: VIRTIO_MMIO_BLOCK_QUEUE_NOTIFY_VALUE,
                byte: VIRTIO_MMIO_BLOCK_QUEUE_NOTIFY_VALUE as u8,
            }
        );
        {
            let block = bus
                .find_device_mut_at::<VirtioMmioBlockDevice>(block_base)
                .unwrap();
            block
                .complete_next_available_block_request(&mut memory)
                .unwrap();
        }
        assert_eq!(
            bus.dispatch(MmioAccess::read(
                block_base + VIRTIO_MMIO_INTERRUPT_STATUS_OFFSET,
                4,
            )),
            MmioAction::ReadValue(VIRTIO_MMIO_INTERRUPT_USED_BUFFER_VALUE)
        );

        assert_eq!(
            bus.dispatch(MmioAccess::write(
                block_base + VIRTIO_MMIO_STATUS_OFFSET,
                0,
                4,
            )),
            MmioAction::WriteAccepted { value: 0, byte: 0 }
        );
        for (offset, expected) in [
            (VIRTIO_MMIO_STATUS_OFFSET, 0),
            (VIRTIO_MMIO_DRIVER_FEATURES_OFFSET, 0),
            (VIRTIO_MMIO_QUEUE_NUM_OFFSET, 0),
            (VIRTIO_MMIO_QUEUE_READY_OFFSET, 0),
            (VIRTIO_MMIO_QUEUE_DESC_LOW_OFFSET, 0),
            (VIRTIO_MMIO_QUEUE_DRIVER_LOW_OFFSET, 0),
            (VIRTIO_MMIO_QUEUE_DEVICE_LOW_OFFSET, 0),
            (
                VIRTIO_MMIO_INTERRUPT_STATUS_OFFSET,
                VIRTIO_MMIO_BLOCK_INTERRUPT_STATUS_VALUE,
            ),
            (
                VIRTIO_MMIO_BLOCK_CAPACITY_LOW_OFFSET,
                VIRTIO_MMIO_BLOCK_CAPACITY_SECTORS & 0xffff_ffff,
            ),
        ] {
            assert_eq!(
                bus.dispatch(MmioAccess::read(block_base + offset, 4)),
                MmioAction::ReadValue(expected)
            );
        }

        configure_virtio_block_queue_on_bus(&mut bus, block_base);
        seed_synthetic_virtio_block_read_request(&mut memory).unwrap();
        let block = bus
            .find_device_mut_at::<VirtioMmioBlockDevice>(block_base)
            .unwrap();
        assert!(block
            .complete_next_available_block_request(&mut memory)
            .is_ok());
    }

    #[test]
    fn virtio_block_completes_one_available_read_request() {
        let block_base = 0x5000_2000;
        let guest_base = 0x4000_0000;
        let header_ipa = guest_base + 0x80;
        let data_ipa = guest_base + 0x400;
        let status_ipa = guest_base + 0x700;
        let sector = 7;
        let mut backing = vec![0_u8; 16 * 1024];
        let mut memory = VirtioGuestMemory::new(guest_base, &mut backing);
        let mut block = VirtioMmioBlockDevice::new(block_base);

        for (offset, value) in [
            (
                VIRTIO_MMIO_QUEUE_NUM_OFFSET,
                VIRTIO_MMIO_BLOCK_QUEUE_NUM_VALUE,
            ),
            (
                VIRTIO_MMIO_QUEUE_DESC_LOW_OFFSET,
                VIRTIO_MMIO_BLOCK_QUEUE_DESC_ADDRESS & 0xffff_ffff,
            ),
            (
                VIRTIO_MMIO_QUEUE_DESC_HIGH_OFFSET,
                VIRTIO_MMIO_BLOCK_QUEUE_DESC_ADDRESS >> 32,
            ),
            (
                VIRTIO_MMIO_QUEUE_DRIVER_LOW_OFFSET,
                VIRTIO_MMIO_BLOCK_QUEUE_DRIVER_ADDRESS & 0xffff_ffff,
            ),
            (
                VIRTIO_MMIO_QUEUE_DRIVER_HIGH_OFFSET,
                VIRTIO_MMIO_BLOCK_QUEUE_DRIVER_ADDRESS >> 32,
            ),
            (
                VIRTIO_MMIO_QUEUE_DEVICE_LOW_OFFSET,
                VIRTIO_MMIO_BLOCK_QUEUE_DEVICE_ADDRESS & 0xffff_ffff,
            ),
            (
                VIRTIO_MMIO_QUEUE_DEVICE_HIGH_OFFSET,
                VIRTIO_MMIO_BLOCK_QUEUE_DEVICE_ADDRESS >> 32,
            ),
            (
                VIRTIO_MMIO_QUEUE_READY_OFFSET,
                VIRTIO_MMIO_BLOCK_QUEUE_READY_VALUE,
            ),
        ] {
            assert!(matches!(
                block.handle(MmioAccess::write(block_base + offset, value, 4)),
                MmioAction::WriteAccepted { .. }
            ));
        }

        memory.write_u32(header_ipa, VIRTIO_BLK_T_IN).unwrap();
        memory.write_u32(header_ipa + 4, 0).unwrap();
        memory.write_u64(header_ipa + 8, sector).unwrap();
        VirtqDescriptor {
            addr: header_ipa,
            len: VIRTIO_BLOCK_REQUEST_HEADER_BYTES,
            flags: VIRTQ_DESC_F_NEXT,
            next: 1,
        }
        .write(&mut memory, VIRTIO_MMIO_BLOCK_QUEUE_DESC_ADDRESS, 0)
        .unwrap();
        VirtqDescriptor {
            addr: data_ipa,
            len: 512,
            flags: VIRTQ_DESC_F_NEXT | VIRTQ_DESC_F_WRITE,
            next: 2,
        }
        .write(&mut memory, VIRTIO_MMIO_BLOCK_QUEUE_DESC_ADDRESS, 1)
        .unwrap();
        VirtqDescriptor {
            addr: status_ipa,
            len: VIRTIO_BLOCK_STATUS_BYTES,
            flags: VIRTQ_DESC_F_WRITE,
            next: 0,
        }
        .write(&mut memory, VIRTIO_MMIO_BLOCK_QUEUE_DESC_ADDRESS, 2)
        .unwrap();
        memory
            .write_u16(VIRTIO_MMIO_BLOCK_QUEUE_DRIVER_ADDRESS + 2, 1)
            .unwrap();
        memory
            .write_u16(VIRTIO_MMIO_BLOCK_QUEUE_DRIVER_ADDRESS + 4, 0)
            .unwrap();

        let completion = block
            .complete_next_available_block_request(&mut memory)
            .unwrap();

        assert_eq!(
            completion,
            VirtioBlockRequestCompletion {
                descriptor_index: 0,
                request_type: VIRTIO_BLK_T_IN,
                sector,
                data_bytes: 512,
                status: VIRTIO_BLK_S_OK,
                used_index: 1,
                interrupt_status: VIRTIO_MMIO_INTERRUPT_USED_BUFFER_VALUE,
            }
        );
        assert_eq!(block.completed_requests, 1);
        assert_eq!(
            memory.read_bytes(data_ipa, 8).unwrap(),
            (0..8)
                .map(|offset| synthetic_block_byte(sector, offset))
                .collect::<Vec<_>>()
        );
        assert_eq!(
            memory.read_bytes(status_ipa, 1).unwrap(),
            vec![VIRTIO_BLK_S_OK]
        );
        assert_eq!(
            memory
                .read_u32(VIRTIO_MMIO_BLOCK_QUEUE_DEVICE_ADDRESS + 4)
                .unwrap(),
            0
        );
        assert_eq!(
            memory
                .read_u32(VIRTIO_MMIO_BLOCK_QUEUE_DEVICE_ADDRESS + 8)
                .unwrap(),
            513
        );
        assert_eq!(
            memory
                .read_u16(VIRTIO_MMIO_BLOCK_QUEUE_DEVICE_ADDRESS + 2)
                .unwrap(),
            1
        );
        assert_eq!(
            block.handle(MmioAccess::read(
                block_base + VIRTIO_MMIO_INTERRUPT_STATUS_OFFSET,
                4
            )),
            MmioAction::ReadValue(VIRTIO_MMIO_INTERRUPT_USED_BUFFER_VALUE)
        );
        assert_eq!(
            block.complete_next_available_block_request(&mut memory),
            Err(VirtioBlockRequestError::NoAvailableRequest)
        );
    }

    #[test]
    fn virtio_block_request_model_probe_reports_completion() {
        let probe = probe_virtio_block_request_model();
        let output = probe.render_text();

        assert!(probe.configured_via_mmio);
        assert!(probe.configured_via_mmio_bus);
        assert!(probe.queue_notified);
        assert_eq!(probe.queue_notify_value, Some(0));
        assert!(probe.completed_via_device_bus);
        assert!(probe.completed);
        assert_eq!(probe.descriptor_index, Some(0));
        assert_eq!(probe.request_type, Some(VIRTIO_BLK_T_IN));
        assert_eq!(probe.sector, Some(7));
        assert_eq!(probe.data_bytes, Some(512));
        assert_eq!(
            probe.data_prefix,
            (0..8)
                .map(|offset| synthetic_block_byte(7, offset))
                .collect::<Vec<_>>()
        );
        assert_eq!(probe.status, Some(VIRTIO_BLK_S_OK));
        assert_eq!(probe.used_index, Some(1));
        assert_eq!(probe.used_len, Some(513));
        assert_eq!(
            probe.interrupt_status,
            Some(VIRTIO_MMIO_INTERRUPT_USED_BUFFER_VALUE)
        );
        assert!(probe.blockers.is_empty());
        assert!(output.contains("VirtIO block request model probe"));
        assert!(output.contains("QEMU: not used"));
        assert!(output.contains("Apple VZ: not used"));
        assert!(output.contains("HVF: not entered"));
        assert!(output
            .contains("Guest execution: not entered; in-memory VirtIO block descriptor chain"));
        assert!(output.contains("Configured via MMIO: true"));
        assert!(output.contains("Configured via MMIO bus: true"));
        assert!(output.contains("Queue notified: true"));
        assert!(output.contains("Queue notify value: 0x0"));
        assert!(output.contains("Completed via device bus: true"));
        assert!(output.contains("Completed: true"));
        assert!(output.contains("Descriptor index: 0x0"));
        assert!(output.contains("Request type: 0x0"));
        assert!(output.contains("Sector: 0x7"));
        assert!(output.contains("Data bytes: 0x200"));
        assert!(output.contains("Data prefix: 0x0708090a0b0c0d0e"));
        assert!(output.contains("Status byte: 0x0"));
        assert!(output.contains("Used index: 0x1"));
        assert!(output.contains("Used length: 0x201"));
        assert!(output.contains("Interrupt status: 0x1"));
        assert!(output.contains("Blockers: none"));
        assert!(!output.contains("qemu-system"));
        assert!(!output.contains('%'));
    }

    #[test]
    fn virtio_block_file_backing_probe_reads_from_host_file() {
        let mut disk = vec![0_u8; (VIRTIO_BLOCK_SECTOR_BYTES as usize) * 16];
        let sector_start =
            (VIRTIO_BLOCK_SYNTHETIC_REQUEST_SECTOR * VIRTIO_BLOCK_SECTOR_BYTES) as usize;
        for offset in 0..VIRTIO_BLOCK_SYNTHETIC_REQUEST_DATA_BYTES as usize {
            disk[sector_start + offset] = 0xa0_u8.wrapping_add(offset as u8);
        }
        let path = std::env::temp_dir().join(format!(
            "bridgevm-hvf-file-backed-{}-{}.img",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::write(&path, &disk).unwrap();

        let probe = probe_virtio_block_file_backing(path.clone());
        let output = probe.render_text();
        let _ = std::fs::remove_file(&path);

        assert_eq!(probe.disk_path, path);
        assert_eq!(probe.backing_kind, "host-file");
        assert!(probe.configured_via_mmio);
        assert!(probe.configured_via_mmio_bus);
        assert!(probe.queue_notified);
        assert_eq!(probe.queue_notify_value, Some(0));
        assert!(probe.completed_via_device_bus);
        assert!(probe.completed);
        assert_eq!(probe.descriptor_index, Some(0));
        assert_eq!(probe.request_type, Some(VIRTIO_BLK_T_IN));
        assert_eq!(probe.sector, Some(VIRTIO_BLOCK_SYNTHETIC_REQUEST_SECTOR));
        assert_eq!(probe.byte_offset, Some(0xe00));
        assert_eq!(
            probe.data_bytes,
            Some(VIRTIO_BLOCK_SYNTHETIC_REQUEST_DATA_BYTES)
        );
        assert_eq!(
            probe.data_prefix,
            vec![0xa0, 0xa1, 0xa2, 0xa3, 0xa4, 0xa5, 0xa6, 0xa7]
        );
        assert_eq!(probe.status, Some(VIRTIO_BLK_S_OK));
        assert_eq!(probe.used_index, Some(1));
        assert_eq!(probe.used_len, Some(513));
        assert_eq!(
            probe.interrupt_status,
            Some(VIRTIO_MMIO_INTERRUPT_USED_BUFFER_VALUE)
        );
        assert!(probe.blockers.is_empty());
        assert!(output.contains("VirtIO block file backing probe"));
        assert!(output.contains("QEMU: not used"));
        assert!(output.contains("Apple VZ: not used"));
        assert!(output.contains("HVF: not entered"));
        assert!(output.contains(
            "Guest execution: not entered; host file-backed VirtIO block descriptor chain"
        ));
        assert!(output.contains("Backing kind: host-file"));
        assert!(output.contains("Configured via MMIO: true"));
        assert!(output.contains("Configured via MMIO bus: true"));
        assert!(output.contains("Queue notified: true"));
        assert!(output.contains("Completed via device bus: true"));
        assert!(output.contains("Completed: true"));
        assert!(output.contains("Descriptor index: 0x0"));
        assert!(output.contains("Request type: 0x0"));
        assert!(output.contains("Sector: 0x7"));
        assert!(output.contains("Byte offset: 0xe00"));
        assert!(output.contains("Data bytes: 0x200"));
        assert!(output.contains("Data prefix: 0xa0a1a2a3a4a5a6a7"));
        assert!(output.contains("Status byte: 0x0"));
        assert!(output.contains("Used index: 0x1"));
        assert!(output.contains("Used length: 0x201"));
        assert!(output.contains("Interrupt status: 0x1"));
        assert!(output.contains("Blockers: none"));
        assert!(!output.contains("qemu-system"));
        assert!(!output.contains('%'));
    }

    #[test]
    fn virtio_block_writable_file_backing_probe_writes_flushes_and_persists() {
        let mut disk = vec![0_u8; (VIRTIO_BLOCK_SECTOR_BYTES as usize) * 16];
        let sector_start =
            (VIRTIO_BLOCK_SYNTHETIC_REQUEST_SECTOR * VIRTIO_BLOCK_SECTOR_BYTES) as usize;
        for offset in 0..VIRTIO_BLOCK_SYNTHETIC_REQUEST_DATA_BYTES as usize {
            disk[sector_start + offset] = 0xa0_u8.wrapping_add(offset as u8);
        }
        let path = std::env::temp_dir().join(format!(
            "bridgevm-hvf-writable-file-backed-{}-{}.img",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::write(&path, &disk).unwrap();

        let probe = probe_virtio_block_writable_file_backing(path.clone());
        let output = probe.render_text();
        let _ = std::fs::remove_file(&path);

        assert_eq!(probe.disk_path, path);
        assert_eq!(probe.backing_kind, "host-file-writable");
        assert!(probe.configured_via_mmio);
        assert!(probe.configured_via_mmio_bus);
        assert!(probe.queue_notified);
        assert_eq!(probe.queue_notify_value, Some(0));
        assert_eq!(
            probe.initial_read_prefix,
            vec![0xa0, 0xa1, 0xa2, 0xa3, 0xa4, 0xa5, 0xa6, 0xa7]
        );
        assert!(probe.write_completed);
        assert_eq!(probe.write_request_type, Some(VIRTIO_BLK_T_OUT));
        assert_eq!(
            probe.write_sector,
            Some(VIRTIO_BLOCK_SYNTHETIC_REQUEST_SECTOR)
        );
        assert_eq!(probe.write_byte_offset, Some(0xe00));
        assert_eq!(
            probe.write_data_bytes,
            Some(VIRTIO_BLOCK_SYNTHETIC_REQUEST_DATA_BYTES)
        );
        assert_eq!(
            probe.write_data_prefix,
            vec![0xe0, 0xe1, 0xe2, 0xe3, 0xe4, 0xe5, 0xe6, 0xe7]
        );
        assert_eq!(probe.write_status, Some(VIRTIO_BLK_S_OK));
        assert_eq!(probe.write_used_index, Some(2));
        assert_eq!(probe.write_used_len, Some(VIRTIO_BLOCK_STATUS_BYTES));
        assert!(probe.flush_completed);
        assert_eq!(probe.flush_request_type, Some(VIRTIO_BLK_T_FLUSH));
        assert_eq!(probe.flush_status, Some(VIRTIO_BLK_S_OK));
        assert_eq!(probe.flush_used_index, Some(3));
        assert_eq!(probe.flush_used_len, Some(VIRTIO_BLOCK_STATUS_BYTES));
        assert_eq!(
            probe.persisted_data_prefix,
            vec![0xe0, 0xe1, 0xe2, 0xe3, 0xe4, 0xe5, 0xe6, 0xe7]
        );
        assert_eq!(
            probe.interrupt_status,
            Some(VIRTIO_MMIO_INTERRUPT_USED_BUFFER_VALUE)
        );
        assert!(probe.blockers.is_empty());
        assert!(output.contains("VirtIO block writable file backing probe"));
        assert!(output.contains("QEMU: not used"));
        assert!(output.contains("Apple VZ: not used"));
        assert!(output.contains("HVF: not entered"));
        assert!(output.contains(
            "Guest execution: not entered; host file-backed VirtIO block write/flush persistence descriptor chain"
        ));
        assert!(output.contains("Backing kind: host-file-writable"));
        assert!(output.contains("Configured via MMIO: true"));
        assert!(output.contains("Configured via MMIO bus: true"));
        assert!(output.contains("Queue notified: true"));
        assert!(output.contains("Initial read data prefix: 0xa0a1a2a3a4a5a6a7"));
        assert!(output.contains("Write completed: true"));
        assert!(output.contains("Write request type: 0x1"));
        assert!(output.contains("Write sector: 0x7"));
        assert!(output.contains("Write byte offset: 0xe00"));
        assert!(output.contains("Write data bytes: 0x200"));
        assert!(output.contains("Write data prefix: 0xe0e1e2e3e4e5e6e7"));
        assert!(output.contains("Write status byte: 0x0"));
        assert!(output.contains("Write used index: 0x2"));
        assert!(output.contains("Write used length: 0x1"));
        assert!(output.contains("Flush completed: true"));
        assert!(output.contains("Flush request type: 0x4"));
        assert!(output.contains("Flush status byte: 0x0"));
        assert!(output.contains("Flush used index: 0x3"));
        assert!(output.contains("Flush used length: 0x1"));
        assert!(output.contains("Persisted data prefix: 0xe0e1e2e3e4e5e6e7"));
        assert!(output.contains("Interrupt status: 0x1"));
        assert!(output.contains("Blockers: none"));
        assert!(!output.contains("qemu-system"));
        assert!(!output.contains('%'));
    }

    #[test]
    fn virtio_block_iso_backing_probe_reads_from_read_only_iso() {
        let mut iso = vec![0_u8; (VIRTIO_BLOCK_SECTOR_BYTES as usize) * 16];
        let sector_start =
            (VIRTIO_BLOCK_SYNTHETIC_REQUEST_SECTOR * VIRTIO_BLOCK_SECTOR_BYTES) as usize;
        for offset in 0..VIRTIO_BLOCK_SYNTHETIC_REQUEST_DATA_BYTES as usize {
            iso[sector_start + offset] = 0xc0_u8.wrapping_add(offset as u8);
        }
        let path = std::env::temp_dir().join(format!(
            "bridgevm-hvf-iso-backed-{}-{}.iso",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::write(&path, &iso).unwrap();

        let probe = probe_virtio_block_iso_backing(path.clone());
        let output = probe.render_text();
        let _ = std::fs::remove_file(&path);

        assert_eq!(probe.iso_path, path);
        assert_eq!(probe.backing_kind, "host-iso-readonly");
        assert_eq!(probe.media_mode, "read-only");
        assert!(probe.configured_via_mmio);
        assert!(probe.configured_via_mmio_bus);
        assert!(probe.queue_notified);
        assert_eq!(probe.queue_notify_value, Some(0));
        assert!(probe.completed_via_device_bus);
        assert!(probe.completed);
        assert_eq!(probe.descriptor_index, Some(0));
        assert_eq!(probe.request_type, Some(VIRTIO_BLK_T_IN));
        assert_eq!(probe.sector, Some(VIRTIO_BLOCK_SYNTHETIC_REQUEST_SECTOR));
        assert_eq!(probe.byte_offset, Some(0xe00));
        assert_eq!(
            probe.data_bytes,
            Some(VIRTIO_BLOCK_SYNTHETIC_REQUEST_DATA_BYTES)
        );
        assert_eq!(
            probe.data_prefix,
            vec![0xc0, 0xc1, 0xc2, 0xc3, 0xc4, 0xc5, 0xc6, 0xc7]
        );
        assert_eq!(probe.status, Some(VIRTIO_BLK_S_OK));
        assert_eq!(probe.used_index, Some(1));
        assert_eq!(probe.used_len, Some(513));
        assert_eq!(
            probe.interrupt_status,
            Some(VIRTIO_MMIO_INTERRUPT_USED_BUFFER_VALUE)
        );
        assert!(probe.readonly_write_rejected);
        assert_eq!(probe.readonly_write_status, Some(VIRTIO_BLK_S_IOERR));
        assert_eq!(probe.readonly_write_used_index, Some(2));
        assert_eq!(
            probe.readonly_write_used_len,
            Some(VIRTIO_BLOCK_STATUS_BYTES)
        );
        assert_eq!(
            probe.readonly_write_interrupt_status,
            Some(VIRTIO_MMIO_INTERRUPT_USED_BUFFER_VALUE)
        );
        assert!(probe.blockers.is_empty());
        assert!(output.contains("VirtIO block ISO backing probe"));
        assert!(output.contains("QEMU: not used"));
        assert!(output.contains("Apple VZ: not used"));
        assert!(output.contains("HVF: not entered"));
        assert!(output.contains(
            "Guest execution: not entered; read-only ISO-backed VirtIO block descriptor chain"
        ));
        assert!(output.contains(&format!("ISO path: {}", path.display())));
        assert!(output.contains("Backing kind: host-iso-readonly"));
        assert!(output.contains("Media mode: read-only"));
        assert!(output.contains("Configured via MMIO: true"));
        assert!(output.contains("Configured via MMIO bus: true"));
        assert!(output.contains("Queue notified: true"));
        assert!(output.contains("Completed via device bus: true"));
        assert!(output.contains("Completed: true"));
        assert!(output.contains("Descriptor index: 0x0"));
        assert!(output.contains("Request type: 0x0"));
        assert!(output.contains("Sector: 0x7"));
        assert!(output.contains("Byte offset: 0xe00"));
        assert!(output.contains("Data bytes: 0x200"));
        assert!(output.contains("Data prefix: 0xc0c1c2c3c4c5c6c7"));
        assert!(output.contains("Status byte: 0x0"));
        assert!(output.contains("Used index: 0x1"));
        assert!(output.contains("Used length: 0x201"));
        assert!(output.contains("Interrupt status: 0x1"));
        assert!(output.contains("Read-only write rejected: true"));
        assert!(output.contains("Read-only write status byte: 0x1"));
        assert!(output.contains("Read-only write used index: 0x2"));
        assert!(output.contains("Read-only write used length: 0x1"));
        assert!(output.contains("Read-only write interrupt status: 0x1"));
        assert!(output.contains("Blockers: none"));
        assert!(!output.contains("qemu-system"));
        assert!(!output.contains('%'));
    }

    #[test]
    fn mmio_bus_reports_unmapped_access_as_unhandled() {
        let mut bus = MmioBus::default();
        bus.attach(Box::new(Pl011UartDevice::new(0x5000_0000, 0x90)));

        assert_eq!(
            bus.dispatch(MmioAccess::read(0x6000_0000, 8)),
            MmioAction::Unhandled
        );
    }

    #[test]
    fn mmio_rtc_device_probe_is_opt_in_and_qemu_free() {
        let probe = probe_hvf_mmio_rtc_device(false);
        let output = probe.render_text();

        assert!(!probe.allowed);
        assert!(!probe.attempted);
        assert!(!probe.vm_created);
        assert!(!probe.memory_mapped);
        assert!(!probe.vcpu_created);
        assert!(!probe.device_bus_created);
        assert_eq!(probe.device_bus_device_count, 0);
        assert!(!probe.first_run_attempted);
        assert!(!probe.rtc_exit_observed);
        assert!(!probe.rtc_handled_by_device);
        assert!(!probe.rtc_value_injected);
        assert!(!probe.second_run_attempted);
        assert!(!probe.continuation_exit_observed);
        assert!(output.contains("HVF MMIO RTC device probe"));
        assert!(output.contains("QEMU: not used"));
        assert!(output.contains("Apple VZ: not used"));
        assert!(output.contains("Guest execution: LDR RTC data register, then HVC"));
        assert!(output.contains("Device models: PL011 UART skeleton; PL031 RTC skeleton"));
        assert!(output.contains("Allowed: false"));
        assert!(output.contains("Attempted: false"));
        assert!(output.contains("Device bus created: false"));
        assert!(output.contains("Device bus device count: 0"));
        assert!(output.contains("RTC handled by device: false"));
        assert!(output.contains("RTC value injected: false"));
        assert!(output.contains("UART IPA: 0x50000000"));
        assert!(output.contains("RTC IPA: 0x50001000"));
        assert!(output.contains("RTC value: 0x20260618"));
        assert!(output.contains("First run status name: not attempted"));
        assert!(output.contains("Second run status name: not attempted"));
        assert!(!output.contains("qemu-system"));
        assert!(!output.contains('%'));
    }

    #[test]
    fn mmio_rtc_device_probe_render_records_multi_device_continuation() {
        let probe = HvfMmioRtcDeviceProbe {
            allowed: true,
            attempted: true,
            vm_created: true,
            memory_allocated: true,
            memory_mapped: true,
            vcpu_created: true,
            pc_set: true,
            cpsr_set: true,
            rtc_address_register_set: true,
            device_bus_created: true,
            device_bus_device_count: 2,
            first_run_attempted: true,
            rtc_exit_observed: true,
            rtc_handled_by_device: true,
            rtc_value_injected: true,
            pc_read_after_rtc_exit: true,
            pc_advanced: true,
            second_run_attempted: true,
            continuation_exit_observed: true,
            rtc_value_preserved: true,
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
            device_models: BOOT_MMIO_DEVICE_MODELS,
            code_ipa_start: 0x4000_0000,
            uart_ipa: 0x5000_0000,
            rtc_ipa: 0x5000_1000,
            bytes: 16 * 1024,
            instructions: "LDR X0, [X1]; HVC #0",
            rtc_value: 0x2026_0618,
            vm_create_status: Some(0),
            allocate_status: Some(0),
            map_status: Some(0),
            vcpu_create_status: Some(0),
            pc_set_status: Some(0),
            cpsr_set_status: Some(0),
            rtc_address_register_set_status: Some(0),
            first_run_status: Some(0),
            rtc_exit_reason: Some(1),
            rtc_exit_syndrome: Some(0x93c0_8006),
            rtc_exit_virtual_address: Some(0x5000_1000),
            rtc_exit_physical_address: Some(0x5000_1000),
            first_watchdog_cancel_status: None,
            rtc_value_set_status: Some(0),
            pc_read_status: Some(0),
            pc_after_rtc_exit: Some(0x4000_0000),
            pc_advance_status: Some(0),
            second_run_status: Some(0),
            continuation_exit_reason: Some(1),
            continuation_exit_syndrome: Some(0x5a00_0000),
            continuation_exit_virtual_address: Some(0),
            continuation_exit_physical_address: Some(0),
            second_watchdog_cancel_status: None,
            rtc_value_after_continue_status: Some(0),
            rtc_value_after_continue: Some(0x2026_0618),
            vcpu_destroy_status: Some(0),
            unmap_status: Some(0),
            vm_destroy_status: Some(0),
            deallocate_status: Some(0),
            blockers: Vec::new(),
        };
        let output = probe.render_text();

        assert!(output.contains("Allowed: true"));
        assert!(output.contains("Attempted: true"));
        assert!(output.contains("Device models: PL011 UART skeleton; PL031 RTC skeleton"));
        assert!(output.contains("Device bus created: true"));
        assert!(output.contains("Device bus device count: 2"));
        assert!(output.contains("RTC exit observed: true"));
        assert!(output.contains("RTC handled by device: true"));
        assert!(output.contains("RTC value injected: true"));
        assert!(output.contains("PC advanced: true"));
        assert!(output.contains("Continuation exit observed: true"));
        assert!(output.contains("RTC value preserved: true"));
        assert!(output.contains("RTC exit syndrome: 0x93c08006"));
        assert!(output.contains("RTC exit virtual address: 0x50001000"));
        assert!(output.contains("Continuation exit syndrome: 0x5a000000"));
        assert!(output.contains("RTC value after continue: 0x20260618"));
        assert!(output.contains("Blockers: none"));
        assert!(!output.contains('%'));
    }

    #[test]
    fn mmio_block_device_probe_is_opt_in_and_qemu_free() {
        let probe = probe_hvf_mmio_block_device(false);
        let output = probe.render_text();

        assert!(!probe.allowed);
        assert!(!probe.attempted);
        assert!(!probe.vm_created);
        assert!(!probe.memory_mapped);
        assert!(!probe.vcpu_created);
        assert!(!probe.device_bus_created);
        assert_eq!(probe.device_bus_device_count, 0);
        assert_eq!(probe.register_reads.len(), 4);
        assert!(probe.register_reads.iter().all(|read| !read.run_attempted));
        assert!(!probe.continuation_run_attempted);
        assert!(!probe.continuation_exit_observed);
        assert!(!probe.vendor_value_preserved);
        assert!(output.contains("HVF MMIO block device probe"));
        assert!(output.contains("QEMU: not used"));
        assert!(output.contains("Apple VZ: not used"));
        assert!(output.contains("Guest execution: LDR W0 VirtIO-MMIO identity registers, then HVC"));
        assert!(output.contains(
            "Device models: PL011 UART skeleton; PL031 RTC skeleton; VirtIO-MMIO block identity skeleton"
        ));
        assert!(output.contains("Allowed: false"));
        assert!(output.contains("Attempted: false"));
        assert!(output.contains("Device bus created: false"));
        assert!(output.contains("Device bus device count: 0"));
        assert!(output.contains("magic at 0x50002000: expected 0x74726976"));
        assert!(output.contains("version at 0x50002004: expected 0x2"));
        assert!(output.contains("device_id at 0x50002008: expected 0x2"));
        assert!(output.contains("vendor_id at 0x5000200c: expected 0x4252564d"));
        assert!(output.contains("Continuation exit observed: false"));
        assert!(output.contains("Vendor value preserved: false"));
        assert!(output.contains("Block IPA: 0x50002000"));
        assert!(output.contains("VirtIO magic value: 0x74726976"));
        assert!(output.contains("VirtIO version value: 0x2"));
        assert!(output.contains("VirtIO block device ID value: 0x2"));
        assert!(output.contains("VirtIO vendor ID value: 0x4252564d"));
        assert!(!output.contains("qemu-system"));
        assert!(!output.contains('%'));
    }

    #[test]
    fn mmio_block_queue_probe_is_opt_in_and_qemu_free() {
        let probe = probe_hvf_mmio_block_queue(false, None, None, None);
        let output = probe.render_text();

        assert!(!probe.allowed);
        assert!(!probe.attempted);
        assert!(!probe.vm_created);
        assert!(!probe.memory_mapped);
        assert!(!probe.vcpu_created);
        assert!(!probe.device_bus_created);
        assert_eq!(probe.device_bus_device_count, 0);
        assert_eq!(probe.steps.len(), 26);
        assert!(probe.steps.iter().all(|step| !step.run_attempted));
        assert!(!probe.continuation_run_attempted);
        assert!(!probe.continuation_exit_observed);
        assert!(!probe.capacity_high_value_preserved);
        assert_eq!(probe.block_backing_kind, "synthetic-sector-pattern");
        assert_eq!(probe.block_backing_path, None);
        assert!(!probe.request_ring_seeded);
        assert!(!probe.request_completed_after_notify);
        assert_eq!(probe.request_descriptor_index, None);
        assert_eq!(probe.request_sector, None);
        assert_eq!(probe.request_byte_offset, None);
        assert_eq!(probe.request_data_bytes, None);
        assert!(probe.request_data_prefix.is_empty());
        assert_eq!(probe.request_status, None);
        assert_eq!(probe.request_used_index, None);
        assert_eq!(probe.request_used_len, None);
        assert_eq!(probe.request_interrupt_status, None);
        assert!(!probe.write_completed_after_notify);
        assert_eq!(probe.write_request_type, None);
        assert_eq!(probe.write_sector, None);
        assert_eq!(probe.write_byte_offset, None);
        assert_eq!(probe.write_data_bytes, None);
        assert!(probe.write_data_prefix.is_empty());
        assert_eq!(probe.write_status, None);
        assert_eq!(probe.write_used_index, None);
        assert_eq!(probe.write_used_len, None);
        assert!(!probe.flush_completed_after_notify);
        assert_eq!(probe.flush_request_type, None);
        assert_eq!(probe.flush_status, None);
        assert_eq!(probe.flush_used_index, None);
        assert_eq!(probe.flush_used_len, None);
        assert!(probe.persisted_data_prefix.is_empty());
        assert!(output.contains("HVF MMIO block queue/config/address/notify probe"));
        assert!(output.contains("QEMU: not used"));
        assert!(output.contains("Apple VZ: not used"));
        assert!(output.contains(
            "Guest execution: VirtIO-MMIO feature, queue, ring address, notify, status, and capacity registers, then HVC"
        ));
        assert!(output.contains(
            "Device models: PL011 UART skeleton; PL031 RTC skeleton; VirtIO-MMIO block identity skeleton; VirtIO-MMIO block queue/config/address/notify skeleton"
        ));
        assert!(output.contains("Allowed: false"));
        assert!(output.contains("Attempted: false"));
        assert!(output.contains("Device bus created: false"));
        assert!(output.contains("Device bus device count: 0"));
        assert!(output.contains("read device_features at 0x50002010: expected=0x0"));
        assert!(output
            .contains("write driver_features at 0x50002020: expected=not observed, write=0x0"));
        assert!(output.contains("write status_ack at 0x50002070: expected=not observed, write=0x1"));
        assert!(
            output.contains("write status_driver at 0x50002070: expected=not observed, write=0x3")
        );
        assert!(output
            .contains("write status_features_ok at 0x50002070: expected=not observed, write=0xb"));
        assert!(
            output.contains("write queue_select at 0x50002030: expected=not observed, write=0x0")
        );
        assert!(output.contains("read queue_num_max at 0x50002034: expected=0x80"));
        assert!(output.contains("write queue_num at 0x50002038: expected=not observed, write=0x8"));
        assert!(output.contains(
            "write queue_desc_low at 0x50002080: expected=not observed, write=0x40001000"
        ));
        assert!(output
            .contains("write queue_desc_high at 0x50002084: expected=not observed, write=0x0"));
        assert!(output.contains(
            "write queue_driver_low at 0x50002090: expected=not observed, write=0x40002000"
        ));
        assert!(output
            .contains("write queue_driver_high at 0x50002094: expected=not observed, write=0x0"));
        assert!(output.contains(
            "write queue_device_low at 0x500020a0: expected=not observed, write=0x40003000"
        ));
        assert!(output
            .contains("write queue_device_high at 0x500020a4: expected=not observed, write=0x0"));
        assert!(
            output.contains("write queue_ready at 0x50002044: expected=not observed, write=0x1")
        );
        assert!(output
            .contains("write status_driver_ok at 0x50002070: expected=not observed, write=0xf"));
        assert!(output.contains("read status at 0x50002070: expected=0xf"));
        assert!(
            output.contains("write queue_notify at 0x50002050: expected=not observed, write=0x0")
        );
        assert!(output.contains("read queue_ready at 0x50002044: expected=0x1"));
        assert!(output.contains("read queue_desc_low at 0x50002080: expected=0x40001000"));
        assert!(output.contains("read queue_driver_low at 0x50002090: expected=0x40002000"));
        assert!(output.contains("read queue_device_low at 0x500020a0: expected=0x40003000"));
        assert!(output.contains("read interrupt_status at 0x50002060: expected=0x1"));
        assert!(output.contains("read config_generation at 0x500020fc: expected=0x0"));
        assert!(output.contains("read capacity_low at 0x50002100: expected=0x4000"));
        assert!(output.contains("read capacity_high at 0x50002104: expected=0x0"));
        assert!(output.contains("Continuation exit observed: false"));
        assert!(output.contains("Capacity high value preserved: false"));
        assert!(output.contains("Block IPA: 0x50002000"));
        assert!(output.contains(
            "Instructions: LDR/STR W0 VirtIO-MMIO queue/config/address/notify registers; HVC #0"
        ));
        assert!(output.contains("Queue num max value: 0x80"));
        assert!(output.contains("Queue descriptor address: 0x40001000"));
        assert!(output.contains("Queue driver address: 0x40002000"));
        assert!(output.contains("Queue device address: 0x40003000"));
        assert!(output.contains("Queue notify value: 0x0"));
        assert!(output.contains("Interrupt status value: 0x1"));
        assert!(output.contains("Block backing kind: synthetic-sector-pattern"));
        assert!(output.contains("Block backing path: not observed"));
        assert!(output.contains("Request ring seeded: false"));
        assert!(output.contains("Request completed after notify: false"));
        assert!(output.contains("Request descriptor index: not observed"));
        assert!(output.contains("Request sector: not observed"));
        assert!(output.contains("Request byte offset: not observed"));
        assert!(output.contains("Request data bytes: not observed"));
        assert!(output.contains("Request data prefix: not observed"));
        assert!(output.contains("Request status byte: not observed"));
        assert!(output.contains("Request used index: not observed"));
        assert!(output.contains("Request used length: not observed"));
        assert!(output.contains("Request interrupt status: not observed"));
        assert!(output.contains("Write completed after notify: false"));
        assert!(output.contains("Write request type: not observed"));
        assert!(output.contains("Write sector: not observed"));
        assert!(output.contains("Write byte offset: not observed"));
        assert!(output.contains("Write data bytes: not observed"));
        assert!(output.contains("Write data prefix: not observed"));
        assert!(output.contains("Write status byte: not observed"));
        assert!(output.contains("Write used index: not observed"));
        assert!(output.contains("Write used length: not observed"));
        assert!(output.contains("Flush completed after notify: false"));
        assert!(output.contains("Flush request type: not observed"));
        assert!(output.contains("Flush status byte: not observed"));
        assert!(output.contains("Flush used index: not observed"));
        assert!(output.contains("Flush used length: not observed"));
        assert!(output.contains("Persisted data prefix: not observed"));
        assert!(output.contains("Status value: 0xf"));
        assert!(output.contains("Capacity sectors: 0x4000"));
        assert!(!output.contains("qemu-system"));
        assert!(!output.contains('%'));
    }

    #[test]
    fn mmio_block_queue_probe_reports_file_backing_without_live_opt_in() {
        let disk_path = PathBuf::from("/tmp/bridgevm-live-block.img");
        let probe = probe_hvf_mmio_block_queue(false, Some(disk_path.clone()), None, None);
        let output = probe.render_text();

        assert!(!probe.allowed);
        assert!(!probe.attempted);
        assert_eq!(probe.block_backing_kind, "host-file");
        assert_eq!(probe.block_backing_path, Some(disk_path.clone()));
        assert!(!probe.request_completed_after_notify);
        assert_eq!(probe.request_byte_offset, None);
        assert!(output.contains("Block backing kind: host-file"));
        assert!(output.contains(&format!("Block backing path: {}", disk_path.display())));
        assert!(output.contains("Request byte offset: not observed"));
        assert!(!output.contains("qemu-system"));
        assert!(!output.contains('%'));
    }

    #[test]
    fn mmio_block_queue_probe_reports_iso_backing_without_live_opt_in() {
        let iso_path = PathBuf::from("/tmp/Win11_Arm64.iso");
        let probe = probe_hvf_mmio_block_queue(false, None, Some(iso_path.clone()), None);
        let output = probe.render_text();

        assert!(!probe.allowed);
        assert!(!probe.attempted);
        assert_eq!(probe.block_backing_kind, "host-iso-readonly");
        assert_eq!(probe.block_backing_path, Some(iso_path.clone()));
        assert!(!probe.request_completed_after_notify);
        assert_eq!(probe.request_byte_offset, None);
        assert!(output.contains("Block backing kind: host-iso-readonly"));
        assert!(output.contains(&format!("Block backing path: {}", iso_path.display())));
        assert!(output.contains("Request byte offset: not observed"));
        assert!(!output.contains("qemu-system"));
        assert!(!output.contains('%'));
    }

    #[test]
    fn mmio_block_queue_probe_reports_writable_file_backing_without_live_opt_in() {
        let disk_path = PathBuf::from("/tmp/bridgevm-writable-live-block.img");
        let probe = probe_hvf_mmio_block_queue(false, None, None, Some(disk_path.clone()));
        let output = probe.render_text();

        assert!(!probe.allowed);
        assert!(!probe.attempted);
        assert_eq!(probe.block_backing_kind, "host-file-writable");
        assert_eq!(probe.block_backing_path, Some(disk_path.clone()));
        assert!(!probe.request_completed_after_notify);
        assert!(!probe.write_completed_after_notify);
        assert!(!probe.flush_completed_after_notify);
        assert_eq!(probe.request_byte_offset, None);
        assert_eq!(probe.write_byte_offset, None);
        assert!(probe.persisted_data_prefix.is_empty());
        assert!(output.contains("Block backing kind: host-file-writable"));
        assert!(output.contains(&format!("Block backing path: {}", disk_path.display())));
        assert!(output.contains("Request byte offset: not observed"));
        assert!(output.contains("Write completed after notify: false"));
        assert!(output.contains("Flush completed after notify: false"));
        assert!(output.contains("Persisted data prefix: not observed"));
        assert!(!output.contains("qemu-system"));
        assert!(!output.contains('%'));
    }
}
