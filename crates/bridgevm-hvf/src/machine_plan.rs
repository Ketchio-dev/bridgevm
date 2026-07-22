//! Declarative machine plan for the Windows 11 Arm HVF path.
//!
//! Moved verbatim out of the legacy probe monolith; the crate root re-exports
//! every item so the public surface is unchanged.

use std::path::PathBuf;

use crate::{
    query_hvf_host_capabilities, render_optional_u32, windows_11_arm_no_qemu_vmm_gates,
    HvfHostCapabilities, WindowsArmVmmGate, WindowsArmVmmGateStatus, WINDOWS_ARM_DEVICE_MMIO_BYTES,
    WINDOWS_ARM_DEVICE_MMIO_IPA, WINDOWS_ARM_GUEST_RAM_IPA, WINDOWS_ARM_UEFI_CODE_IPA,
    WINDOWS_ARM_UEFI_PFLASH_BYTES,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HvfMachinePlanOptions {
    pub installer: Option<PathBuf>,
    pub memory_gib: u32,
    pub vcpu_count: u8,
}

impl Default for HvfMachinePlanOptions {
    fn default() -> Self {
        Self {
            installer: None,
            memory_gib: 6,
            vcpu_count: 4,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HvfMemoryRegionPlan {
    pub name: &'static str,
    pub start: u64,
    pub size: u64,
    pub detail: &'static str,
}

impl HvfMemoryRegionPlan {
    pub fn end_exclusive(&self) -> u64 {
        self.start.saturating_add(self.size)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HvfVcpuLifecyclePlan {
    pub vcpu_count: u8,
    pub create_destroy: WindowsArmVmmGateStatus,
    pub run_loop: WindowsArmVmmGateStatus,
    pub exit_handling: WindowsArmVmmGateStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HvfDevicePlan {
    pub name: &'static str,
    pub status: WindowsArmVmmGateStatus,
    pub detail: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HvfMachinePlan {
    pub engine: &'static str,
    pub substrate: &'static str,
    pub guest: &'static str,
    pub installer: Option<PathBuf>,
    pub qemu_used: bool,
    pub host: HvfHostCapabilities,
    pub memory_gib: u32,
    pub memory_regions: Vec<HvfMemoryRegionPlan>,
    pub vcpu_lifecycle: HvfVcpuLifecyclePlan,
    pub devices: Vec<HvfDevicePlan>,
    pub gates: Vec<WindowsArmVmmGate>,
}

impl HvfMachinePlan {
    pub fn overall_status(&self) -> WindowsArmVmmGateStatus {
        if self
            .gates
            .iter()
            .any(|gate| gate.status == WindowsArmVmmGateStatus::Blocked)
            || self
                .devices
                .iter()
                .any(|device| device.status == WindowsArmVmmGateStatus::Blocked)
            || self.vcpu_lifecycle.run_loop == WindowsArmVmmGateStatus::Blocked
            || self.vcpu_lifecycle.exit_handling == WindowsArmVmmGateStatus::Blocked
        {
            WindowsArmVmmGateStatus::Blocked
        } else if self
            .gates
            .iter()
            .any(|gate| gate.status == WindowsArmVmmGateStatus::Research)
            || self
                .devices
                .iter()
                .any(|device| device.status == WindowsArmVmmGateStatus::Research)
            || self.vcpu_lifecycle.create_destroy == WindowsArmVmmGateStatus::Research
        {
            WindowsArmVmmGateStatus::Research
        } else {
            WindowsArmVmmGateStatus::Pass
        }
    }

    pub fn render_text(&self) -> String {
        let mut output = String::new();
        output.push_str("Windows 11 Arm HVF machine plan\n");
        output.push_str(&format!("Engine: {}\n", self.engine));
        output.push_str(&format!("Substrate: {}\n", self.substrate));
        output.push_str(&format!("Guest: {}\n", self.guest));
        output.push_str(&format!(
            "Installer: {}\n",
            self.installer
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "not provided".to_string())
        ));
        output.push_str(if self.qemu_used {
            "QEMU: used\n"
        } else {
            "QEMU: not used\n"
        });
        output.push_str(&format!("Host: {}\n", self.host.host));
        output.push_str(&format!("Host HVF available: {}\n", self.host.available));
        output.push_str(&format!(
            "IPA bits: default={}, max={}\n",
            render_optional_u32(self.host.default_ipa_bits),
            render_optional_u32(self.host.max_ipa_bits)
        ));
        output.push_str(&format!("Memory: {} GiB\n", self.memory_gib));
        output.push_str("Memory map:\n");
        for region in &self.memory_regions {
            output.push_str(&format!(
                "- {}: {:#014x}..{:#014x} - {}\n",
                region.name,
                region.start,
                region.end_exclusive(),
                region.detail
            ));
        }
        output.push_str("vCPU lifecycle:\n");
        output.push_str(&format!("- count: {}\n", self.vcpu_lifecycle.vcpu_count));
        output.push_str(&format!(
            "- create/destroy: {}\n",
            self.vcpu_lifecycle.create_destroy.as_str()
        ));
        output.push_str(&format!(
            "- run loop: {}\n",
            self.vcpu_lifecycle.run_loop.as_str()
        ));
        output.push_str(&format!(
            "- exit handling: {}\n",
            self.vcpu_lifecycle.exit_handling.as_str()
        ));
        output.push_str("Devices:\n");
        for device in &self.devices {
            output.push_str(&format!(
                "- {}: {} - {}\n",
                device.status.as_str(),
                device.name,
                device.detail
            ));
        }
        if self.host.blockers.is_empty() {
            output.push_str("Host blockers: none\n");
        } else {
            output.push_str("Host blockers:\n");
            for blocker in &self.host.blockers {
                output.push_str(&format!("- {blocker}\n"));
            }
        }
        output.push_str(&format!("Overall: {}\n", self.overall_status().as_str()));
        output
    }
}

pub fn plan_windows_11_arm_hvf_machine(options: HvfMachinePlanOptions) -> HvfMachinePlan {
    build_windows_11_arm_hvf_machine_plan(options, query_hvf_host_capabilities())
}

pub fn build_windows_11_arm_hvf_machine_plan(
    options: HvfMachinePlanOptions,
    host: HvfHostCapabilities,
) -> HvfMachinePlan {
    let memory_bytes = u64::from(options.memory_gib).saturating_mul(1024 * 1024 * 1024);
    HvfMachinePlan {
        engine: "BridgeVM HVF",
        substrate: "Apple Hypervisor.framework",
        guest: "Windows 11 Arm",
        installer: options.installer,
        qemu_used: false,
        memory_gib: options.memory_gib,
        memory_regions: vec![
            HvfMemoryRegionPlan {
                name: "guest RAM",
                start: WINDOWS_ARM_GUEST_RAM_IPA,
                size: memory_bytes,
                detail: "contiguous AArch64 RAM window planned for Windows",
            },
            HvfMemoryRegionPlan {
                name: "firmware pflash",
                start: WINDOWS_ARM_UEFI_CODE_IPA,
                size: WINDOWS_ARM_UEFI_PFLASH_BYTES,
                detail: "reserved for BridgeVM-owned AArch64 UEFI code and variable pflash slots",
            },
            HvfMemoryRegionPlan {
                name: "device MMIO",
                start: WINDOWS_ARM_DEVICE_MMIO_IPA,
                size: WINDOWS_ARM_DEVICE_MMIO_BYTES,
                detail: "reserved for block, network, display, input, TPM, and entropy devices",
            },
        ],
        vcpu_lifecycle: HvfVcpuLifecyclePlan {
            vcpu_count: options.vcpu_count,
            create_destroy: if host.available {
                WindowsArmVmmGateStatus::Research
            } else {
                WindowsArmVmmGateStatus::Blocked
            },
            run_loop: WindowsArmVmmGateStatus::Blocked,
            exit_handling: WindowsArmVmmGateStatus::Blocked,
        },
        devices: windows_11_arm_hvf_machine_devices(),
        gates: windows_11_arm_no_qemu_vmm_gates(),
        host,
    }
}

pub fn windows_11_arm_hvf_machine_devices() -> Vec<HvfDevicePlan> {
    vec![
        HvfDevicePlan {
            name: "AArch64 UEFI firmware",
            status: WindowsArmVmmGateStatus::Research,
            detail: "Firmware FD/vars pflash handoff, pflash memory-image loading, opt-in HVF pflash map/unmap, and opt-in reset-vector first-entry are proven; Windows Boot Manager handoff is not implemented.",
        },
        HvfDevicePlan {
            name: "firmware UART and RTC skeletons",
            status: WindowsArmVmmGateStatus::Pass,
            detail: "PL011 UART and PL031 RTC MMIO skeletons have opt-in HVF live probes through the BridgeVM device bus.",
        },
        HvfDevicePlan {
            name: "read-only installer media",
            status: WindowsArmVmmGateStatus::Pass,
            detail: "VirtIO-MMIO ISO-backed reads and read-only write rejection are proven in metadata-safe probes, with signed opt-in --iso queue_notify completion through HVF.",
        },
        HvfDevicePlan {
            name: "system boot disk",
            status: WindowsArmVmmGateStatus::Blocked,
            detail: "A metadata-safe writable host-file sector write/flush/reopen persistence boundary exists, and a sparse raw GPT/ESP/MSR/Windows layout probe can create and verify the boot-disk boundary; firmware handoff, partition install state, and reboot persistence are not implemented.",
        },
        HvfDevicePlan {
            name: "network adapter",
            status: WindowsArmVmmGateStatus::Blocked,
            detail: "No Windows-compatible paravirtual or emulated NIC exists in the HVF engine.",
        },
        HvfDevicePlan {
            name: "TPM and Secure Boot",
            status: WindowsArmVmmGateStatus::Blocked,
            detail: "Windows 11 Arm requires a trusted boot story before installation is viable.",
        },
        HvfDevicePlan {
            name: "display and input",
            status: WindowsArmVmmGateStatus::Blocked,
            detail: "Metal presentation, pointer, keyboard, and resize plumbing are not implemented.",
        },
        HvfDevicePlan {
            name: "Windows integration tools",
            status: WindowsArmVmmGateStatus::Blocked,
            detail: "Clipboard, folders, drag/drop, and app integration need Windows guest services/drivers.",
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_11_arm_hvf_machine_plan_is_blocked_and_qemu_free() {
        let plan = build_windows_11_arm_hvf_machine_plan(
            HvfMachinePlanOptions {
                installer: Some(PathBuf::from("ISO/Win11_25H2_English_Arm64_v2.iso")),
                memory_gib: 8,
                vcpu_count: 6,
            },
            HvfHostCapabilities {
                available: true,
                host: "macos-aarch64",
                default_ipa_bits: Some(36),
                max_ipa_bits: Some(40),
                el2_supported: Some(true),
                blockers: Vec::new(),
            },
        );

        assert_eq!(plan.engine, "BridgeVM HVF");
        assert!(!plan.qemu_used);
        assert_eq!(plan.memory_gib, 8);
        assert_eq!(plan.vcpu_lifecycle.vcpu_count, 6);
        assert_eq!(
            plan.vcpu_lifecycle.create_destroy,
            WindowsArmVmmGateStatus::Research
        );
        assert_eq!(plan.overall_status(), WindowsArmVmmGateStatus::Blocked);
        assert!(
            plan.devices
                .iter()
                .any(|device| device.name == "TPM and Secure Boot"
                    && device.status == WindowsArmVmmGateStatus::Blocked),
            "Windows 11 Arm machine plan must keep TPM/Secure Boot as a visible blocker"
        );
        assert!(
            plan.devices
                .iter()
                .any(|device| device.name == "read-only installer media"
                    && device.status == WindowsArmVmmGateStatus::Pass),
            "Windows 11 Arm machine plan must show read-only installer media as a proven boundary"
        );
        assert!(
            plan.devices
                .iter()
                .any(|device| device.name == "system boot disk"
                    && device.status == WindowsArmVmmGateStatus::Blocked),
            "Windows 11 Arm machine plan must keep persistent boot disk lifecycle blocked"
        );
    }

    #[test]
    fn windows_11_arm_hvf_machine_plan_renders_cli_contract() {
        let plan = build_windows_11_arm_hvf_machine_plan(
            HvfMachinePlanOptions {
                installer: Some(PathBuf::from("ISO/Win11_25H2_English_Arm64_v2.iso")),
                memory_gib: 6,
                vcpu_count: 4,
            },
            HvfHostCapabilities {
                available: false,
                host: "unsupported",
                default_ipa_bits: None,
                max_ipa_bits: None,
                el2_supported: None,
                blockers: vec!["unsupported host".to_string()],
            },
        );
        let output = plan.render_text();

        assert!(output.contains("Windows 11 Arm HVF machine plan"));
        assert!(output.contains("Engine: BridgeVM HVF"));
        assert!(output.contains("Substrate: Apple Hypervisor.framework"));
        assert!(output.contains("Installer: ISO/Win11_25H2_English_Arm64_v2.iso"));
        assert!(output.contains("QEMU: not used"));
        assert!(output.contains("Host: unsupported"));
        assert!(output.contains("IPA bits: default=unknown, max=unknown"));
        assert!(output.contains("Memory: 6 GiB"));
        assert!(output.contains("Memory map:"));
        assert!(output.contains("vCPU lifecycle:"));
        assert!(output.contains("- count: 4"));
        assert!(output.contains("Devices:"));
        assert!(output.contains("firmware UART and RTC skeletons"));
        assert!(output.contains("read-only installer media"));
        assert!(output.contains("ISO-backed reads and read-only write rejection"));
        assert!(output.contains("system boot disk"));
        assert!(
            output.contains("writable host-file sector write/flush/reopen persistence boundary")
        );
        assert!(output.contains("sparse raw GPT/ESP/MSR/Windows layout probe"));
        assert!(output.contains("firmware handoff"));
        assert!(output.contains("TPM and Secure Boot"));
        assert!(output.contains("Overall: blocked"));
        assert!(!output.contains("qemu-system"));
        assert!(!output.contains('%'));
    }
}
