//! TPM 2.0 Physical Presence Interface shared-memory page.
//!
//! TCG PPI 1.3 and QEMU expose a 0x400-byte, little-endian memory window next
//! to the TIS registers. Firmware and the ACPI `_DSM` exchange request/result
//! fields through this window. It is RAM, not a register file: bytes survive a
//! platform reset so firmware can consume an OS request on the next boot.

pub const MMIO_SIZE: u64 = 0x400;

/// One policy byte for each PPI operation number (FUNC[0..=255]).
pub const FUNC_OFFSET: usize = 0x000;
pub const FUNC_COUNT: usize = 0x100;

/// PPI parameter-block fields, matching QEMU/TCG PPI 1.3.
pub const PPIN_OFFSET: usize = 0x100;
pub const PPIP_OFFSET: usize = 0x101;
pub const PPRP_OFFSET: usize = 0x105;
pub const PPRQ_OFFSET: usize = 0x109;
pub const PPRM_OFFSET: usize = 0x10d;
pub const LPPR_OFFSET: usize = 0x111;
pub const MOVV_OFFSET: usize = 0x15a;

pub const FUNC_NOT_IMPLEMENTED: u8 = 0;
pub const FUNC_BIOS_ONLY: u8 = 1;
pub const FUNC_BLOCKED: u8 = 2;
pub const FUNC_ALLOWED_USER_REQUIRED: u8 = 3;
pub const FUNC_ALLOWED_USER_NOT_REQUIRED: u8 = 4;
pub const FUNC_MASK: u8 = 7;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TpmPpiStats {
    pub reads: u64,
    pub writes: u64,
    pub rejected_accesses: u64,
}

/// Guest-visible PPI shared memory.
///
/// The function-policy array starts as all `NOT_IMPLEMENTED`. Firmware may
/// populate it before handing control to the OS. `reset_runtime_state` keeps
/// the bytes intact, matching QEMU's RAM-backed PPI handoff across reboot.
#[derive(Debug, Clone)]
pub struct TpmPpi {
    bytes: [u8; MMIO_SIZE as usize],
    stats: TpmPpiStats,
}

impl Default for TpmPpi {
    fn default() -> Self {
        Self::new()
    }
}

impl TpmPpi {
    pub fn new() -> Self {
        Self {
            bytes: [0; MMIO_SIZE as usize],
            stats: TpmPpiStats::default(),
        }
    }

    pub fn stats(&self) -> TpmPpiStats {
        self.stats
    }

    pub fn bytes(&self) -> &[u8; MMIO_SIZE as usize] {
        &self.bytes
    }

    /// Reset transient accounting while preserving the firmware/OS mailbox.
    pub fn reset_runtime_state(&mut self) {
        self.stats = TpmPpiStats::default();
    }

    pub fn memory_overwrite_requested(&self) -> bool {
        self.bytes[MOVV_OFFSET] & 1 != 0
    }

    pub fn mmio_read(&mut self, offset: u64, size: u8) -> u64 {
        let Some(range) = access_range(offset, size) else {
            self.stats.rejected_accesses = self.stats.rejected_accesses.saturating_add(1);
            return u64::MAX;
        };
        self.stats.reads = self.stats.reads.saturating_add(1);
        let mut value = 0u64;
        for (shift, byte) in self.bytes[range].iter().enumerate() {
            value |= u64::from(*byte) << (shift * 8);
        }
        value
    }

    pub fn mmio_write(&mut self, offset: u64, size: u8, value: u64) {
        let Some(range) = access_range(offset, size) else {
            self.stats.rejected_accesses = self.stats.rejected_accesses.saturating_add(1);
            return;
        };
        self.stats.writes = self.stats.writes.saturating_add(1);
        for (shift, byte) in self.bytes[range].iter_mut().enumerate() {
            *byte = (value >> (shift * 8)) as u8;
        }
    }
}

fn access_range(offset: u64, size: u8) -> Option<std::ops::Range<usize>> {
    if !matches!(size, 1 | 2 | 4 | 8) {
        return None;
    }
    let start = usize::try_from(offset).ok()?;
    let end = start.checked_add(usize::from(size))?;
    (end <= MMIO_SIZE as usize).then_some(start..end)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parameter_block_is_little_endian_shared_memory() {
        let mut ppi = TpmPpi::new();
        ppi.mmio_write(PPRQ_OFFSET as u64, 4, 0x1122_3344);
        ppi.mmio_write(PPRM_OFFSET as u64, 4, 0xaabb_ccdd);

        assert_eq!(ppi.mmio_read(PPRQ_OFFSET as u64, 4), 0x1122_3344);
        assert_eq!(ppi.mmio_read(PPRM_OFFSET as u64, 4), 0xaabb_ccdd);
        assert_eq!(
            &ppi.bytes()[PPRQ_OFFSET..PPRQ_OFFSET + 4],
            &[0x44, 0x33, 0x22, 0x11]
        );
        assert_eq!(ppi.stats().reads, 2);
        assert_eq!(ppi.stats().writes, 2);
    }

    #[test]
    fn function_policy_and_movv_are_guest_visible() {
        let mut ppi = TpmPpi::new();
        assert_eq!(ppi.mmio_read(23, 1), u64::from(FUNC_NOT_IMPLEMENTED));

        ppi.mmio_write(23, 1, u64::from(FUNC_ALLOWED_USER_REQUIRED));
        ppi.mmio_write(MOVV_OFFSET as u64, 1, 1);

        assert_eq!(ppi.mmio_read(23, 1), 3);
        assert!(ppi.memory_overwrite_requested());
    }

    #[test]
    fn reset_preserves_mailbox_but_clears_accounting() {
        let mut ppi = TpmPpi::new();
        ppi.mmio_write(PPRQ_OFFSET as u64, 4, 23);
        assert_ne!(ppi.stats(), TpmPpiStats::default());

        ppi.reset_runtime_state();

        assert_eq!(ppi.mmio_read(PPRQ_OFFSET as u64, 4), 23);
        assert_eq!(ppi.stats().reads, 1);
        assert_eq!(ppi.stats().writes, 0);
    }

    #[test]
    fn rejects_unsupported_and_cross_boundary_accesses() {
        let mut ppi = TpmPpi::new();
        assert_eq!(ppi.mmio_read(MMIO_SIZE - 1, 2), u64::MAX);
        ppi.mmio_write(0, 3, 0x00ff_ffff);

        assert_eq!(ppi.stats().rejected_accesses, 2);
        assert_eq!(ppi.bytes()[0], 0);
    }
}
