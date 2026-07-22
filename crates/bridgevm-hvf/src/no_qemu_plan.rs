//! Product gates and the declarative no-QEMU plan for the Windows 11 Arm path.
//!
//! The crate root re-exports every item here, so the public CLI-facing surface
//! (`plan_windows_11_arm_no_qemu`, `windows_11_arm_no_qemu_vmm_gates`, and the
//! gate/plan types) is unchanged.

use std::path::PathBuf;

use crate::WindowsArmVmmGateStatus;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowsArmVmmGate {
    pub name: &'static str,
    pub status: WindowsArmVmmGateStatus,
    pub detail: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsArmNoQemuPlan {
    pub engine: &'static str,
    pub substrate: &'static str,
    pub guest: &'static str,
    pub installer: Option<PathBuf>,
    pub qemu_used: bool,
    pub gates: Vec<WindowsArmVmmGate>,
}

impl WindowsArmNoQemuPlan {
    pub fn overall_status(&self) -> WindowsArmVmmGateStatus {
        if self
            .gates
            .iter()
            .any(|gate| gate.status == WindowsArmVmmGateStatus::Blocked)
        {
            WindowsArmVmmGateStatus::Blocked
        } else if self
            .gates
            .iter()
            .any(|gate| gate.status == WindowsArmVmmGateStatus::Research)
        {
            WindowsArmVmmGateStatus::Research
        } else {
            WindowsArmVmmGateStatus::Pass
        }
    }

    pub fn render_text(&self) -> String {
        let mut output = String::new();
        output.push_str("Windows 11 Arm no-QEMU HVF plan\n");
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
        output.push_str(&format!("Overall: {}\n", self.overall_status().as_str()));
        output.push_str("Gates:\n");
        for gate in &self.gates {
            output.push_str(&format!(
                "- {}: {} - {}\n",
                gate.status.as_str(),
                gate.name,
                gate.detail
            ));
        }
        output
    }
}

pub fn plan_windows_11_arm_no_qemu(installer: Option<PathBuf>) -> WindowsArmNoQemuPlan {
    WindowsArmNoQemuPlan {
        engine: "BridgeVM HVF",
        substrate: "Apple Hypervisor.framework",
        guest: "Windows 11 Arm",
        installer,
        qemu_used: false,
        gates: windows_11_arm_no_qemu_vmm_gates(),
    }
}

/// Product gates for the Parallels-like Windows 11 Arm path.
///
/// This is intentionally separate from the QEMU/HVF Compatibility path. QEMU can
/// prove installer reachability, but it is not the product architecture that can
/// make Windows feel lightweight on macOS.
pub fn windows_11_arm_no_qemu_vmm_gates() -> Vec<WindowsArmVmmGate> {
    vec![
        WindowsArmVmmGate {
            name: "Apple Hypervisor.framework host boundary",
            status: if cfg!(target_os = "macos") {
                WindowsArmVmmGateStatus::Pass
            } else {
                WindowsArmVmmGateStatus::Blocked
            },
            detail: "HVF is the CPU virtualization substrate; BridgeVM still needs its own VMM above it.",
        },
        WindowsArmVmmGate {
            name: "AArch64 vCPU lifecycle",
            status: WindowsArmVmmGateStatus::Research,
            detail: "Generic create/run/exit probes and a Windows UEFI reset-vector first-entry probe exist, but a sustained firmware/OS run loop is not implemented.",
        },
        WindowsArmVmmGate {
            name: "UEFI and boot device model",
            status: WindowsArmVmmGateStatus::Blocked,
            detail: "BridgeVM can validate AArch64 UEFI FD/vars handoff metadata, map pflash into HVF, and enter the reset vector once, but UEFI Boot Manager and boot-services handoff are not implemented.",
        },
        WindowsArmVmmGate {
            name: "Storage and networking devices",
            status: WindowsArmVmmGateStatus::Blocked,
            detail: "BridgeVM-owned installer media reads and a GPT/ESP/MSR/Windows boot-disk layout boundary exist, but installed Windows persistence semantics and networking do not.",
        },
        WindowsArmVmmGate {
            name: "TPM and Secure Boot",
            status: WindowsArmVmmGateStatus::Blocked,
            detail: "Windows 11 requirements are not satisfied by the custom VMM path yet.",
        },
        WindowsArmVmmGate {
            name: "Display and input pipeline",
            status: WindowsArmVmmGateStatus::Blocked,
            detail: "No BridgeVM-owned Windows display/input device or Metal presentation path exists yet.",
        },
        WindowsArmVmmGate {
            name: "Windows guest tools and drivers",
            status: WindowsArmVmmGateStatus::Blocked,
            detail: "No Windows service, installer, or signed drivers are available for this path.",
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_11_arm_no_qemu_path_is_not_ready() {
        let gates = windows_11_arm_no_qemu_vmm_gates();

        assert!(gates.iter().any(|gate| {
            gate.name == "Apple Hypervisor.framework host boundary"
                && matches!(
                    gate.status,
                    WindowsArmVmmGateStatus::Pass | WindowsArmVmmGateStatus::Blocked
                )
        }));
        assert!(
            gates
                .iter()
                .any(|gate| gate.name == "UEFI and boot device model"
                    && gate.status == WindowsArmVmmGateStatus::Blocked),
            "custom Windows Arm VMM must not be reported ready while boot devices are missing"
        );
        assert!(
            gates
                .iter()
                .any(|gate| gate.name == "Windows guest tools and drivers"
                    && gate.status == WindowsArmVmmGateStatus::Blocked),
            "Parallels-like Windows usability requires guest tooling"
        );
    }

    #[test]
    fn windows_11_arm_no_qemu_plan_is_blocked_and_does_not_use_qemu() {
        let plan =
            plan_windows_11_arm_no_qemu(Some(PathBuf::from("ISO/Win11_25H2_English_Arm64_v2.iso")));

        assert_eq!(plan.engine, "BridgeVM HVF");
        assert_eq!(plan.substrate, "Apple Hypervisor.framework");
        assert_eq!(plan.guest, "Windows 11 Arm");
        assert!(!plan.qemu_used);
        assert_eq!(plan.overall_status(), WindowsArmVmmGateStatus::Blocked);
        assert_eq!(
            plan.installer.as_deref(),
            Some(std::path::Path::new("ISO/Win11_25H2_English_Arm64_v2.iso"))
        );
    }

    #[test]
    fn windows_11_arm_no_qemu_plan_renders_cli_contract() {
        let plan =
            plan_windows_11_arm_no_qemu(Some(PathBuf::from("ISO/Win11_25H2_English_Arm64_v2.iso")));
        let output = plan.render_text();

        assert!(output.contains("Windows 11 Arm no-QEMU HVF plan"));
        assert!(output.contains("Engine: BridgeVM HVF"));
        assert!(output.contains("Substrate: Apple Hypervisor.framework"));
        assert!(output.contains("Installer: ISO/Win11_25H2_English_Arm64_v2.iso"));
        assert!(output.contains("QEMU: not used"));
        assert!(output.contains("Overall: blocked"));
        assert!(!output.contains("qemu-system"));
        assert!(!output.contains('%'));
    }
}
