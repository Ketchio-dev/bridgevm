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
