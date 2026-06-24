#[path = "windows_arm_xhci_hid_boot_key_probe_xhci.rs"]
mod xhci_probe;

#[cfg(test)]
#[path = "windows_arm_xhci_hid_boot_key_probe_tests.rs"]
mod tests;

use xhci_probe::{
    run_windows_arm_xhci_hid_probe, WINDOWS_ARM_XHCI_HID_BOOT_KEY_USAGE_ID,
    WINDOWS_ARM_XHCI_HID_BOOT_KEY_USAGE_PAGE,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsArmXhciHidBootKeyReportProbe {
    pub qemu_used: bool,
    pub apple_vz_used: bool,
    pub hvf_entered: bool,
    pub windows_boot_claimed: bool,
    pub usage_page: u16,
    pub usage_id: u16,
    pub key_report: [u8; 8],
    pub release_report: [u8; 8],
    pub transfer_events: usize,
    pub blockers: Vec<String>,
}

impl WindowsArmXhciHidBootKeyReportProbe {
    pub fn render_text(&self) -> String {
        let mut output = String::new();
        output.push_str("Windows 11 Arm HVF xHCI HID boot-key report probe\n");
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
        output.push_str(if self.windows_boot_claimed {
            "Windows boot: claimed\n"
        } else {
            "Windows boot: not claimed\n"
        });
        output.push_str(&format!("Usage page: 0x{:02x}\n", self.usage_page));
        output.push_str(&format!("Usage ID: 0x{:02x}\n", self.usage_id));
        output.push_str(&format!(
            "Key report: {}\n",
            render_hid_boot_keyboard_report(self.key_report)
        ));
        output.push_str(&format!(
            "Release report: {}\n",
            render_hid_boot_keyboard_report(self.release_report)
        ));
        output.push_str(&format!("Transfer events: {}\n", self.transfer_events));
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

pub fn probe_windows_11_arm_xhci_hid_boot_key_report() -> WindowsArmXhciHidBootKeyReportProbe {
    let outcome = run_windows_arm_xhci_hid_probe();

    WindowsArmXhciHidBootKeyReportProbe {
        qemu_used: false,
        apple_vz_used: false,
        hvf_entered: false,
        windows_boot_claimed: false,
        usage_page: WINDOWS_ARM_XHCI_HID_BOOT_KEY_USAGE_PAGE,
        usage_id: WINDOWS_ARM_XHCI_HID_BOOT_KEY_USAGE_ID,
        key_report: outcome.key_report,
        release_report: outcome.release_report,
        transfer_events: outcome.transfer_events,
        blockers: outcome.blockers,
    }
}

fn render_hid_boot_keyboard_report(report: [u8; 8]) -> String {
    report
        .into_iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}
