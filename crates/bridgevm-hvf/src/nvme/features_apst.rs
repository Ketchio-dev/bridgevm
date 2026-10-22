//! GET FEATURES for Autonomous Power State Transition returns a 256-byte table.

use super::*;
use crate::fwcfg::GuestMemoryMut;

impl NvmeController {
    /// Writes the disabled (all-zero) APST table through the command's PRP entries, so a
    /// table that starts near the end of a page continues at PRP2 like every other admin
    /// payload. A command without a data pointer keeps the earlier lenient success.
    pub(crate) fn write_apst_table(
        &mut self,
        cmd: &SubmissionEntry,
        mem: &mut dyn GuestMemoryMut,
    ) -> u16 {
        if cmd.prp1 == 0 {
            return SC_SUCCESS;
        }
        self.write_admin_data(cmd, &ZERO_APST_FEATURE_DATA, mem)
    }
}
