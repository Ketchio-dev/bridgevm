//! The vtimer-PPI verdict table, split from the capture struct so the
//! decision logic reads as one page.

use super::{GicIrqState, VTIMER_PPI_BIT};

impl GicIrqState {
    /// Why the vtimer PPI is not being taken, judged from the GIC alone.
    /// `None` when a needed register could not be read: no verdict beats a
    /// fabricated one.
    pub(crate) fn vtimer_verdict(&self) -> Option<&'static str> {
        let enabled = self.isenabler0? & u64::from(VTIMER_PPI_BIT) != 0;
        let pending = self.ispendr0? & u64::from(VTIMER_PPI_BIT) != 0;
        // Which group PPI 27 is in decides which enable gates it. The first
        // live capture (t4-soak boot-1, 2026-08-06) showed pending + IGRPEN1
        // on + PMR open and still no delivery -- the group split was the one
        // gate the verdict had not looked at.
        let in_group1 = self.igroupr0? & u64::from(VTIMER_PPI_BIT) != 0;
        let group_on = if in_group1 { self.igrpen1? & 1 != 0 } else { self.igrpen0? & 1 != 0 };
        // PMR of 0 masks every priority; the reset value before the guest
        // programs it. Anything nonzero admits at least priority 0.
        let pmr_open = self.pmr? != 0;
        Some(match (enabled, pending, group_on, pmr_open) {
            (false, _, _, _) => "vtimer PPI disabled at GICR_ISENABLER0",
            (true, false, _, _) => "vtimer PPI enabled but not pending at the GICR",
            (true, true, false, _) if in_group1 => "vtimer PPI pending but ICC group 1 is off",
            (true, true, false, _) => "vtimer PPI pending in group 0 but ICC group 0 is off",
            (true, true, true, false) => "vtimer PPI pending but ICC_PMR masks all priorities",
            (true, true, true, true) => "vtimer PPI pending and deliverable; the block is not the GIC",
        })
    }
}
