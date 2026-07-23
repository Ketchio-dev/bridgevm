//! Base Address Register model: sizing probe, read/write latch, address-to-offset resolution.

use super::*;

/// One Base Address Register and the size of the region it can decode.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Bar {
    /// Latched BAR value (low config/type bits OR'd with the programmed base).
    pub(crate) value: u32,
    /// Size mask: `!(size - 1)` for a power-of-two `size`, or `0` if the BAR is
    /// unimplemented. During the sizing probe the device returns this mask.
    pub(crate) size_mask: u32,
    /// Non-writable low type bits (memory/IO, 32/64-bit, prefetch) kept across
    /// a base re-program and the sizing probe.
    pub(crate) type_bits: u32,
    /// Whether the last write was the all-ones BAR sizing probe.  Inferring
    /// this from `value == size_mask | type_bits` is incorrect because a valid
    /// address at the top of an aperture can have exactly that bit pattern.
    pub(crate) sizing_probe: bool,
    pub(crate) kind: BarKind,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum BarKind {
    #[default]
    Memory32,
    Memory64Low,
    Memory64High,
    Io,
}

impl Bar {
    /// Construct a 32-bit, non-prefetchable memory BAR with a power-of-two size.
    pub(crate) fn memory32(size: u32) -> Self {
        assert!(
            size >= 0x10,
            "PCI memory BAR size must be at least 16 bytes"
        );
        assert!(
            size.is_power_of_two(),
            "PCI memory BAR size must be a power of two"
        );
        Self {
            value: 0,
            size_mask: !(size - 1),
            type_bits: 0,
            sizing_probe: false,
            kind: BarKind::Memory32,
        }
    }

    pub(crate) fn memory64(size: u32) -> (Self, Self) {
        Self::memory64_with_type_bits(size, 0x4)
    }

    pub(crate) fn memory64_prefetchable(size: u32) -> (Self, Self) {
        Self::memory64_with_type_bits(size, 0x0c)
    }

    pub(crate) fn memory64_with_type_bits(size: u32, low_type_bits: u32) -> (Self, Self) {
        assert!(
            size >= 0x10,
            "PCI memory BAR size must be at least 16 bytes"
        );
        assert!(
            size.is_power_of_two(),
            "PCI memory BAR size must be a power of two"
        );
        (
            Self {
                value: 0,
                size_mask: !(size - 1),
                type_bits: low_type_bits,
                sizing_probe: false,
                kind: BarKind::Memory64Low,
            },
            Self {
                value: 0,
                size_mask: 0xFFFF_FFFF,
                type_bits: 0,
                sizing_probe: false,
                kind: BarKind::Memory64High,
            },
        )
    }

    /// Construct an I/O BAR with a power-of-two size.
    pub(crate) fn io(size: u32) -> Self {
        assert!(size >= 0x4, "PCI I/O BAR size must be at least 4 bytes");
        assert!(
            size.is_power_of_two(),
            "PCI I/O BAR size must be a power of two"
        );
        Self {
            value: 0,
            size_mask: !(size - 1),
            type_bits: 0x1,
            sizing_probe: false,
            kind: BarKind::Io,
        }
    }

    /// Read back the BAR. After an all-ones sizing write the latched value is
    /// the size mask; otherwise it is the programmed base. Unimplemented BARs
    /// always read `0`.
    pub(crate) fn read(&self) -> u32 {
        if self.size_mask == 0 {
            0
        } else {
            self.value
        }
    }

    /// Apply a 32-bit BAR write. Writing all-ones latches the size mask (the
    /// sizing protocol); any other value latches the base with the type bits
    /// preserved.
    pub(crate) fn write(&mut self, value: u32) {
        if self.size_mask == 0 {
            return; // unimplemented: writes are dropped
        }
        if value == 0xFFFF_FFFF {
            // Sizing probe: report `size_mask | type_bits` on read-back.
            self.sizing_probe = true;
            self.value = self.size_mask | self.type_bits;
        } else {
            // Program a base: only the address bits above the size are kept.
            self.sizing_probe = false;
            self.value = (value & self.size_mask) | self.type_bits;
        }
    }

    /// Size of the decoded BAR region, or zero if unimplemented.
    pub(crate) fn size(&self) -> u64 {
        if self.size_mask == 0 {
            0
        } else {
            let mask = match self.kind {
                BarKind::Memory32 | BarKind::Memory64Low => self.size_mask & !0xF,
                BarKind::Memory64High => return 0,
                BarKind::Io => self.size_mask & !0x3,
            };
            u64::from((!mask).wrapping_add(1))
        }
    }

    /// Programmed base, if the BAR is implemented.
    pub(crate) fn base(&self) -> Option<u64> {
        if self.size_mask == 0 {
            return None;
        }
        let mask = match self.kind {
            BarKind::Memory32 | BarKind::Memory64Low => !0xF,
            BarKind::Memory64High => return None,
            BarKind::Io => !0x3,
        };
        Some(u64::from(self.value & self.size_mask & mask))
    }

    pub(crate) fn assigned_base(&self) -> Option<u64> {
        let base = self.base()?;
        match self.kind {
            BarKind::Memory32 | BarKind::Memory64Low => {
                (base != 0 && !self.sizing_probe).then_some(base)
            }
            BarKind::Io => (!self.sizing_probe).then_some(base),
            BarKind::Memory64High => None,
        }
    }

    /// Offset into this BAR for `addr`, if the BAR currently decodes it.
    pub(crate) fn offset_of(&self, addr: u64) -> Option<u64> {
        let base = self.assigned_base()?;
        let size = self.size();
        let offset = addr.checked_sub(base)?;
        (offset < size).then_some(offset)
    }

    pub(crate) fn pio_offset_of(&self, port: u64) -> Option<u64> {
        (self.kind == BarKind::Io)
            .then(|| self.offset_of(port))
            .flatten()
    }
}

impl Function {
    pub(crate) fn memory64_assigned_base(&self, idx: usize) -> Option<u64> {
        let low = self.bars.get(idx)?;
        let high = self.bars.get(idx + 1)?;
        if low.kind != BarKind::Memory64Low || high.kind != BarKind::Memory64High {
            return None;
        }
        if low.sizing_probe || high.sizing_probe {
            return None;
        }
        let base = (u64::from(high.value) << 32) | low.base()?;
        (base != 0).then_some(base)
    }

    pub(crate) fn mmio_target_of_bar(&self, idx: usize, gpa: u64) -> Option<PcieMmioTargetMru> {
        let bar = self.bars.get(idx)?;
        let (base, size) = match bar.kind {
            BarKind::Memory32 => (bar.assigned_base()?, bar.size()),
            BarKind::Memory64Low => (self.memory64_assigned_base(idx)?, bar.size()),
            BarKind::Memory64High | BarKind::Io => return None,
        };
        let end = base.checked_add(size)?;
        let offset = gpa.checked_sub(base)?;
        (offset < size).then_some(PcieMmioTargetMru {
            base,
            end,
            target: PcieMmioTarget {
                bdf: self.bdf,
                bar_index: idx,
                offset,
            },
        })
    }
}
