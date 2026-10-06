//! Active Priorities registers derived from the acknowledged-interrupt list.
//! Arm IHI0069G §4.8.3: the encoding is IMPLEMENTATION DEFINED and zero means
//! no active priority. Group 1 bit n of the 128-bit view is group priority 2n,
//! as in QEMU with seven preemption bits at the minimum binary point. Group 0
//! is never acknowledged by this model, so its registers stay idle.

use super::*;

impl CpuInterface {
    fn active_priority_bits(&self) -> impl Iterator<Item = usize> + '_ {
        self.active
            .iter()
            .filter(|active| !active.priority_dropped)
            .map(|active| usize::from(active.priority >> 1))
    }

    pub(super) fn active_priorities(&self, group1: bool, n: usize) -> u64 {
        if !group1 {
            return 0;
        }
        self.active_priority_bits()
            .filter(|bit| bit / 32 == n)
            .fold(0, |word, bit| word | 1 << (bit % 32))
    }

    /// Clearing a reported bit drops that priority; returns whether one dropped.
    /// Setting a bit with no acknowledged interrupt behind it is ignored.
    pub(super) fn write_active_priorities(&mut self, group1: bool, n: usize, value: u64) -> bool {
        let mut dropped = false;
        for active in self
            .active
            .iter_mut()
            .filter(|a| group1 && !a.priority_dropped)
        {
            let bit = usize::from(active.priority >> 1);
            if bit / 32 == n && value & (1 << (bit % 32)) == 0 {
                active.priority_dropped = true;
                dropped = true;
            }
        }
        dropped
    }
}
