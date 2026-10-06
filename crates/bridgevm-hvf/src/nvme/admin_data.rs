//! PRP placement for existing admin command response payloads.

use super::*;
use crate::fwcfg::GuestMemoryMut;

impl NvmeController {
    pub(crate) fn write_admin_data(
        &mut self,
        cmd: &SubmissionEntry,
        data: &[u8],
        mem: &mut dyn GuestMemoryMut,
    ) -> u16 {
        let mut spans = std::mem::take(&mut self.prp_spans_scratch);
        spans.clear();
        let mut status = SC_INVALID_FIELD;
        if prp_spans_into(cmd, data.len(), mem, &mut spans, &mut self.io_scratch) {
            status = SC_SUCCESS;
            let mut offset = 0;
            for &(gpa, len) in &spans {
                if !mem.write_bytes(gpa, &data[offset..offset + len]) {
                    status = SC_INVALID_FIELD;
                    break;
                }
                offset += len;
            }
        }
        spans.clear();
        self.prp_spans_scratch = spans;
        status
    }
}
