//! SET/GET FEATURES handling, the feature-ID table, and feature capability reporting.

use super::*;

pub(crate) fn feature_capabilities(fid: u8) -> Option<u32> {
    match fid {
        FEATURE_TEMPERATURE_THRESHOLD
        | FEATURE_VOLATILE_WRITE_CACHE
        | FEATURE_NUMBER_OF_QUEUES
        | FEATURE_WRITE_ATOMICITY_NORMAL
        | FEATURE_ASYNC_EVENT_CONFIGURATION => Some(FEATURE_CAP_CHANGEABLE),
        FEATURE_ERROR_RECOVERY => Some(FEATURE_CAP_CHANGEABLE | FEATURE_CAP_NAMESPACE_SPECIFIC),
        FEATURE_ARBITRATION
        | FEATURE_POWER_MANAGEMENT
        | FEATURE_INTERRUPT_COALESCING
        | FEATURE_INTERRUPT_VECTOR_CONFIGURATION
        | FEATURE_AUTONOMOUS_POWER_STATE_TRANSITION => Some(0),
        _ => None,
    }
}

impl NvmeController {
    /// SET FEATURES (NVMe 1.4 §5.21). Keep the small set Windows probes aligned
    /// with QEMU defaults; unsupported features remain harmless no-ops here.
    pub(crate) fn admin_set_features(&mut self, cmd: &SubmissionEntry) -> u16 {
        let fid = (cmd.cdw10 & 0xff) as u8;
        match fid {
            FEATURE_NUMBER_OF_QUEUES => {
                // CDW11: NSQR bits 15:0, NCQR bits 31:16 (both 0-based requests).
                let nsqr = (cmd.cdw11 & 0xffff) as u16;
                let ncqr = ((cmd.cdw11 >> 16) & 0xffff) as u16;
                // Grant the smaller of each request and our capacity (all 0-based).
                let capacity = self.max_io_queues.saturating_sub(1);
                let sq_granted = nsqr.min(capacity);
                let cq_granted = ncqr.min(capacity);
                // The completion DW0 carries the allocated counts (0-based: NSQA in
                // bits 15:0, NCQA in bits 31:16); the generic completion path emits
                // it via `last_feature_result`.
                self.last_feature_result = (u32::from(cq_granted) << 16) | u32::from(sq_granted);
            }
            FEATURE_VOLATILE_WRITE_CACHE => {
                self.volatile_write_cache_enabled = (cmd.cdw11 & 1) != 0;
                if !self.volatile_write_cache_enabled {
                    let status = self.flush_all_namespaces();
                    if status != SC_SUCCESS {
                        return status;
                    }
                }
            }
            _ => {}
        }
        SC_SUCCESS
    }
}
