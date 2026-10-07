//! Interrupt acknowledgement (ICC_IAR1_EL1) into the CPU interface active list.
//! Arm IHI0069G §12.2.11: acknowledging moves the interrupt to active and raises
//! the running priority until priority drop.

use super::*;

impl UserspaceGic {
    pub(super) fn acknowledge(&mut self, cpu: usize) -> u32 {
        let iface = &self.ifaces[cpu];
        if !iface.group1_enabled {
            return SPURIOUS_INTID;
        }
        let Some(candidate) = self.highest_candidate(cpu, iface.threshold()) else {
            return SPURIOUS_INTID;
        };
        let intid = candidate.intid as usize;
        if intid < 32 {
            let redist = &mut self.redists[cpu];
            let bit = 1u32 << intid;
            redist.pending0 &= !bit;
            redist.active0 |= bit;
        } else {
            let (reg, bit) = Distributor::bit(intid);
            self.dist.pending[reg] &= !bit;
            self.dist.active[reg] |= bit;
        }
        // An interrupt is acknowledged only while inactive. A priority-dropped
        // entry for it was therefore already deactivated, e.g. by
        // GICD_ICACTIVER instead of ICC_DIR_EL1, and no longer holds an
        // active priority. Retire it so the list stays bounded.
        let active = &mut self.ifaces[cpu].active;
        active.retain(|entry| !(entry.priority_dropped && entry.intid == candidate.intid));
        active.push(ActiveInterrupt {
            intid: candidate.intid,
            priority: candidate.priority,
            priority_dropped: false,
        });
        candidate.intid
    }
}
