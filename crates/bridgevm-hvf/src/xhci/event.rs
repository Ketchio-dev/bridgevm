use super::{XhciController, USB_CMD_RS, USB_STS_HCH};

pub(super) const USB_STS_EINT: u32 = 1 << 3;
pub(super) const IMAN_INTERRUPT_PENDING: u32 = 1 << 0;

const ERDP_EHB: u32 = 1 << 3;
const IMAN_INTERRUPT_ENABLE: u32 = 1 << 1;

impl XhciController {
    pub(super) fn write_iman0(&mut self, value: u32) {
        let pending = if value & IMAN_INTERRUPT_PENDING != 0 {
            0
        } else {
            self.iman0 & IMAN_INTERRUPT_PENDING
        };
        self.iman0 = pending | (value & IMAN_INTERRUPT_ENABLE);
    }

    pub(super) fn usb_status(&self) -> u32 {
        let event_interrupt = if self.iman0 & IMAN_INTERRUPT_PENDING != 0 {
            USB_STS_EINT
        } else {
            0
        };
        if self.usb_command & USB_CMD_RS == 0 {
            USB_STS_HCH | event_interrupt
        } else {
            event_interrupt
        }
    }

    pub(super) fn reset_event_ring(&mut self) {
        self.event_enqueue = 0;
        self.event_handler_busy = false;
        self.event_cycle = true;
    }

    pub(super) fn erdp_low(&self) -> u32 {
        let busy = if self.event_handler_busy { ERDP_EHB } else { 0 };
        ((self.erdp0 as u32) & !ERDP_EHB) | busy
    }

    pub(super) fn write_erdp_low(&mut self, value: u32) {
        if value & ERDP_EHB != 0 {
            self.event_handler_busy = false;
            self.iman0 &= !IMAN_INTERRUPT_PENDING;
        }
        self.erdp0 = (self.erdp0 & !0xffff_ffff) | u64::from(value & !ERDP_EHB);
    }

    pub(super) fn write_erdp_high(&mut self, value: u32) {
        self.erdp0 = (self.erdp0 & 0xffff_ffff) | (u64::from(value) << 32);
    }
}
