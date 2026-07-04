use crate::fwcfg::GuestMemoryMut;

use super::device_context_mem::{
    output_context_for_slot, read_mem_u32, read_mem_u64, read_u64, write_mem_u64,
};
use super::trace::{self, Dci5InputCaptureTrace, Dci5InputContextField};
use super::XhciController;

const SLOT_ID: u32 = 1;
const DCBAA_POINTER_MASK: u64 = !0x3f;
const INPUT_CONTROL_ADD_CONTEXT_OFFSET: u64 = 0x04;
const DCI5: u32 = 5;
const DCI5_INPUT_CONTEXT_OFFSET: u64 = 0xc0;
const DCI5_OUTPUT_CONTEXT_OFFSET: u64 = 0xa0;
const EP_CONTEXT_BYTES: usize = 32;
const EP_TR_DEQUEUE_OFFSET: u64 = 0x08;
const EP_TR_DEQUEUE_MASK: u64 = !0xf;

impl XhciController {
    pub(super) fn invalidate_slot1_dci5_endpoint_state(&mut self) {
        self.slot1_dci5_dequeue = 0;
        self.slot1_dci5_ring_base = 0;
        self.slot1_dci5_dcs = false;
        self.slot1_dci5_last_drain_blocked = None;
    }

    pub(super) fn capture_slot1_dci5_input_context(
        &mut self,
        mem: &mut dyn GuestMemoryMut,
        input_context: u64,
    ) -> bool {
        let mut capture_trace = Dci5InputCaptureTrace {
            input_context,
            add_context: Dci5InputContextField::Unreadable,
            dci5_context: Dci5InputContextField::Unreadable,
            raw_dequeue: None,
            output_context: None,
            dci5_output: None,
            published: false,
            reason: "started",
        };
        capture_trace.add_context =
            match input_context.checked_add(INPUT_CONTROL_ADD_CONTEXT_OFFSET) {
                Some(gpa) => read_mem_u32(mem, gpa)
                    .map(|add_context| Dci5InputContextField::Value(u64::from(add_context)))
                    .unwrap_or(Dci5InputContextField::Unreadable),
                None => Dci5InputContextField::Overflow,
            };
        match capture_trace.add_context {
            Dci5InputContextField::Value(add_context) if add_context & (1_u64 << DCI5) != 0 => {}
            Dci5InputContextField::Value(_) => {
                capture_trace.reason = "dci5_not_added";
                trace::dci5_input_capture(capture_trace);
                return false;
            }
            Dci5InputContextField::Unreadable => {
                capture_trace.reason = "add_context_unreadable";
                trace::dci5_input_capture(capture_trace);
                return false;
            }
            Dci5InputContextField::Overflow => {
                capture_trace.reason = "add_context_overflow";
                trace::dci5_input_capture(capture_trace);
                return false;
            }
        }
        let dci5_input_gpa = match input_context.checked_add(DCI5_INPUT_CONTEXT_OFFSET) {
            Some(gpa) => gpa,
            None => {
                capture_trace.dci5_context = Dci5InputContextField::Overflow;
                capture_trace.reason = "dci5_context_overflow";
                trace::dci5_input_capture(capture_trace);
                return false;
            }
        };
        capture_trace.dci5_context = Dci5InputContextField::Value(dci5_input_gpa);
        let Some(dci5_input_context) = mem.read_bytes(dci5_input_gpa, EP_CONTEXT_BYTES) else {
            capture_trace.dci5_context = Dci5InputContextField::Unreadable;
            capture_trace.reason = "dci5_context_unreadable";
            trace::dci5_input_capture(capture_trace);
            return false;
        };
        let Some(raw_dequeue) = read_u64(&dci5_input_context, EP_TR_DEQUEUE_OFFSET as usize) else {
            capture_trace.reason = "raw_dequeue_unreadable";
            trace::dci5_input_capture(capture_trace);
            return false;
        };
        capture_trace.raw_dequeue = Some(raw_dequeue);
        let dci5_dequeue = raw_dequeue & EP_TR_DEQUEUE_MASK;
        self.slot1_dci5_dequeue = dci5_dequeue;
        self.slot1_dci5_ring_base = dci5_dequeue;
        self.slot1_dci5_dcs = raw_dequeue & 1 != 0;
        self.slot1_dci5_last_drain_blocked = None;
        capture_trace.published = true;

        let Some(output_context) =
            output_context_for_slot(mem, self.dcbaap & DCBAA_POINTER_MASK, SLOT_ID)
        else {
            capture_trace.reason = "published_output_context_unreadable";
            trace::dci5_input_capture(capture_trace);
            return true;
        };
        capture_trace.output_context = Some(output_context);
        let Some(dci5_output_gpa) = output_context.checked_add(DCI5_OUTPUT_CONTEXT_OFFSET) else {
            capture_trace.reason = "published_dci5_output_overflow";
            trace::dci5_input_capture(capture_trace);
            return true;
        };
        capture_trace.dci5_output = Some(dci5_output_gpa);
        if !mem.write_bytes(dci5_output_gpa, &dci5_input_context) {
            capture_trace.reason = "published_dci5_output_write_failed";
            trace::dci5_input_capture(capture_trace);
            return true;
        }
        capture_trace.reason = "published";
        trace::dci5_input_capture(capture_trace);
        true
    }

    pub(super) fn write_slot1_dci5_output_dequeue(&mut self, mem: &mut dyn GuestMemoryMut) {
        let Some(output_context) =
            output_context_for_slot(mem, self.dcbaap & DCBAA_POINTER_MASK, SLOT_ID)
        else {
            return;
        };
        let Some(dci5_dequeue_gpa) = output_context
            .checked_add(DCI5_OUTPUT_CONTEXT_OFFSET)
            .and_then(|gpa| gpa.checked_add(EP_TR_DEQUEUE_OFFSET))
        else {
            return;
        };
        let _ = write_mem_u64(
            mem,
            dci5_dequeue_gpa,
            self.slot1_dci5_dequeue | u64::from(self.slot1_dci5_dcs),
        );
    }

    pub(super) fn slot1_dci5_output_dequeue_state(
        &self,
        mem: &dyn GuestMemoryMut,
    ) -> Option<(u64, bool)> {
        let output_context =
            output_context_for_slot(mem, self.dcbaap & DCBAA_POINTER_MASK, SLOT_ID)?;
        let dci5_dequeue_gpa = output_context
            .checked_add(DCI5_OUTPUT_CONTEXT_OFFSET)?
            .checked_add(EP_TR_DEQUEUE_OFFSET)?;
        let raw_dequeue = read_mem_u64(mem, dci5_dequeue_gpa)?;
        let dequeue = raw_dequeue & EP_TR_DEQUEUE_MASK;
        (dequeue != 0).then_some((dequeue, raw_dequeue & 1 != 0))
    }
}
