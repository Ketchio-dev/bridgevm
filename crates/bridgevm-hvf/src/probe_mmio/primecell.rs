//! Split out of probe_mmio.rs by responsibility.

use super::*;
use crate::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Pl011UartDevice {
    pub(crate) base_ipa: u64,
    pub(crate) flag_value: u64,
}

impl Pl011UartDevice {
    pub(crate) fn new(base_ipa: u64, flag_value: u64) -> Self {
        Self {
            base_ipa,
            flag_value,
        }
    }

    pub(crate) fn data_ipa(&self) -> u64 {
        self.base_ipa + PL011_DR_OFFSET
    }

    pub(crate) fn flags_ipa(&self) -> u64 {
        self.base_ipa + PL011_FR_OFFSET
    }
}

impl MmioDevice for Pl011UartDevice {
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn range(&self) -> MmioRange {
        MmioRange {
            start: self.base_ipa,
            bytes: PL011_REGISTER_WINDOW_BYTES,
        }
    }

    fn handle(&mut self, access: MmioAccess) -> MmioAction {
        match (access.kind, access.ipa, access.value) {
            (MmioAccessKind::Write, ipa, Some(value)) if ipa == self.data_ipa() => {
                let mask = if access.width >= 8 {
                    u64::MAX
                } else {
                    (1_u64 << (u64::from(access.width) * 8)) - 1
                };
                let value = value & mask;
                MmioAction::WriteAccepted {
                    value,
                    byte: (value & 0xff) as u8,
                }
            }
            (MmioAccessKind::Read, ipa, None) if ipa == self.flags_ipa() => {
                MmioAction::ReadValue(self.flag_value)
            }
            _ => MmioAction::Unhandled,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Pl031RtcDevice {
    pub(crate) base_ipa: u64,
    pub(crate) data_value: u64,
}

impl Pl031RtcDevice {
    pub(crate) fn new(base_ipa: u64, data_value: u64) -> Self {
        Self {
            base_ipa,
            data_value,
        }
    }

    pub(crate) fn data_ipa(&self) -> u64 {
        self.base_ipa + PL031_DR_OFFSET
    }
}

impl MmioDevice for Pl031RtcDevice {
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn range(&self) -> MmioRange {
        MmioRange {
            start: self.base_ipa,
            bytes: PL031_REGISTER_WINDOW_BYTES,
        }
    }

    fn handle(&mut self, access: MmioAccess) -> MmioAction {
        match (access.kind, access.ipa, access.value) {
            (MmioAccessKind::Read, ipa, None) if ipa == self.data_ipa() => {
                MmioAction::ReadValue(self.data_value)
            }
            _ => MmioAction::Unhandled,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
