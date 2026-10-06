//! SGI target selection within the advertised affinity topology.

use super::*;

impl UserspaceGic {
    pub(super) fn sgi1r_targets(&self, value: u64) -> Vec<usize> {
        if value & (1 << 40) != 0 {
            // IRM: all but self — caller filters self out.
            return (0..self.num_cpus).collect();
        }
        let target_list = value & 0xffff;
        let aff1 = (value >> 16) & 0xff;
        let aff2 = (value >> 32) & 0xff;
        (0..self.num_cpus)
            .filter(|&c| {
                let mpidr = machine::cpu_mpidr(c as u64);
                let cpu_aff1 = (mpidr >> 8) & 0xff;
                let cpu_aff2 = (mpidr >> 16) & 0xff;
                let cpu_aff0 = mpidr & 0xff;
                cpu_aff2 == aff2
                    && cpu_aff1 == aff1
                    && cpu_aff0 < 16
                    && target_list & (1 << cpu_aff0) != 0
            })
            .collect()
    }
}
