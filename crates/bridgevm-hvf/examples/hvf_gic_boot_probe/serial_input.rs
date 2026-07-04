use bridgevm_hvf::platform_virt::VirtPlatform;

#[derive(Debug)]
pub(crate) struct SerialTriggeredUartInput {
    name: &'static str,
    marker: Vec<u8>,
    bytes: Vec<u8>,
    fired: bool,
    marker_scan: IncrementalMarkerScan,
}

impl SerialTriggeredUartInput {
    pub(crate) fn from_env(name: &'static str, bytes_env: &str, marker: &[u8]) -> Option<Self> {
        let bytes = std::env::var(bytes_env).ok()?.into_bytes();
        Self::from_parts(name, marker.to_vec(), bytes)
    }

    pub(crate) fn from_env_with_marker_env(
        name: &'static str,
        bytes_env: &str,
        marker_env: &str,
    ) -> Option<Self> {
        let bytes = std::env::var(bytes_env).ok()?.into_bytes();
        let marker = std::env::var(marker_env).ok()?.into_bytes();
        Self::from_parts(name, marker, bytes)
    }

    fn from_parts(name: &'static str, marker: Vec<u8>, bytes: Vec<u8>) -> Option<Self> {
        if marker.is_empty() || bytes.is_empty() {
            return None;
        }
        Some(Self {
            name,
            marker,
            bytes,
            fired: false,
            marker_scan: IncrementalMarkerScan::default(),
        })
    }

    pub(crate) fn maybe_fire(&mut self, platform: &mut VirtPlatform) {
        if self.fired
            || !self
                .marker_scan
                .contains_new(platform.uart_output(), &self.marker)
        {
            return;
        }
        platform.push_uart_input(&self.bytes);
        self.fired = true;
        println!(
            "UART RX injection {} fired: {} bytes after serial marker {:?}",
            self.name,
            self.bytes.len(),
            String::from_utf8_lossy(&self.marker)
        );
    }

    pub(crate) const fn name(&self) -> &'static str {
        self.name
    }

    pub(crate) const fn fired(&self) -> bool {
        self.fired
    }

    pub(crate) fn bytes_len(&self) -> usize {
        self.bytes.len()
    }
}

fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

#[derive(Debug, Default)]
struct IncrementalMarkerScan {
    scanned_len: usize,
    found: bool,
}

impl IncrementalMarkerScan {
    fn contains_new(&mut self, haystack: &[u8], needle: &[u8]) -> bool {
        if self.found {
            return true;
        }
        if needle.is_empty() {
            self.found = true;
            return true;
        }
        let overlap = needle.len().saturating_sub(1);
        let start = self.scanned_len.saturating_sub(overlap).min(haystack.len());
        self.scanned_len = haystack.len();
        self.found = contains_bytes(&haystack[start..], needle);
        self.found
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bridgevm_hvf::dtb::VirtFdtConfig;
    use bridgevm_hvf::machine;
    use bridgevm_hvf::platform_virt::{FlatGuestRam, MmioOp, MmioOutcome};

    fn emit_uart(platform: &mut VirtPlatform, bytes: &[u8]) {
        let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0);
        for byte in bytes {
            assert_eq!(
                platform.on_mmio(
                    machine::UART.base,
                    MmioOp::Write {
                        size: 1,
                        value: u64::from(*byte),
                    },
                    &mut mem,
                ),
                MmioOutcome::WriteAck
            );
        }
    }

    #[test]
    fn does_not_fire_before_marker() {
        let mut platform = VirtPlatform::new(VirtFdtConfig::default());
        let mut trigger =
            SerialTriggeredUartInput::from_parts("test", b"Boot0001".to_vec(), b" ".to_vec())
                .unwrap();

        trigger.maybe_fire(&mut platform);

        assert!(!trigger.fired());
        assert_eq!(platform.uart_input_len(), 0);
    }

    #[test]
    fn fires_once_after_marker() {
        let mut platform = VirtPlatform::new(VirtFdtConfig::default());
        let mut trigger =
            SerialTriggeredUartInput::from_parts("test", b"Boot0001".to_vec(), b" \r".to_vec())
                .unwrap();
        emit_uart(&mut platform, b"BdsDxe: starting Boot0001");

        trigger.maybe_fire(&mut platform);
        trigger.maybe_fire(&mut platform);

        assert!(trigger.fired());
        assert_eq!(trigger.bytes_len(), 2);
        assert_eq!(platform.uart_input_len(), 2);
    }

    #[test]
    fn rejects_empty_marker_or_empty_bytes() {
        assert!(SerialTriggeredUartInput::from_parts("test", Vec::new(), b" ".to_vec()).is_none());
        assert!(
            SerialTriggeredUartInput::from_parts("test", b"Boot0001".to_vec(), Vec::new())
                .is_none()
        );
    }
}
