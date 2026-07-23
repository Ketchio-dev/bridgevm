//! Split out of windows_arm.rs by responsibility.

use super::*;
use crate::*;

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
