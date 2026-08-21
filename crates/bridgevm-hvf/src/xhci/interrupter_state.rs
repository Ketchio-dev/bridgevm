//! xHCI interrupter notification and ERDP/EHB state.
//!
//! When software clears EHB through ERDP and the dequeue still differs from
//! the producer, the controller raises a new notification for unconsumed
//! events. Windows requires this in addition to per-event MSI delivery during
//! enumeration (live run 20260821-052328-2196-32526).

use super::event::{
    IMAN_INTERRUPT_ENABLE, IMAN_INTERRUPT_PENDING, USB_STS_EINT, XHCI_INTERRUPTER_COUNT,
};
use super::trace::{self, EventPostStateTrace};
use super::XhciController;

const ERDP_EHB: u32 = 1 << 3;
const EVENT_TRB_BYTES: u64 = 16;

impl XhciController {
    pub(super) fn set_interrupter_pending(&mut self, index: usize) {
        if index >= XHCI_INTERRUPTER_COUNT {
            return;
        }
        self.interrupters[index].iman |= IMAN_INTERRUPT_PENDING;
        self.refresh_interrupter_pending_bits(index);
    }

    pub(super) fn clear_interrupter_pending(&mut self, index: usize) {
        if index >= XHCI_INTERRUPTER_COUNT {
            return;
        }
        self.interrupters[index].iman &= !IMAN_INTERRUPT_PENDING;
        self.refresh_interrupter_pending_bits(index);
    }

    pub(super) fn refresh_interrupter_pending_bits(&mut self, index: usize) {
        let Some(bit) = 1u32.checked_shl(index as u32) else {
            return;
        };
        let interrupter = self.interrupters[index];
        if interrupter.iman & IMAN_INTERRUPT_PENDING != 0 {
            self.pending_interrupter_bits |= bit;
        } else {
            self.pending_interrupter_bits &= !bit;
        }
        if interrupter.iman & (IMAN_INTERRUPT_PENDING | IMAN_INTERRUPT_ENABLE)
            == (IMAN_INTERRUPT_PENDING | IMAN_INTERRUPT_ENABLE)
        {
            self.pending_enabled_interrupter_bits |= bit;
        } else {
            self.pending_enabled_interrupter_bits &= !bit;
        }
    }

    pub(super) fn erdp_low(&self, index: usize) -> u32 {
        let interrupter = self.interrupters[index];
        let busy = u32::from(interrupter.event_handler_busy) * ERDP_EHB;
        ((interrupter.erdp as u32) & !ERDP_EHB) | busy
    }

    pub(super) fn write_erdp_low(&mut self, index: usize, value: u32) {
        let next = (self.interrupters[index].erdp & !0xffff_ffff) | u64::from(value & !ERDP_EHB);
        self.write_erdp(index, next, value & ERDP_EHB != 0);
    }

    pub(super) fn write_erdp_high(&mut self, index: usize, value: u32) {
        let next = (self.interrupters[index].erdp & 0xffff_ffff) | (u64::from(value) << 32);
        self.write_erdp(index, next, false);
    }

    fn write_erdp(&mut self, index: usize, next: u64, clear_ehb: bool) {
        self.record_erdp_update(next);
        self.interrupters[index].erdp = next;
        if !clear_ehb {
            return;
        }
        self.interrupters[index].iman &= !IMAN_INTERRUPT_PENDING;
        self.refresh_interrupter_pending_bits(index);
        self.event_lifecycle_stats.erdp_ehb_consumed = self
            .event_lifecycle_stats
            .erdp_ehb_consumed
            .saturating_add(1);
        self.interrupters[index].event_handler_busy = false;
        let renotify = self.interrupter_has_unconsumed_events(index);
        if renotify {
            self.interrupters[index].event_handler_busy = true;
            self.set_interrupter_pending(index);
        }
        trace::erdp_ehb_consumed(
            next,
            index,
            EventPostStateTrace {
                event_handler_busy: self.interrupters[index].event_handler_busy,
                iman_interrupt_pending: self.interrupters[index].iman & IMAN_INTERRUPT_PENDING != 0,
                usb_sts_eint: self.usb_status() & USB_STS_EINT != 0,
            },
        );
    }

    fn interrupter_has_unconsumed_events(&self, index: usize) -> bool {
        let interrupter = self.interrupters[index];
        if interrupter.event_segment_base == 0 || interrupter.event_segment_trbs == 0 {
            return false;
        }
        let producer =
            interrupter.event_segment_base + u64::from(interrupter.event_enqueue) * EVENT_TRB_BYTES;
        interrupter.erdp != producer
    }
}
