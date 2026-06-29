use crate::fwcfg::GuestMemoryMut;

use super::{
    trace::{self, EventPostStateTrace, EventPostTrace, EventRingTrace},
    XhciController, USB_CMD_RS, USB_STS_HCH,
};

pub(super) const USB_STS_EINT: u32 = 1 << 3;
pub(super) const IMAN_INTERRUPT_PENDING: u32 = 1 << 0;

const ERDP_EHB: u32 = 1 << 3;
const IMAN_INTERRUPT_ENABLE: u32 = 1 << 1;
const PORT_STATUS_CHANGE_EVENT_PORT_ID: u64 = 1 << 24;
const TRB_TYPE_PORT_STATUS_CHANGE_EVENT: u32 = 34;
const TRB_SIZE: usize = 16;
const TRB_SIZE_BYTES: u64 = 16;
const TRB_CYCLE: u32 = 1;
const EVENT_RING_PROGRAMMING_START: u64 = 0x1028;
const EVENT_RING_PROGRAMMING_END: u64 = 0x1038;

pub(super) fn is_event_ring_programming_write(offset: u64, size: u8) -> bool {
    let end = offset.saturating_add(u64::from(size.min(8)));
    offset < EVENT_RING_PROGRAMMING_END && end > EVENT_RING_PROGRAMMING_START
}

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

    pub(super) fn interrupt_pending_and_enabled(&self) -> bool {
        let enabled_pending = IMAN_INTERRUPT_PENDING | IMAN_INTERRUPT_ENABLE;
        self.iman0 & enabled_pending == enabled_pending
    }

    pub(super) fn reset_event_ring(&mut self) {
        self.event_enqueue = 0;
        self.event_handler_busy = false;
        self.event_cycle = true;
    }

    pub(super) fn mark_port_status_change_pending(&mut self) {
        self.port_status_change_pending = true;
    }

    pub(super) fn post_pending_port_status_change_event(
        &mut self,
        mem: &mut dyn GuestMemoryMut,
    ) -> bool {
        if !self.port_status_change_pending {
            return false;
        }
        let posted = self.post_event(
            mem,
            PORT_STATUS_CHANGE_EVENT_PORT_ID,
            0,
            TRB_TYPE_PORT_STATUS_CHANGE_EVENT << 10,
        );
        if posted {
            self.port_status_change_pending = false;
        }
        posted
    }

    pub(super) fn erdp_low(&self) -> u32 {
        let busy = if self.event_handler_busy { ERDP_EHB } else { 0 };
        ((self.erdp0 as u32) & !ERDP_EHB) | busy
    }

    pub(super) fn write_erdp_low(&mut self, value: u32) {
        let next_erdp = (self.erdp0 & !0xffff_ffff) | u64::from(value & !ERDP_EHB);
        if value & ERDP_EHB != 0 {
            self.event_handler_busy = false;
            self.iman0 &= !IMAN_INTERRUPT_PENDING;
            trace::erdp_ehb_consumed(
                next_erdp,
                EventPostStateTrace {
                    event_handler_busy: self.event_handler_busy,
                    iman_interrupt_pending: self.iman0 & IMAN_INTERRUPT_PENDING != 0,
                    usb_sts_eint: self.usb_status() & USB_STS_EINT != 0,
                },
            );
        }
        self.erdp0 = next_erdp;
    }

    pub(super) fn write_erdp_high(&mut self, value: u32) {
        self.erdp0 = (self.erdp0 & 0xffff_ffff) | (u64::from(value) << 32);
    }

    pub(super) fn post_event(
        &mut self,
        mem: &mut dyn GuestMemoryMut,
        parameter: u64,
        status: u32,
        control_without_cycle: u32,
    ) -> bool {
        if self.erstsz0 == 0 {
            trace::event_post_reject("erst_size_zero");
            return false;
        }
        let Some(raw_erst) = mem.read_bytes(self.erstba0, 16) else {
            trace::event_post_reject_with_gpa("erst_read_failed", self.erstba0);
            return false;
        };
        let Some(segment_base) = read_u64(&raw_erst, 0) else {
            trace::event_post_reject_with_gpa("erst_segment_base_decode_failed", self.erstba0);
            return false;
        };
        let Some(segment_trbs) = read_u32(&raw_erst, 8) else {
            trace::event_post_reject_with_gpa("erst_segment_size_decode_failed", self.erstba0);
            return false;
        };
        if segment_base == 0 || segment_trbs == 0 || self.event_enqueue >= segment_trbs {
            trace::event_post_reject_with_ring(
                "invalid_event_segment",
                EventRingTrace {
                    segment_base,
                    segment_trbs,
                    enqueue: self.event_enqueue,
                    cycle: self.event_cycle,
                },
            );
            return false;
        }

        let event_gpa = segment_base + u64::from(self.event_enqueue) * TRB_SIZE_BYTES;
        let cycle = if self.event_cycle { TRB_CYCLE } else { 0 };
        let control = control_without_cycle | cycle;
        let trace = EventPostTrace {
            ring: EventRingTrace {
                segment_base,
                segment_trbs,
                enqueue: self.event_enqueue,
                cycle: self.event_cycle,
            },
            parameter,
            status,
            control,
            event_gpa,
        };
        let mut event = [0u8; TRB_SIZE];
        event[0..8].copy_from_slice(&parameter.to_le_bytes());
        event[8..12].copy_from_slice(&status.to_le_bytes());
        event[12..16].copy_from_slice(&control.to_le_bytes());
        if !mem.write_bytes(event_gpa, &event) {
            trace::event_post_reject_with_event("event_write_failed", trace);
            return false;
        }
        self.event_enqueue += 1;
        if self.event_enqueue == segment_trbs {
            self.event_enqueue = 0;
            self.event_cycle = !self.event_cycle;
        }
        self.event_handler_busy = true;
        self.iman0 |= IMAN_INTERRUPT_PENDING;
        trace::event_post_success(
            trace,
            EventPostStateTrace {
                event_handler_busy: self.event_handler_busy,
                iman_interrupt_pending: self.iman0 & IMAN_INTERRUPT_PENDING != 0,
                usb_sts_eint: self.usb_status() & USB_STS_EINT != 0,
            },
        );
        true
    }
}

fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    let raw = bytes.get(offset..offset + 4)?;
    let array: [u8; 4] = raw.try_into().ok()?;
    Some(u32::from_le_bytes(array))
}

fn read_u64(bytes: &[u8], offset: usize) -> Option<u64> {
    let raw = bytes.get(offset..offset + 8)?;
    let array: [u8; 8] = raw.try_into().ok()?;
    Some(u64::from_le_bytes(array))
}
