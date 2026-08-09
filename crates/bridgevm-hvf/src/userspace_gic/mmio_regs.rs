//! GICD / GICR / GICv2m-frame MMIO register emulation.
//!
//! Split from `userspace_gic/mod.rs` (structural budget): the interrupt
//! state machine lives there; this file is the register plumbing.

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum WriteMode {
    Store,
    SetBits,
    ClearBits,
}

impl UserspaceGic {
    pub(super) fn msi_frame_mmio(&mut self, offset: u64, write: Option<u64>) -> UsGicMmioResult {
        match (offset, write) {
            (GICM_SET_SPI_NSR, Some(value)) => {
                let kick_mask = self.send_msi(
                    machine::GIC_MSI_FRAME.base + GICM_SET_SPI_NSR,
                    (value & 0x3ff) as u32,
                );
                UsGicMmioResult {
                    value: 0,
                    kick_mask,
                }
            }
            (GICM_TYPER, None) => UsGicMmioResult {
                // Base SPI and count of the MSI INTID window.
                value: (u64::from(machine::GIC_MSI_INTID_BASE) << 16)
                    | u64::from(machine::GIC_MSI_INTID_COUNT),
                kick_mask: 0,
            },
            (GICM_PIDR2, None) => UsGicMmioResult {
                value: 0x20, // GICv2m
                kick_mask: 0,
            },
            _ => UsGicMmioResult {
                value: 0,
                kick_mask: 0,
            },
        }
    }

    pub(super) fn read_u32_field(registers: &[u32], base: u64, offset: u64) -> Option<u64> {
        let index = offset.checked_sub(base)? / 4;
        let aligned = (offset - base) % 4 == 0;
        (aligned && (index as usize) < registers.len())
            .then(|| u64::from(registers[index as usize]))
    }

    pub(super) fn write_u32_field(
        registers: &mut [u32],
        base: u64,
        offset: u64,
        value: u32,
        mode: WriteMode,
    ) -> bool {
        let Some(rel) = offset.checked_sub(base) else {
            return false;
        };
        if rel % 4 != 0 {
            return false;
        }
        let index = (rel / 4) as usize;
        if index >= registers.len() {
            return false;
        }
        match mode {
            WriteMode::Store => registers[index] = value,
            WriteMode::SetBits => registers[index] |= value,
            WriteMode::ClearBits => registers[index] &= !value,
        }
        true
    }

    pub(super) fn priority_bytes_access(
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

    pub(super) fn dist_mmio(
        &mut self,
        offset: u64,
        width: u8,
        write: Option<u64>,
    ) -> UsGicMmioResult {
        let mut kick_mask = 0u64;
        let value = match (offset, write) {
            (GICD_CTLR, None) => u64::from(self.dist.ctlr | GICD_CTLR_ARE_NS | GICD_CTLR_DS),
            (GICD_CTLR, Some(value)) => {
                self.dist.ctlr = (value as u32) & 0x3;
                kick_mask = self.all_cpus_mask();
                0
            }
            // ITLinesNumber for 256 INTIDs, IDbits=7 (256), no security ext.
            (GICD_TYPER, None) => ((GIC_INTID_COUNT as u64 / 32) - 1) | (7 << 19),
            (GICD_IIDR, None) => IIDR,
            (GICD_STATUSR, None) | (GICD_STATUSR, Some(_)) => 0,
            (GICD_PIDR2, None) => PIDR2_GICV3,
            _ => {
                // RAZ/WI for everything unmodeled.
                self.dist_banked_mmio(offset, width, write, &mut kick_mask)
                    .unwrap_or_default()
            }
        };
        UsGicMmioResult { value, kick_mask }
    }

    pub(super) fn dist_banked_mmio(
        &mut self,
        offset: u64,
        width: u8,
        write: Option<u64>,
        kick_mask: &mut u64,
    ) -> Option<u64> {
        // IROUTER: 64-bit registers, one per INTID.
        if (GICD_IROUTER..GICD_IROUTER + (GIC_INTID_COUNT as u64) * 8).contains(&offset) {
            let rel = offset - GICD_IROUTER;
            let intid = (rel / 8) as usize;
            let high_word = rel % 8 == 4;
            match write {
                Some(value) => {
                    let current = self.dist.route[intid];
                    self.dist.route[intid] = if width >= 8 {
                        value
                    } else if high_word {
                        (current & 0xffff_ffff) | (value << 32)
                    } else {
                        (current & !0xffff_ffff) | (value & 0xffff_ffff)
                    };
                    *kick_mask = self.all_cpus_mask();
                    return Some(0);
                }
                None => {
                    let current = self.dist.route[intid];
                    return Some(if width >= 8 {
                        current
                    } else if high_word {
                        current >> 32
                    } else {
                        current & 0xffff_ffff
                    });
                }
            }
        }
        if (GICD_IPRIORITYR..GICD_IPRIORITYR + GIC_INTID_COUNT as u64).contains(&offset) {
            return Self::priority_bytes_access(
                &mut self.dist.priority,
                GICD_IPRIORITYR,
                offset,
                width,
                write,
            );
        }
        let value32 = write.map(|v| v as u32);
        let table: [(u64, WriteMode, bool); 9] = [
            (GICD_IGROUPR, WriteMode::Store, true),
            (GICD_ISENABLER, WriteMode::SetBits, true),
            (GICD_ICENABLER, WriteMode::ClearBits, true),
            (GICD_ISPENDR, WriteMode::SetBits, true),
            (GICD_ICPENDR, WriteMode::ClearBits, true),
            (GICD_ISACTIVER, WriteMode::SetBits, true),
            (GICD_ICACTIVER, WriteMode::ClearBits, true),
            (GICD_ICFGR, WriteMode::Store, false),
            (GICD_IGRPMODR, WriteMode::Store, false),
        ];
        for (base, mode, kicks) in table {
            let registers: &mut [u32] = match base {
                GICD_IGROUPR => &mut self.dist.group,
                GICD_ISENABLER | GICD_ICENABLER => &mut self.dist.enabled,
                GICD_ISPENDR | GICD_ICPENDR => &mut self.dist.pending,
                GICD_ISACTIVER | GICD_ICACTIVER => &mut self.dist.active,
                GICD_ICFGR => &mut self.dist.icfgr,
                GICD_IGRPMODR => &mut self.dist.grpmod,
                _ => unreachable!(),
            };
            let span = (registers.len() as u64) * 4;
            if !(base..base + span).contains(&offset) {
                continue;
            }
            match value32 {
                Some(value) => {
                    if Self::write_u32_field(registers, base, offset, value, mode) {
                        if kicks {
                            *kick_mask = self.all_cpus_mask();
                        }
                        return Some(0);
                    }
                }
                None => {
                    // ICENABLER/ICPENDR/ICACTIVER read the same underlying
                    // state as their set-side twins.
                    if let Some(value) = Self::read_u32_field(registers, base, offset) {
                        return Some(value);
                    }
                }
            }
        }
        None
    }

    pub(super) fn redist_mmio(
        &mut self,
        cpu: usize,
        offset: u64,
        width: u8,
        write: Option<u64>,
    ) -> UsGicMmioResult {
        let mut kick_mask = 0u64;
        let last = cpu == self.num_cpus - 1;
        let value = match (offset, write) {
            (GICR_CTLR, _) => 0,
            (GICR_IIDR, None) => IIDR,
            (GICR_TYPER, None) | (0x000c, None) => {
                let affinity = machine::cpu_mpidr(cpu as u64) & 0xff_ffff;
                let typer = (affinity << 32) | ((cpu as u64) << 8) | (u64::from(last) << 4);
                if offset == GICR_TYPER && width >= 8 {
                    typer
                } else if offset == GICR_TYPER {
                    typer & 0xffff_ffff
                } else {
                    typer >> 32
                }
            }
            (GICR_STATUSR, _) => 0,
            (GICR_WAKER, None) => u64::from(self.redists[cpu].waker),
            (GICR_WAKER, Some(value)) => {
                let sleeping = value as u32 & GICR_WAKER_PROCESSOR_SLEEP != 0;
                self.redists[cpu].waker = if sleeping {
                    GICR_WAKER_PROCESSOR_SLEEP | GICR_WAKER_CHILDREN_ASLEEP
                } else {
                    0
                };
                kick_mask = 1u64 << cpu;
                0
            }
            (GICR_PROPBASER, None) => self.redists[cpu].propbaser,
            (GICR_PROPBASER, Some(value)) => {
                self.redists[cpu].propbaser = value;
                0
            }
            (GICR_PENDBASER, None) => self.redists[cpu].pendbaser,
            (GICR_PENDBASER, Some(value)) => {
                self.redists[cpu].pendbaser = value;
                0
            }
            (GICR_PIDR2, None) => PIDR2_GICV3,
            (GICR_IGROUPR0, None) => u64::from(self.redists[cpu].group0),
            (GICR_IGROUPR0, Some(value)) => {
                if value as u32 != u32::MAX {
                    // A guest moving SGIs/PPIs to Group 0 (FIQ) would silently
                    // lose them: this model only delivers Group 1. Shout so a
                    // boot park can be attributed instead of guessed at.
                    println!(
                        "USGIC cpu{cpu} IGROUPR0 write {value:#x}: Group0 requested, UNSUPPORTED"
                    );
                }
                self.redists[cpu].group0 = value as u32;
                kick_mask = 1u64 << cpu;
                0
            }
            (GICR_ISENABLER0, None) | (GICR_ICENABLER0, None) => {
                u64::from(self.redists[cpu].enabled0)
            }
            (GICR_ISENABLER0, Some(value)) => {
                self.redists[cpu].enabled0 |= value as u32;
                kick_mask = 1u64 << cpu;
                0
            }
            (GICR_ICENABLER0, Some(value)) => {
                self.redists[cpu].enabled0 &= !(value as u32);
                kick_mask = 1u64 << cpu;
                0
            }
            (GICR_ISPENDR0, None) | (GICR_ICPENDR0, None) => {
                u64::from(self.redists[cpu].effective_pending())
            }
            (GICR_ISPENDR0, Some(value)) => {
                self.redists[cpu].pending0 |= value as u32;
                kick_mask = 1u64 << cpu;
                0
            }
            (GICR_ICPENDR0, Some(value)) => {
                self.redists[cpu].pending0 &= !(value as u32);
                kick_mask = 1u64 << cpu;
                0
            }
            (GICR_ISACTIVER0, None) | (GICR_ICACTIVER0, None) => {
                u64::from(self.redists[cpu].active0)
            }
            (GICR_ISACTIVER0, Some(value)) => {
                self.redists[cpu].active0 |= value as u32;
                0
            }
            (GICR_ICACTIVER0, Some(value)) => {
                self.redists[cpu].active0 &= !(value as u32);
                kick_mask = 1u64 << cpu;
                0
            }
            (GICR_ICFGR0, None) => u64::from(self.redists[cpu].icfgr[0]),
            (GICR_ICFGR1, None) => u64::from(self.redists[cpu].icfgr[1]),
            (GICR_ICFGR0, Some(value)) => {
                self.redists[cpu].icfgr[0] = value as u32;
                0
            }
            (GICR_ICFGR1, Some(value)) => {
                self.redists[cpu].icfgr[1] = value as u32;
                0
            }
            (GICR_IGRPMODR0, None) => u64::from(self.redists[cpu].grpmod0),
            (GICR_IGRPMODR0, Some(value)) => {
                self.redists[cpu].grpmod0 = value as u32;
                0
            }
            _ => {
                if (GICR_IPRIORITYR..GICR_IPRIORITYR + 32).contains(&offset) {
                    Self::priority_bytes_access(
                        &mut self.redists[cpu].priority,
                        GICR_IPRIORITYR,
                        offset,
                        width,
                        write,
                    )
                    .unwrap_or(0)
                } else {
                    0 // RAZ/WI.
                }
            }
        };
        UsGicMmioResult { value, kick_mask }
    }
}
