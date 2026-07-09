use crate::fwcfg::GuestMemoryMut;

use super::{
    trace::{self, EventPostStateTrace, EventPostTrace, EventRingTrace},
    XhciController, USB_CMD_RS, USB_STS_HCH,
};

pub(super) const USB_STS_EINT: u32 = 1 << 3;
pub(super) const IMAN_INTERRUPT_PENDING: u32 = 1 << 0;

/// HCSPARAMS1 advertises the QEMU oracle's 16 interrupters; Windows bootmgr
/// programs interrupter 1 for transfer events while commands and port changes
/// stay on interrupter 0.
pub(super) const XHCI_INTERRUPTER_COUNT: usize = 16;

const ERDP_EHB: u32 = 1 << 3;
pub(super) const IMAN_INTERRUPT_ENABLE: u32 = 1 << 1;
const PORT_STATUS_CHANGE_EVENT_PORT_ID: u64 = 1 << 24;
const TRB_TYPE_PORT_STATUS_CHANGE_EVENT: u32 = 34;
const TRB_TYPE_TRANSFER_EVENT: u32 = 32;
const TRB_TYPE_COMMAND_COMPLETION_EVENT: u32 = 33;
const TRB_TYPE_SHIFT: u32 = 10;
const TRB_TYPE_MASK: u32 = 0x3f;
const TRB_SIZE: usize = 16;
const TRB_SIZE_BYTES: u64 = 16;
const TRB_CYCLE: u32 = 1;
const EVENT_RING_PROGRAMMING_START: u64 = 0x1028;
const EVENT_RING_PROGRAMMING_END: u64 = 0x1038;
const PRIMARY_INTERRUPTER: usize = 0;

#[derive(Debug, Clone, Copy)]
pub(super) struct Interrupter {
    pub(super) iman: u32,
    pub(super) imod: u32,
    pub(super) erstsz: u32,
    pub(super) erstba: u64,
    pub(super) erdp: u64,
    pub(super) event_handler_busy: bool,
    pub(super) event_enqueue: u32,
    pub(super) event_cycle: bool,
}

impl Interrupter {
    pub(super) const fn new() -> Self {
        Self {
            iman: 0,
            imod: 0,
            erstsz: 0,
            erstba: 0,
            erdp: 0,
            event_handler_busy: false,
            event_enqueue: 0,
            event_cycle: true,
        }
    }
}

pub(super) fn is_event_ring_programming_write(offset: u64, size: u8) -> bool {
    let end = offset.saturating_add(u64::from(size.min(8)));
    offset < EVENT_RING_PROGRAMMING_END && end > EVENT_RING_PROGRAMMING_START
}

impl XhciController {
    pub(super) fn write_iman(&mut self, index: usize, value: u32) {
        let interrupter = &mut self.interrupters[index];
        let pending = if value & IMAN_INTERRUPT_PENDING != 0 {
            0
        } else {
            interrupter.iman & IMAN_INTERRUPT_PENDING
        };
        interrupter.iman = pending | (value & IMAN_INTERRUPT_ENABLE);
    }

    pub(super) fn usb_status(&self) -> u32 {
        let event_interrupt = if self
            .interrupters
            .iter()
            .any(|interrupter| interrupter.iman & IMAN_INTERRUPT_PENDING != 0)
        {
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
        self.interrupters
            .iter()
            .any(|interrupter| interrupter.iman & enabled_pending == enabled_pending)
    }

    pub(super) fn reset_event_ring(&mut self, index: usize) {
        let interrupter = &mut self.interrupters[index];
        interrupter.event_enqueue = 0;
        interrupter.event_handler_busy = false;
        interrupter.event_cycle = true;
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

    pub(super) fn erdp_low(&self, index: usize) -> u32 {
        let interrupter = &self.interrupters[index];
        let busy = if interrupter.event_handler_busy {
            ERDP_EHB
        } else {
            0
        };
        ((interrupter.erdp as u32) & !ERDP_EHB) | busy
    }

    pub(super) fn write_erdp_low(&mut self, index: usize, value: u32) {
        let next_erdp =
            (self.interrupters[index].erdp & !0xffff_ffff) | u64::from(value & !ERDP_EHB);
        self.record_erdp_update(next_erdp);
        if value & ERDP_EHB != 0 {
            self.event_lifecycle_stats.erdp_ehb_consumed = self
                .event_lifecycle_stats
                .erdp_ehb_consumed
                .saturating_add(1);
            self.interrupters[index].event_handler_busy = false;
            self.interrupters[index].iman &= !IMAN_INTERRUPT_PENDING;
            trace::erdp_ehb_consumed(
                next_erdp,
                index,
                EventPostStateTrace {
                    event_handler_busy: self.interrupters[index].event_handler_busy,
                    iman_interrupt_pending: self.interrupters[index].iman & IMAN_INTERRUPT_PENDING
                        != 0,
                    usb_sts_eint: self.usb_status() & USB_STS_EINT != 0,
                },
            );
        }
        self.interrupters[index].erdp = next_erdp;
    }

    pub(super) fn write_erdp_high(&mut self, index: usize, value: u32) {
        let next_erdp = (self.interrupters[index].erdp & 0xffff_ffff) | (u64::from(value) << 32);
        self.record_erdp_update(next_erdp);
        self.interrupters[index].erdp = next_erdp;
    }

    pub(super) fn post_event(
        &mut self,
        mem: &mut dyn GuestMemoryMut,
        parameter: u64,
        status: u32,
        control_without_cycle: u32,
    ) -> bool {
        self.post_event_to_interrupter(
            mem,
            PRIMARY_INTERRUPTER,
            parameter,
            status,
            control_without_cycle,
        )
    }

    pub(super) fn post_event_to_interrupter(
        &mut self,
        mem: &mut dyn GuestMemoryMut,
        index: usize,
        parameter: u64,
        status: u32,
        control_without_cycle: u32,
    ) -> bool {
        // An interrupter target beyond the modeled set falls back to the
        // primary interrupter rather than dropping the event silently.
        let index = if index < XHCI_INTERRUPTER_COUNT {
            index
        } else {
            PRIMARY_INTERRUPTER
        };
        self.record_event_post_attempt();
        let interrupter = self.interrupters[index];
        if interrupter.erstsz == 0 {
            trace::event_post_reject("erst_size_zero");
            self.record_event_post_failure();
            return false;
        }
        let mut raw_erst = [0u8; 16];
        if !mem.read_into(interrupter.erstba, &mut raw_erst) {
            trace::event_post_reject_with_gpa("erst_read_failed", interrupter.erstba);
            self.record_event_post_failure();
            return false;
        }
        let Some(segment_base) = read_u64(&raw_erst, 0) else {
            trace::event_post_reject_with_gpa(
                "erst_segment_base_decode_failed",
                interrupter.erstba,
            );
            self.record_event_post_failure();
            return false;
        };
        let Some(segment_trbs) = read_u32(&raw_erst, 8) else {
            trace::event_post_reject_with_gpa(
                "erst_segment_size_decode_failed",
                interrupter.erstba,
            );
            self.record_event_post_failure();
            return false;
        };
        if segment_base == 0 || segment_trbs == 0 || interrupter.event_enqueue >= segment_trbs {
            trace::event_post_reject_with_ring(
                "invalid_event_segment",
                EventRingTrace {
                    segment_base,
                    segment_trbs,
                    enqueue: interrupter.event_enqueue,
                    cycle: interrupter.event_cycle,
                    interrupter: index,
                },
            );
            self.record_event_post_failure();
            return false;
        }

        let event_gpa = segment_base + u64::from(interrupter.event_enqueue) * TRB_SIZE_BYTES;
        let cycle = if interrupter.event_cycle {
            TRB_CYCLE
        } else {
            0
        };
        let control = control_without_cycle | cycle;
        let trace = EventPostTrace {
            ring: EventRingTrace {
                segment_base,
                segment_trbs,
                enqueue: interrupter.event_enqueue,
                cycle: interrupter.event_cycle,
                interrupter: index,
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
            self.record_event_post_failure();
            return false;
        }
        let interrupter = &mut self.interrupters[index];
        interrupter.event_enqueue += 1;
        if interrupter.event_enqueue == segment_trbs {
            interrupter.event_enqueue = 0;
            interrupter.event_cycle = !interrupter.event_cycle;
        }
        interrupter.event_handler_busy = true;
        interrupter.iman |= IMAN_INTERRUPT_PENDING;
        self.record_event_post_success(trace);
        trace::event_post_success(
            trace,
            EventPostStateTrace {
                event_handler_busy: self.interrupters[index].event_handler_busy,
                iman_interrupt_pending: self.interrupters[index].iman & IMAN_INTERRUPT_PENDING != 0,
                usb_sts_eint: self.usb_status() & USB_STS_EINT != 0,
            },
        );
        true
    }

    pub fn event_lifecycle_stats(&self) -> super::XhciEventLifecycleStats {
        self.event_lifecycle_stats
    }

    fn record_erdp_update(&mut self, erdp: u64) {
        self.event_lifecycle_stats.erdp_updates =
            self.event_lifecycle_stats.erdp_updates.saturating_add(1);
        self.event_lifecycle_stats.last_erdp = erdp;
    }

    fn record_event_post_attempt(&mut self) {
        self.event_lifecycle_stats.event_post_attempts = self
            .event_lifecycle_stats
            .event_post_attempts
            .saturating_add(1);
    }

    fn record_event_post_failure(&mut self) {
        self.event_lifecycle_stats.event_post_failures = self
            .event_lifecycle_stats
            .event_post_failures
            .saturating_add(1);
    }

    fn record_event_post_success(&mut self, trace: EventPostTrace) {
        self.event_lifecycle_stats.event_post_successes = self
            .event_lifecycle_stats
            .event_post_successes
            .saturating_add(1);
        match event_type(trace.control) {
            TRB_TYPE_TRANSFER_EVENT => {
                self.event_lifecycle_stats.transfer_event_posts = self
                    .event_lifecycle_stats
                    .transfer_event_posts
                    .saturating_add(1);
            }
            TRB_TYPE_COMMAND_COMPLETION_EVENT => {
                self.event_lifecycle_stats.command_completion_event_posts = self
                    .event_lifecycle_stats
                    .command_completion_event_posts
                    .saturating_add(1);
            }
            TRB_TYPE_PORT_STATUS_CHANGE_EVENT => {
                self.event_lifecycle_stats.port_status_change_event_posts = self
                    .event_lifecycle_stats
                    .port_status_change_event_posts
                    .saturating_add(1);
            }
            _ => {}
        }
        self.event_lifecycle_stats.last_event_interrupter = trace.ring.interrupter;
        self.event_lifecycle_stats.last_event_gpa = trace.event_gpa;
        self.event_lifecycle_stats.last_event_parameter = trace.parameter;
        self.event_lifecycle_stats.last_event_status = trace.status;
        self.event_lifecycle_stats.last_event_control = trace.control;
    }
}

fn event_type(control: u32) -> u32 {
    (control >> TRB_TYPE_SHIFT) & TRB_TYPE_MASK
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
