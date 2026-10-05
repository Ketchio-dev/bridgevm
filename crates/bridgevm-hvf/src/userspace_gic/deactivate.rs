//! Interrupt deactivation and notification of a changed SPI target line.

use super::*;

impl UserspaceGic {
    pub(super) fn deactivate(&mut self, cpu: usize, intid: u32) -> u64 {
        // An SPI can be retargeted while active, or deactivated by another
        // CPU after priority drop. Its current target needs a fresh IRQ pin
        // when clearing shared active state makes pending work deliverable.
        let target = if (SPI_BASE as u32..GIC_INTID_COUNT as u32).contains(&intid) {
            self.route_target(intid as usize)
                .map(|target| (target, self.line_asserted(target)))
        } else {
            None
        };
        if let Some(position) = self.ifaces[cpu]
            .active
            .iter()
            .rposition(|a| a.intid == intid)
        {
            self.ifaces[cpu].active.remove(position);
        }
        let intid = intid as usize;
        if intid < 32 {
            self.redists[cpu].active0 &= !(1u32 << intid);
        } else if intid < GIC_INTID_COUNT {
            let (reg, bit) = Distributor::bit(intid);
            self.dist.active[reg] &= !bit;
        }
        target
            .map(|(target, was_asserted)| self.kick_if_line_changed(target, was_asserted))
            .unwrap_or(0)
    }
}
