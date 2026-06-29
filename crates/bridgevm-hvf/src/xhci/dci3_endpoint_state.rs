use crate::fwcfg::GuestMemoryMut;

use super::device_context_mem::{output_context_for_slot, read_mem_u32, read_u64};
use super::trace::{self, Dci3InputCaptureTrace, Dci3InputContextField};
use super::XhciController;

const SLOT_ID: u32 = 1;
const DCBAA_POINTER_MASK: u64 = !0x3f;
const INPUT_CONTROL_ADD_CONTEXT_OFFSET: u64 = 0x04;
const DCI3: u32 = 3;
const DCI3_INPUT_CONTEXT_OFFSET: u64 = 0x80;
const DCI3_OUTPUT_CONTEXT_OFFSET: u64 = 0x60;
const EP_CONTEXT_BYTES: usize = 32;
const EP_TR_DEQUEUE_OFFSET: u64 = 0x08;
const EP_TR_DEQUEUE_MASK: u64 = !0xf;

impl XhciController {
    pub(super) fn invalidate_slot1_dci3_endpoint_state(&mut self) {
        self.slot1_dci3_dequeue = 0;
        self.slot1_dci3_ring_base = 0;
        self.slot1_dci3_dcs = false;
        self.slot1_dci3_two_entry_queue_rearm = false;
        self.slot1_dci3_last_dequeue = 0;
        self.slot1_dci3_last_dcs = false;
        self.slot1_dci3_last_ring_base = 0;
        self.slot1_dci3_last_ring_dcs = false;
        self.slot1_dci3_last_reusable = false;
    }

    pub(super) fn remember_slot1_dci3_endpoint_state(&mut self) {
        if self.slot1_dci3_dequeue != 0 && self.slot1_dci3_ring_base != 0 {
            self.slot1_dci3_last_dequeue = self.slot1_dci3_dequeue;
            self.slot1_dci3_last_dcs = self.slot1_dci3_dcs;
            self.slot1_dci3_last_ring_base = self.slot1_dci3_ring_base;
            self.slot1_dci3_last_ring_dcs = self.slot1_dci3_dcs;
            self.slot1_dci3_last_reusable = true;
        }
    }

    pub(super) fn capture_slot1_dci3_input_context(
        &mut self,
        mem: &mut dyn GuestMemoryMut,
        input_context: u64,
    ) -> bool {
        let mut capture_trace = Dci3InputCaptureTrace {
            input_context,
            add_context: Dci3InputContextField::Unreadable,
            dci3_context: Dci3InputContextField::Unreadable,
            raw_dequeue: None,
            output_context: None,
            dci3_output: None,
            published: false,
            reason: "started",
        };
        capture_trace.add_context =
            match input_context.checked_add(INPUT_CONTROL_ADD_CONTEXT_OFFSET) {
                Some(gpa) => read_mem_u32(mem, gpa)
                    .map(|add_context| Dci3InputContextField::Value(u64::from(add_context)))
                    .unwrap_or(Dci3InputContextField::Unreadable),
                None => Dci3InputContextField::Overflow,
            };
        match capture_trace.add_context {
            Dci3InputContextField::Value(add_context) if add_context & (1_u64 << DCI3) != 0 => {}
            Dci3InputContextField::Value(_) => {
                capture_trace.reason = "dci3_not_added";
                trace::dci3_input_capture(capture_trace);
                return false;
            }
            Dci3InputContextField::Unreadable => {
                capture_trace.reason = "add_context_unreadable";
                trace::dci3_input_capture(capture_trace);
                return false;
            }
            Dci3InputContextField::Overflow => {
                capture_trace.reason = "add_context_overflow";
                trace::dci3_input_capture(capture_trace);
                return false;
            }
        }
        let dci3_input_gpa = match input_context.checked_add(DCI3_INPUT_CONTEXT_OFFSET) {
            Some(gpa) => gpa,
            None => {
                capture_trace.dci3_context = Dci3InputContextField::Overflow;
                capture_trace.reason = "dci3_context_overflow";
                trace::dci3_input_capture(capture_trace);
                return false;
            }
        };
        capture_trace.dci3_context = Dci3InputContextField::Value(dci3_input_gpa);
        let Some(dci3_input_context) = mem.read_bytes(dci3_input_gpa, EP_CONTEXT_BYTES) else {
            capture_trace.dci3_context = Dci3InputContextField::Unreadable;
            capture_trace.reason = "dci3_context_unreadable";
            trace::dci3_input_capture(capture_trace);
            return false;
        };
        let Some(raw_dequeue) = read_u64(&dci3_input_context, EP_TR_DEQUEUE_OFFSET as usize) else {
            capture_trace.reason = "raw_dequeue_unreadable";
            trace::dci3_input_capture(capture_trace);
            return false;
        };
        capture_trace.raw_dequeue = Some(raw_dequeue);
        let dci3_dequeue = raw_dequeue & EP_TR_DEQUEUE_MASK;
        self.slot1_dci3_dequeue = dci3_dequeue;
        self.slot1_dci3_ring_base = dci3_dequeue;
        self.slot1_dci3_dcs = raw_dequeue & 1 != 0;
        self.slot1_dci3_two_entry_queue_rearm = false;
        self.remember_slot1_dci3_endpoint_state();
        capture_trace.published = true;

        let Some(output_context) =
            output_context_for_slot(mem, self.dcbaap & DCBAA_POINTER_MASK, SLOT_ID)
        else {
            capture_trace.reason = "published_output_context_unreadable";
            trace::dci3_input_capture(capture_trace);
            return true;
        };
        capture_trace.output_context = Some(output_context);
        let Some(dci3_output_gpa) = output_context.checked_add(DCI3_OUTPUT_CONTEXT_OFFSET) else {
            capture_trace.reason = "published_dci3_output_overflow";
            trace::dci3_input_capture(capture_trace);
            return true;
        };
        capture_trace.dci3_output = Some(dci3_output_gpa);
        if !mem.write_bytes(dci3_output_gpa, &dci3_input_context) {
            capture_trace.reason = "published_dci3_output_write_failed";
            trace::dci3_input_capture(capture_trace);
            return true;
        }
        capture_trace.reason = "published";
        trace::dci3_input_capture(capture_trace);
        true
    }
}
