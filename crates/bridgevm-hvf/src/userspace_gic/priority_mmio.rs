//! Priority byte accesses and IRQ-line changes caused by guest writes.

use super::*;

impl UserspaceGic {
    pub(super) fn dist_priority_access(
        &mut self,
        offset: u64,
        width: u8,
        write: Option<u64>,
        kick_mask: &mut u64,
    ) -> Option<u64> {
        // A packed write can affect interrupts routed to different CPUs.
        let before = write.map(|_| {
            (0..self.num_cpus)
                .map(|cpu| self.line_asserted(cpu))
                .collect::<Vec<_>>()
        });
        let value = Self::priority_bytes_access(
            &mut self.dist.priority,
            GICD_IPRIORITYR,
            offset,
            width,
            write,
        );
        if value.is_some() {
            if let Some(before) = before {
                for (cpu, was_asserted) in before.into_iter().enumerate() {
                    *kick_mask |= self.kick_if_line_changed(cpu, was_asserted);
                }
            }
        }
        value
    }

    pub(super) fn redist_priority_access(
        &mut self,
        cpu: usize,
        offset: u64,
        width: u8,
        write: Option<u64>,
        kick_mask: &mut u64,
    ) -> Option<u64> {
        let before = write.map(|_| self.line_asserted(cpu));
        let value = Self::priority_bytes_access(
            &mut self.redists[cpu].priority,
            GICR_IPRIORITYR,
            offset,
            width,
            write,
        );
        if value.is_some() {
            if let Some(was_asserted) = before {
                *kick_mask |= self.kick_if_line_changed(cpu, was_asserted);
            }
        }
        value
    }

    fn priority_bytes_access(
        priorities: &mut [u8],
        base: u64,
        offset: u64,
        width: u8,
        write: Option<u64>,
    ) -> Option<u64> {
        let rel = offset.checked_sub(base)?;
        let start = rel as usize;
        let width = usize::from(width).clamp(1, 8);
        if start + width > priorities.len() {
            return None;
        }
        match write {
            Some(value) => {
                for (i, slot) in priorities[start..start + width].iter_mut().enumerate() {
                    *slot = ((value >> (i * 8)) & 0xff) as u8;
                }
                Some(0)
            }
            None => {
                let mut value = 0u64;
                for (i, slot) in priorities[start..start + width].iter().enumerate() {
                    value |= u64::from(*slot) << (i * 8);
                }
                Some(value)
            }
        }
    }
}
