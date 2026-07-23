//! SET/GET FEATURES handling, the feature-ID table, and feature capability reporting.

use super::*;
use crate::fwcfg::GuestMemoryMut;

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
                    let _ = self.disk.flush();
                }
            }
            _ => {}
        }
        SC_SUCCESS
    }

    /// GET FEATURES (NVMe 1.4 §5.14). Windows probes several optional features
    /// during setup. Return boring, disabled defaults for the generic features
    /// this tiny controller can safely expose, and report invalid-field (not
    /// invalid-opcode) for reserved/vendor-specific feature IDs.
    pub(crate) fn admin_get_features(
        &mut self,
        cmd: &SubmissionEntry,
        mem: &mut dyn GuestMemoryMut,
    ) -> u16 {
        let fid = (cmd.cdw10 & 0xff) as u8;
        let select = (cmd.cdw10 >> GET_FEATURE_SELECT_SHIFT) & 0x7;
        if select == GET_FEATURE_SELECT_CAPABILITIES {
            let Some(capabilities) = feature_capabilities(fid) else {
                return SC_INVALID_FIELD_DNR;
            };
            self.last_feature_result = capabilities;
            return SC_SUCCESS;
        }
        let wants_default = matches!(
            select,
            GET_FEATURE_SELECT_DEFAULT | GET_FEATURE_SELECT_SAVED
        );
        let value = match fid {
            FEATURE_ARBITRATION => 0,
            FEATURE_POWER_MANAGEMENT => 0,
            FEATURE_TEMPERATURE_THRESHOLD => 0,
            FEATURE_ERROR_RECOVERY => 0,
            FEATURE_VOLATILE_WRITE_CACHE => {
                if wants_default {
                    0
                } else {
                    u32::from(self.volatile_write_cache_enabled)
                }
            }
            FEATURE_NUMBER_OF_QUEUES => {
                let granted = u32::from(self.max_io_queues.saturating_sub(1));
                (granted << 16) | granted
            }
            FEATURE_INTERRUPT_COALESCING => 0,
            FEATURE_INTERRUPT_VECTOR_CONFIGURATION => cmd.cdw11 & 0xffff,
            FEATURE_WRITE_ATOMICITY_NORMAL => 0,
            FEATURE_ASYNC_EVENT_CONFIGURATION => 0,
            FEATURE_AUTONOMOUS_POWER_STATE_TRANSITION => {
                if cmd.prp1 != 0 && !mem.write_bytes(cmd.prp1, &ZERO_APST_FEATURE_DATA) {
                    return SC_INVALID_FIELD;
                }
                0
            }
            _ => return SC_INVALID_FIELD_DNR,
        };
        self.last_feature_result = value;
        SC_SUCCESS
    }
}
