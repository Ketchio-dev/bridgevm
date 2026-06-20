//! Minimal PL031 RTC model for the Path A QEMU `virt` platform.
//!
//! The PL031 lives at [`crate::machine::RTC`] (`0x0901_0000`) on QEMU `virt`.
//! Firmware generally needs only a sane `RTCDR` seconds counter and the PrimeCell
//! ID registers, but this also keeps the small writable register set QEMU
//! exposes so later OS probes do not see an inert block.

use std::time::{Instant, SystemTime, UNIX_EPOCH};

const RTCDR: u64 = 0x000;
const RTCMR: u64 = 0x004;
const RTCLR: u64 = 0x008;
const RTCCR: u64 = 0x00c;
const RTCIMSC: u64 = 0x010;
const RTCRIS: u64 = 0x014;
const RTCMIS: u64 = 0x018;
const RTCICR: u64 = 0x01c;

const RTCCR_ENABLE: u64 = 1;
const IRQ_MATCH: u32 = 1;

/// A modelled PL031 real-time clock.
#[derive(Debug)]
pub struct Pl031 {
    base_epoch_seconds: u32,
    host_start: Instant,
    match_value: u32,
    fired_match_value: Option<u32>,
    interrupt_mask: u32,
    raw_interrupt: u32,
}

impl Default for Pl031 {
    fn default() -> Self {
        Self::new()
    }
}

impl Pl031 {
    pub fn new() -> Self {
        Self::new_at_epoch(host_epoch_seconds())
    }

    /// Deterministic constructor for tests.
    pub fn new_at_epoch(epoch_seconds: u32) -> Self {
        Self {
            base_epoch_seconds: epoch_seconds,
            host_start: Instant::now(),
            match_value: 0,
            fired_match_value: None,
            interrupt_mask: 0,
            raw_interrupt: 0,
        }
    }

    /// MMIO read within the RTC window.
    pub fn mmio_read(&mut self, offset: u64, _size: u8) -> u64 {
        self.refresh_match_interrupt();
        match offset {
            RTCDR => u64::from(self.current_seconds()),
            RTCMR => u64::from(self.match_value),
            RTCLR => 0,
            RTCCR => RTCCR_ENABLE,
            RTCIMSC => u64::from(self.interrupt_mask),
            RTCRIS => u64::from(self.raw_interrupt),
            RTCMIS => u64::from(self.raw_interrupt & self.interrupt_mask),
            0xfe0 => 0x31,
            0xfe4 => 0x10,
            0xfe8 => 0x14,
            0xfec => 0x00,
            0xff0 => 0x0d,
            0xff4 => 0xf0,
            0xff8 => 0x05,
            0xffc => 0xb1,
            _ => 0,
        }
    }

    /// MMIO write within the RTC window.
    pub fn mmio_write(&mut self, offset: u64, _size: u8, value: u64) {
        match offset {
            RTCMR => {
                self.match_value = value as u32;
                self.fired_match_value = None;
            }
            RTCLR => {
                self.base_epoch_seconds = value as u32;
                self.host_start = Instant::now();
                self.fired_match_value = None;
                self.raw_interrupt = 0;
            }
            RTCIMSC => self.interrupt_mask = (value as u32) & IRQ_MATCH,
            RTCICR => self.raw_interrupt &= !((value as u32) & IRQ_MATCH),
            _ => {}
        }
    }

    fn current_seconds(&self) -> u32 {
        self.base_epoch_seconds
            .wrapping_add(self.host_start.elapsed().as_secs() as u32)
    }

    fn refresh_match_interrupt(&mut self) {
        if self.match_value != 0
            && self.current_seconds() >= self.match_value
            && self.fired_match_value != Some(self.match_value)
        {
            self.raw_interrupt |= IRQ_MATCH;
            self.fired_match_value = Some(self.match_value);
        }
    }
}

fn host_epoch_seconds() -> u32 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_register_reports_epoch_seconds() {
        let mut rtc = Pl031::new_at_epoch(0x2026_0619);
        assert_eq!(rtc.mmio_read(RTCDR, 4), 0x2026_0619);
    }

    #[test]
    fn load_register_resets_the_counter_base() {
        let mut rtc = Pl031::new_at_epoch(1);
        rtc.mmio_write(RTCLR, 4, 0x1234_5678);
        assert_eq!(rtc.mmio_read(RTCDR, 4), 0x1234_5678);
    }

    #[test]
    fn match_interrupt_respects_mask_and_clear() {
        let mut rtc = Pl031::new_at_epoch(100);
        rtc.mmio_write(RTCMR, 4, 99);
        assert_eq!(rtc.mmio_read(RTCRIS, 4), 1);
        assert_eq!(rtc.mmio_read(RTCMIS, 4), 0);
        rtc.mmio_write(RTCIMSC, 4, 1);
        assert_eq!(rtc.mmio_read(RTCMIS, 4), 1);
        rtc.mmio_write(RTCICR, 4, 1);
        assert_eq!(rtc.mmio_read(RTCRIS, 4), 0);
    }

    #[test]
    fn primecell_ids_match_qemu_pl031() {
        let mut rtc = Pl031::new_at_epoch(0);
        assert_eq!(rtc.mmio_read(0xfe0, 4), 0x31);
        assert_eq!(rtc.mmio_read(0xfe4, 4), 0x10);
        assert_eq!(rtc.mmio_read(0xfe8, 4), 0x14);
        assert_eq!(rtc.mmio_read(0xff0, 4), 0x0d);
        assert_eq!(rtc.mmio_read(0xff4, 4), 0xf0);
        assert_eq!(rtc.mmio_read(0xff8, 4), 0x05);
        assert_eq!(rtc.mmio_read(0xffc, 4), 0xb1);
    }
}
