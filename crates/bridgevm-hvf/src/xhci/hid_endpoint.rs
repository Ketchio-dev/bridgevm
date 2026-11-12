//! Lifecycle of the two modeled HID interrupt-IN endpoints.
use super::{
    device_context_mem::{output_context_for_slot, write_ep_context_state},
    interrupt_trb::{transfer_event_control, trb_interrupter_target, trb_transfer_length},
    XhciController,
};
use crate::fwcfg::GuestMemoryMut;

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct HidEndpoint {
    pub(super) state: u32,
    // Only a Normal TRB actually reached by the transfer engine is pending.
    // Retain its identity across NAKs; do not infer execution from guest memory.
    pub(super) pending: Option<(u64, u32)>,
}

impl XhciController {
    pub(super) fn hid_index(ep: u32) -> Option<usize> {
        match ep {
            3 => Some(0),
            5 => Some(1),
            _ => None,
        }
    }
    pub(super) fn publish_hid_state(&self, mem: &mut dyn GuestMemoryMut, ep: u32) {
        let Some(i) = Self::hid_index(ep) else {
            return;
        };
        let Some(gpa) = output_context_for_slot(mem, self.dcbaap & !0x3f, 1)
            .and_then(|base| base.checked_add(u64::from(ep) * 32))
        else {
            return;
        };
        write_ep_context_state(mem, gpa, self.hid_endpoints[i].state);
    }
    pub(super) fn restart_hid(&mut self, mem: &mut dyn GuestMemoryMut, ep: u32) {
        let Some(i) = Self::hid_index(ep) else {
            return;
        };
        if self.hid_endpoints[i].state == 3 {
            self.hid_endpoints[i].state = 1;
            self.publish_hid_state(mem, ep);
        }
    }
    pub(super) fn stop_hid(&mut self, mem: &mut dyn GuestMemoryMut, ep: u32) -> bool {
        let Some(i) = Self::hid_index(ep) else {
            return false;
        };
        if let Some((gpa, status)) = self.hid_endpoints[i].pending {
            // No bytes were transferred by a NAKed IN. Stopped (26) reports
            // the interrupted TRB and full residual, even for an Event Data TD.
            if !self.post_event_to_interrupter(
                mem,
                trb_interrupter_target(status),
                gpa,
                (26 << 24) | trb_transfer_length(status),
                transfer_event_control(1, ep),
            ) {
                return false;
            }
            self.hid_endpoints[i].pending = None;
        }
        self.hid_endpoints[i].state = 3;
        if ep == 3 {
            self.write_slot1_dci3_output_dequeue(mem);
        } else {
            self.write_slot1_dci5_output_dequeue(mem);
        }
        self.publish_hid_state(mem, ep);
        true
    }
}
