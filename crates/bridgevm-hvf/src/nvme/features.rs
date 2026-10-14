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
    /// SET FEATURES (NVMe 1.4 §5.21). Unsupported optional features remain
    /// harmless no-ops; stateful features are handled explicitly below.
    pub(crate) fn admin_set_features(&mut self, cmd: &SubmissionEntry) -> u16 {
        let fid = (cmd.cdw10 & 0xff) as u8;
        match fid {
            FEATURE_NUMBER_OF_QUEUES => {
                // NVMe 1.4 §5.21.1.7 permits allocating more than requested.
                // Our fixed allocation must match GET FEATURES and queue creation,
                // and must not change between resets. Both counts are zero-based.
                let granted = u32::from(self.max_io_queues.saturating_sub(1));
                self.last_feature_result = (granted << 16) | granted;
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
