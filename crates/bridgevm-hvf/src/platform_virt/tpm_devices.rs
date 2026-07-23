//! TPM TIS/PPI statistics and the memory-overwrite request surface.

use super::*;
use crate::tpm_ppi::TpmPpi;
use crate::tpm_ppi::TpmPpiStats;
use crate::tpm_tis::TpmTis;
use crate::tpm_tis::TpmTisStats;

impl VirtPlatform {
    pub fn tpm_tis_stats(&self) -> Option<TpmTisStats> {
        self.tpm_tis.as_ref().map(TpmTis::stats)
    }

    pub fn tpm_ppi_stats(&self) -> Option<TpmPpiStats> {
        self.tpm_ppi.as_ref().map(TpmPpi::stats)
    }

    pub fn tpm_memory_overwrite_requested(&self) -> bool {
        self.tpm_ppi
            .as_ref()
            .is_some_and(TpmPpi::memory_overwrite_requested)
    }
}
