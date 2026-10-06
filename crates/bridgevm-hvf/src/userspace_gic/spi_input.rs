//! Electrical SPI input sampling and edge-pending latching.

use super::*;

impl UserspaceGic {
    /// Device SPI level (virtio INTx and friends). `intid` is absolute.
    pub fn set_spi(&mut self, intid: u32, level: bool) -> u64 {
        let intid = intid as usize;
        if !(SPI_BASE..GIC_INTID_COUNT).contains(&intid) {
            return 0;
        }
        // Line state BEFORE mutation: the kick decision needs the edge.
        let target = self.route_target(intid);
        let was_line = target.map(|cpu| self.line_asserted(cpu));
        let (reg, bit) = Distributor::bit(intid);
        // Edge-configured SPIs latch into pending on a rising edge; level
        // SPIs track the input. ICFGR bit (2*intid%32+1): 1 = edge.
        let cfg_reg = intid / 16;
        let edge = self.dist.icfgr[cfg_reg] >> ((intid % 16) * 2 + 1) & 1 != 0;
        // Acknowledgement or pending clear must not rearm an input still high.
        let was = self.dist.input_level[reg] & bit != 0;
        if level {
            self.dist.input_level[reg] |= bit;
            self.dist.level[reg] |= bit;
            if edge && !was {
                self.dist.pending[reg] |= bit;
            }
        } else {
            self.dist.input_level[reg] &= !bit;
            self.dist.level[reg] &= !bit;
        }
        if edge {
            // Edge input drops do not clear latched pending.
            self.dist.level[reg] &= !bit;
            if !level {
                return 0;
            }
        }
        match (target, was_line) {
            (Some(cpu), Some(was)) => self.kick_if_line_changed(cpu, was),
            _ => 0,
        }
    }
}
