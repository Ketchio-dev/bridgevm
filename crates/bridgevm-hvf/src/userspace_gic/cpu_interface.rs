//! CPU interface state and Group 1 preemption priority boundary.

use super::*;

impl CpuInterface {
    pub(super) fn new() -> Self {
        Self {
            ctlr: 0,
            priority_mask: 0,
            bpr0: 0,
            bpr1: 0,
            group0_enabled: false,
            group1_enabled: false,
            active: Vec::new(),
            ap0r: [0; 4],
            ap1r: [0; 4],
        }
    }

    pub(super) fn running_priority(&self) -> u8 {
        self.active
            .iter()
            .filter(|a| !a.priority_dropped)
            .map(|a| a.priority)
            .min()
            .unwrap_or(0xff)
    }

    pub(super) fn threshold(&self) -> u8 {
        let running = self.running_priority();
        // G1NS BPR1=1 has group bits [7:1]; each higher value adds a subpriority bit.
        let preemption = if running == 0xff {
            0xff
        } else {
            running & (u8::MAX << self.bpr1.max(1))
        };
        self.priority_mask.min(preemption)
    }

    pub(super) fn eoi_mode(&self) -> bool {
        self.ctlr & ICC_CTLR_EOIMODE != 0
    }
}
