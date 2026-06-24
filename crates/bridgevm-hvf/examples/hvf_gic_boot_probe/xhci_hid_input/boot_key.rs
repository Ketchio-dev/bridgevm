use bridgevm_hvf::platform_virt::VirtPlatform;

use super::marker::{MarkerEnvError, ProbeMarker, MARKER_MAX_BYTES};
use super::report_text::{contains_bytes, format_report};

const CD_PROMPT_MARKER: &[u8] = b"Press any key to boot from CD or DVD";
const HID_BOOT_KEYBOARD_USAGE_SPACE: u8 = 0x2c;
const HID_BOOT_KEYBOARD_SPACE_REPORT: [u8; 8] = [0, 0, 0x2c, 0, 0, 0, 0, 0];
const HID_BOOT_KEYBOARD_RELEASE_REPORT: [u8; 8] = [0; 8];

const _: () = {
    assert!(!CD_PROMPT_MARKER.is_empty());
    assert!(CD_PROMPT_MARKER.len() <= MARKER_MAX_BYTES);
};

#[derive(Debug)]
pub(crate) struct XhciHidBootKeyTrigger {
    name: &'static str,
    marker: ProbeMarker,
    usage: u8,
    fired: bool,
}

impl XhciHidBootKeyTrigger {
    pub(crate) fn from_env(name: &'static str, env_name: &'static str) -> Option<Self> {
        let value = std::env::var(env_name).ok()?;
        Self::from_value_with_marker(name, &value, ProbeMarker::default_bytes(CD_PROMPT_MARKER))
    }

    pub(crate) fn from_env_with_marker_env(
        name: &'static str,
        usage_env: &'static str,
        marker_env: &'static str,
    ) -> Option<Self> {
        let value = std::env::var(usage_env).ok()?;
        if value != " " {
            return None;
        }
        match ProbeMarker::custom_from_env(marker_env) {
            Ok(Some(marker)) => Self::from_value_with_marker(name, &value, marker),
            Ok(None) => None,
            Err(error) => {
                print_marker_rejection(name, &error);
                None
            }
        }
    }

    fn from_value_with_marker(
        name: &'static str,
        value: &str,
        marker: ProbeMarker,
    ) -> Option<Self> {
        if value != " " {
            return None;
        }
        Some(Self {
            name,
            marker,
            usage: HID_BOOT_KEYBOARD_USAGE_SPACE,
            fired: false,
        })
    }

    pub(crate) fn maybe_fire(&mut self, platform: &mut VirtPlatform) {
        if self.fired || !contains_bytes(platform.uart_output(), self.marker.as_bytes()) {
            return;
        }
        if platform.queue_xhci_hid_boot_key_usage(self.usage).is_ok() {
            self.fired = true;
            println!(
                "xHCI HID boot-key injection {} fired: usage=0x{:02x} {}",
                self.name,
                self.usage,
                self.marker.log_summary()
            );
        }
    }

    pub(crate) fn print_summary(&self, platform: &VirtPlatform) {
        let stats = platform.xhci_hid_boot_key_report_stats();
        let marker_seen = contains_bytes(platform.uart_output(), self.marker.as_bytes());
        println!(
            "xHCI HID boot-key injection {}: fired={} marker_seen={} usage=0x{:02x} queued_space_reports={} unsupported_usage_rejections={} busy_rejections={} key_report={} release_report={} {}",
            self.name,
            self.fired,
            marker_seen,
            self.usage,
            stats.queued_space_reports,
            stats.unsupported_usage_rejections,
            stats.busy_rejections,
            format_report(HID_BOOT_KEYBOARD_SPACE_REPORT),
            format_report(HID_BOOT_KEYBOARD_RELEASE_REPORT),
            self.marker.log_summary()
        );
        if self.name == "cd-prompt" && !marker_seen {
            println!("xHCI HID boot-key injection cd-prompt frontier: CD prompt absent; cd-prompt fired=false");
        }
    }

    #[cfg(test)]
    pub(crate) fn from_env_value(name: &'static str, value: &str) -> Option<Self> {
        Self::from_value_with_marker(name, value, ProbeMarker::default_bytes(CD_PROMPT_MARKER))
    }

    #[cfg(test)]
    pub(crate) fn from_env_value_with_marker(
        name: &'static str,
        value: &str,
        marker: &[u8],
    ) -> Option<Self> {
        let marker = ProbeMarker::custom_for_test(marker).ok()?;
        Self::from_value_with_marker(name, value, marker)
    }

    #[cfg(test)]
    pub(crate) const fn fired(&self) -> bool {
        self.fired
    }

    #[cfg(test)]
    pub(crate) const fn usage(&self) -> u8 {
        self.usage
    }

    #[cfg(test)]
    pub(crate) fn marker(&self) -> &[u8] {
        self.marker.as_bytes()
    }
}

fn print_marker_rejection(name: &'static str, error: &MarkerEnvError) {
    println!(
        "xHCI HID boot-key injection {name} rejected: parse_error={} {} queued_space_reports=0 rejected_count=1",
        error.name(),
        error.rejection_summary()
    );
}
