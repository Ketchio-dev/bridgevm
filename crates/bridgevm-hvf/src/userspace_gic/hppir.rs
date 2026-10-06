//! HPPIR reports enabled pending work independently of delivery priority.

use super::*;

impl UserspaceGic {
    pub(super) fn highest_pending_intid(&self, cpu: usize) -> u32 {
        if !self.ifaces[cpu].group1_enabled {
            return SPURIOUS_INTID;
        }
        self.highest_candidate(cpu, 0xff)
            .map(|candidate| candidate.intid)
            .unwrap_or(SPURIOUS_INTID)
    }
}
