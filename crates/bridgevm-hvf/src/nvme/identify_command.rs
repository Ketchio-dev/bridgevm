//! Identify command selection and PRP-scattered data transfer.

use super::*;
use crate::fwcfg::GuestMemoryMut;

impl NvmeController {
    /// IDENTIFY (CNS in CDW10 bits 7:0). Scatter its 4 KiB structure through PRPs.
    pub(crate) fn admin_identify(
        &mut self,
        cmd: &SubmissionEntry,
        mem: &mut dyn GuestMemoryMut,
    ) -> u16 {
        let cns = cmd.cdw10 & 0xff;
        let data = match cns {
            IDENTIFY_CNS_CONTROLLER => self.identify_controller(),
            IDENTIFY_CNS_COMMAND_SET_CONTROLLER => {
                let csi = ((cmd.cdw11 >> 24) & 0xff) as u8;
                if csi != COMMAND_SET_NVM {
                    return SC_INVALID_FIELD;
                }
                self.identify_command_set_controller()
            }
            IDENTIFY_CNS_ACTIVE_NAMESPACE_LIST => self.identify_active_namespace_list(cmd.nsid),
            IDENTIFY_CNS_NAMESPACE_DESCRIPTOR_LIST => {
                if self.backend_for_nsid(cmd.nsid).is_some() {
                    self.identify_namespace_descriptor_list(cmd.nsid)
                } else {
                    return SC_INVALID_FIELD;
                }
            }
            IDENTIFY_CNS_NAMESPACE => {
                if self.backend_for_nsid(cmd.nsid).is_some() {
                    self.identify_namespace(cmd.nsid)
                } else {
                    // Unallocated namespace ⇒ a zeroed structure (NVMe 1.4).
                    [0u8; PAGE_SIZE]
                }
            }
            _ => return SC_INVALID_FIELD,
        };
        if nvme_trace_enabled() {
            let label = identify_cns_name(cns);
            let preview_len = data.len().min(32);
            println!(
                "NVME identify {label} cns={cns:#x} nsid={} len={} first={} block_count={}",
                cmd.nsid,
                data.len(),
                hex_preview(&data[..preview_len]),
                self.block_count()
            );
        }
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
