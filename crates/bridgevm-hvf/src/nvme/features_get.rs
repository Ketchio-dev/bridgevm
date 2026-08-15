//! GET FEATURES. Split out of features.rs, which also carries SET FEATURES and
//! the capability table.

use super::features::feature_capabilities;
use super::*;
use crate::fwcfg::GuestMemoryMut;

impl NvmeController {
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
