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
mod windows_arm;

// Glob re-export preserves each moved item's original visibility, so the
// public surface is byte-identical and crate-internal helpers stay internal.
pub use windows_arm::*;

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
