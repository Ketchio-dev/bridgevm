//! Resolving guest memory and I/O-port addresses to a programmed BAR, with the MRU cache.

use super::*;

/// A decoded memory-space access into a programmed PCI BAR.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PcieMmioTarget {
    pub bdf: (u8, u8, u8),
    pub bar_index: usize,
    pub offset: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PcieMmioTargetMru {
    pub(crate) base: u64,
    pub(crate) end: u64,
    pub(crate) target: PcieMmioTarget,
}

/// A decoded I/O-space access into a programmed PCI BAR.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PciePioTarget {
    pub bdf: (u8, u8, u8),
    pub bar_index: usize,
    pub offset: u64,
}

impl PcieMmioTargetMru {
    pub(crate) fn target_for(self, gpa: u64) -> Option<PcieMmioTarget> {
        (self.base..self.end)
            .contains(&gpa)
            .then(|| PcieMmioTarget {
                offset: gpa - self.base,
                ..self.target
            })
    }
}

impl PcieEcam {
    /// Resolve an absolute guest-physical address in PCI memory space to the
    /// programmed endpoint BAR that decodes it. Only functions with Memory Space
    /// enabled in the PCI command register are allowed to answer.
    pub fn mmio_target(&self, gpa: u64) -> Option<PcieMmioTarget> {
        if let Some(mru) = self.mmio_mru.get() {
            if let Some(target) = mru.target_for(gpa) {
                return Some(target);
            }
        }
        for func in &self.functions {
            if func.command & CMD_MEMORY_SPACE == 0 {
                continue;
            }
            for idx in 0..func.bars.len() {
                if let Some(mru) = func.mmio_target_of_bar(idx, gpa) {
                    self.mmio_mru.set(Some(mru));
                    return Some(mru.target);
                }
            }
        }
        None
    }

    /// Resolve a PCI I/O-port address to the programmed endpoint BAR that
    /// decodes it. Only functions with I/O Space enabled in the command register
    /// are allowed to answer.
    pub fn pio_target(&self, port: u64) -> Option<PciePioTarget> {
        for func in &self.functions {
            if func.command & CMD_IO_SPACE == 0 {
                continue;
            }
            for (idx, bar) in func.bars.iter().enumerate() {
                if let Some(offset) = bar.pio_offset_of(port) {
                    return Some(PciePioTarget {
                        bdf: func.bdf,
                        bar_index: idx,
                        offset,
                    });
                }
            }
        }
        None
    }
}
