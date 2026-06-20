//! AArch64 EL1 stage-1 translation helpers for live guest diagnostics.
//!
//! The Windows HVF live probe often stops in high virtual-address kernel code
//! after the firmware has handed off. Host-side PE/image scanning needs the
//! corresponding guest-physical address, so this module implements the small
//! 4 KiB-granule page-table walker needed for watchdog dumps.

use crate::fwcfg::GuestMemoryMut;

#[derive(Debug, Clone, Copy)]
pub struct Stage1Context {
    pub sctlr_el1: u64,
    pub tcr_el1: u64,
    pub ttbr0_el1: u64,
    pub ttbr1_el1: u64,
    pub mair_el1: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage1Root {
    Ttbr0,
    Ttbr1,
}

impl Stage1Root {
    pub fn label(self) -> &'static str {
        match self {
            Self::Ttbr0 => "TTBR0_EL1",
            Self::Ttbr1 => "TTBR1_EL1",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stage1WalkStep {
    pub level: u8,
    pub table_ipa: u64,
    pub index: u64,
    pub entry_ipa: u64,
    pub descriptor: Option<u64>,
    pub kind: &'static str,
    pub next_table_ipa: Option<u64>,
    pub output_ipa: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stage1Translation {
    pub root: Stage1Root,
    pub va_bits: u8,
    pub start_level: u8,
    pub ipa: u64,
    pub leaf_va_base: u64,
    pub leaf_ipa_base: u64,
    pub leaf_level: u8,
    pub leaf_descriptor: u64,
    pub leaf_kind: &'static str,
    pub attr_index: u8,
    pub access_permissions: u8,
    pub shareability: u8,
    pub access_flag: bool,
    pub pxn: bool,
    pub uxn: bool,
    pub steps: Vec<Stage1WalkStep>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stage1WalkFailure {
    pub reason: String,
    pub steps: Vec<Stage1WalkStep>,
}

pub fn descriptor_kind(descriptor: u64, level: u8) -> &'static str {
    match (descriptor & 0x3, level) {
        (0, _) => "invalid",
        (1, 0..=2) => "block",
        (1, _) => "reserved",
        (3, 0..=2) => "table",
        (3, _) => "page",
        _ => "reserved",
    }
}

pub fn descriptor_span_bytes(level: u8, kind: &'static str) -> Option<u64> {
    let shift = match (kind, level) {
        ("block", 0) => 39,
        ("block", 1) => 30,
        ("block", 2) => 21,
        ("page", 3) => 12,
        _ => return None,
    };
    Some(1u64 << shift)
}

pub fn descriptor_output_address(descriptor: u64, level: u8, kind: &'static str) -> Option<u64> {
    let span = descriptor_span_bytes(level, kind)?;
    Some(descriptor & 0x0000_ffff_ffff_f000 & !(span - 1))
}

pub fn start_level(va_bits: u8) -> Option<u8> {
    match va_bits {
        40..=64 => Some(0),
        31..=39 => Some(1),
        22..=30 => Some(2),
        12..=21 => Some(3),
        _ => None,
    }
}

pub fn root_for_va(ctx: &Stage1Context, va: u64) -> Result<(Stage1Root, u8, u8, u64), String> {
    let t0sz = (ctx.tcr_el1 & 0x3f) as u8;
    let t1sz = ((ctx.tcr_el1 >> 16) & 0x3f) as u8;
    let ttbr0_bits = 64u8
        .checked_sub(t0sz)
        .ok_or_else(|| format!("invalid T0SZ {t0sz}"))?;
    let ttbr1_bits = 64u8
        .checked_sub(t1sz)
        .ok_or_else(|| format!("invalid T1SZ {t1sz}"))?;
    let ttbr0_match = if ttbr0_bits == 64 {
        true
    } else {
        va < (1u64 << ttbr0_bits)
    };
    let ttbr1_match = if ttbr1_bits == 64 {
        true
    } else {
        va >= (u64::MAX << ttbr1_bits)
    };

    let (root, va_bits, ttbr, tg_ok, tg_label) = if ttbr1_match && (!ttbr0_match || (va >> 63) != 0)
    {
        (
            Stage1Root::Ttbr1,
            ttbr1_bits,
            ctx.ttbr1_el1,
            ((ctx.tcr_el1 >> 30) & 0x3) == 0b10,
            "TG1",
        )
    } else if ttbr0_match {
        (
            Stage1Root::Ttbr0,
            ttbr0_bits,
            ctx.ttbr0_el1,
            ((ctx.tcr_el1 >> 14) & 0x3) == 0b00,
            "TG0",
        )
    } else if ttbr1_match {
        (
            Stage1Root::Ttbr1,
            ttbr1_bits,
            ctx.ttbr1_el1,
            ((ctx.tcr_el1 >> 30) & 0x3) == 0b10,
            "TG1",
        )
    } else {
        return Err(format!(
            "VA is outside TTBR0/TTBR1 ranges (T0SZ={t0sz}, T1SZ={t1sz})"
        ));
    };

    if !tg_ok {
        return Err(format!(
            "{} is not 4 KiB granule in TCR_EL1={:#x}",
            tg_label, ctx.tcr_el1
        ));
    }
    let start_level =
        start_level(va_bits).ok_or_else(|| format!("unsupported VA size {va_bits}"))?;
    Ok((root, va_bits, start_level, ttbr))
}

pub fn translate(
    mem: &dyn GuestMemoryMut,
    ctx: &Stage1Context,
    va: u64,
) -> Result<Stage1Translation, Stage1WalkFailure> {
    if ctx.sctlr_el1 & 1 == 0 {
        return Err(Stage1WalkFailure {
            reason: "SCTLR_EL1.M is clear".to_string(),
            steps: Vec::new(),
        });
    }
    let (root, va_bits, start_level, ttbr) = match root_for_va(ctx, va) {
        Ok(root) => root,
        Err(reason) => {
            return Err(Stage1WalkFailure {
                reason,
                steps: Vec::new(),
            });
        }
    };
    let mut table_ipa = ttbr & 0x0000_ffff_ffff_f000;
    let mut steps = Vec::new();
    for level in start_level..=3 {
        let shift = 39u32.saturating_sub(u32::from(level) * 9);
        let index = (va >> shift) & 0x1ff;
        let Some(entry_ipa) = table_ipa.checked_add(index.saturating_mul(8)) else {
            return Err(Stage1WalkFailure {
                reason: format!("L{level} entry IPA overflow"),
                steps,
            });
        };
        let descriptor = read_descriptor(mem, entry_ipa);
        let kind = descriptor
            .map(|d| descriptor_kind(d, level))
            .unwrap_or("not-readable");
        let next_table_ipa = descriptor
            .filter(|_| kind == "table")
            .map(|d| d & 0x0000_ffff_ffff_f000);
        let output_ipa = descriptor.and_then(|d| descriptor_output_address(d, level, kind));
        steps.push(Stage1WalkStep {
            level,
            table_ipa,
            index,
            entry_ipa,
            descriptor,
            kind,
            next_table_ipa,
            output_ipa,
        });
        let Some(descriptor) = descriptor else {
            return Err(Stage1WalkFailure {
                reason: format!("L{level} descriptor at {entry_ipa:#x} is not in RAM view"),
                steps,
            });
        };
        if let Some(next_table_ipa) = next_table_ipa {
            table_ipa = next_table_ipa;
            continue;
        }
        let Some(span) = descriptor_span_bytes(level, kind) else {
            return Err(Stage1WalkFailure {
                reason: format!("L{level} descriptor kind {kind} is not translatable"),
                steps,
            });
        };
        let Some(leaf_ipa_base) = output_ipa else {
            return Err(Stage1WalkFailure {
                reason: format!("L{level} descriptor has no output address"),
                steps,
            });
        };
        let leaf_va_base = va & !(span - 1);
        let Some(ipa) = leaf_ipa_base.checked_add(va - leaf_va_base) else {
            return Err(Stage1WalkFailure {
                reason: "translated IPA overflow".to_string(),
                steps,
            });
        };
        return Ok(Stage1Translation {
            root,
            va_bits,
            start_level,
            ipa,
            leaf_va_base,
            leaf_ipa_base,
            leaf_level: level,
            leaf_descriptor: descriptor,
            leaf_kind: kind,
            attr_index: ((descriptor >> 2) & 0x7) as u8,
            access_permissions: ((descriptor >> 6) & 0x3) as u8,
            shareability: ((descriptor >> 8) & 0x3) as u8,
            access_flag: descriptor & (1 << 10) != 0,
            pxn: descriptor & (1 << 53) != 0,
            uxn: descriptor & (1 << 54) != 0,
            steps,
        });
    }
    Err(Stage1WalkFailure {
        reason: "walk exhausted without leaf".to_string(),
        steps,
    })
}

fn read_descriptor(mem: &dyn GuestMemoryMut, ipa: u64) -> Option<u64> {
    let bytes = mem.read_bytes(ipa, 8)?;
    Some(u64::from_le_bytes(bytes.try_into().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    const AF: u64 = 1 << 10;
    const TABLE: u64 = 0b11;
    const BLOCK: u64 = 0b01;
    const PAGE: u64 = 0b11;

    struct TestMem {
        base: u64,
        bytes: Vec<u8>,
    }

    impl TestMem {
        fn new(base: u64, len: usize) -> Self {
            Self {
                base,
                bytes: vec![0; len],
            }
        }

        fn write_u64(&mut self, gpa: u64, value: u64) {
            assert!(self.write_bytes(gpa, &value.to_le_bytes()));
        }
    }

    impl GuestMemoryMut for TestMem {
        fn write_bytes(&mut self, gpa: u64, data: &[u8]) -> bool {
            let Some(off) = gpa.checked_sub(self.base).map(|o| o as usize) else {
                return false;
            };
            let Some(end) = off.checked_add(data.len()) else {
                return false;
            };
            if end > self.bytes.len() {
                return false;
            }
            self.bytes[off..end].copy_from_slice(data);
            true
        }

        fn read_bytes(&self, gpa: u64, len: usize) -> Option<Vec<u8>> {
            let off = gpa.checked_sub(self.base)? as usize;
            let end = off.checked_add(len)?;
            if end > self.bytes.len() {
                return None;
            }
            Some(self.bytes[off..end].to_vec())
        }
    }

    fn table_index(va: u64, level: u8) -> u64 {
        let shift = 39u32.saturating_sub(u32::from(level) * 9);
        (va >> shift) & 0x1ff
    }

    fn entry_ipa(table: u64, va: u64, level: u8) -> u64 {
        table + table_index(va, level) * 8
    }

    #[test]
    fn ttbr0_walk_translates_l3_page() {
        let va = 0x1234_5678;
        let l0 = 0x1000;
        let l1 = 0x2000;
        let l2 = 0x3000;
        let l3 = 0x4000;
        let leaf_ipa = 0x8000_0000;
        let mut mem = TestMem::new(0, 0x9000);
        mem.write_u64(entry_ipa(l0, va, 0), l1 | TABLE);
        mem.write_u64(entry_ipa(l1, va, 1), l2 | TABLE);
        mem.write_u64(entry_ipa(l2, va, 2), l3 | TABLE);
        mem.write_u64(entry_ipa(l3, va, 3), leaf_ipa | AF | PAGE);

        let ctx = Stage1Context {
            sctlr_el1: 1,
            tcr_el1: 24,
            ttbr0_el1: l0,
            ttbr1_el1: 0,
            mair_el1: 0,
        };
        let got = translate(&mem, &ctx, va).unwrap();

        assert_eq!(got.root, Stage1Root::Ttbr0);
        assert_eq!(got.start_level, 0);
        assert_eq!(got.leaf_level, 3);
        assert_eq!(got.leaf_kind, "page");
        assert_eq!(got.ipa, leaf_ipa + 0x678);
        assert_eq!(got.steps.len(), 4);
    }

    #[test]
    fn ttbr1_walk_translates_windows_high_va_l2_block() {
        let va = 0xffff_f801_4508_1cdc;
        let l0 = 0x5000;
        let l1 = 0x6000;
        let l2 = 0x7000;
        let leaf_ipa = 0x1_0040_0000;
        let mut mem = TestMem::new(0, 0x9000);
        mem.write_u64(entry_ipa(l0, va, 0), l1 | TABLE);
        mem.write_u64(entry_ipa(l1, va, 1), l2 | TABLE);
        mem.write_u64(entry_ipa(l2, va, 2), leaf_ipa | AF | BLOCK);

        let ctx = Stage1Context {
            sctlr_el1: 1,
            tcr_el1: (17 << 16) | (0b10 << 30),
            ttbr0_el1: 0,
            ttbr1_el1: l0,
            mair_el1: 0,
        };
        let got = translate(&mem, &ctx, va).unwrap();

        assert_eq!(got.root, Stage1Root::Ttbr1);
        assert_eq!(got.va_bits, 47);
        assert_eq!(got.leaf_level, 2);
        assert_eq!(got.leaf_kind, "block");
        assert_eq!(got.leaf_va_base, 0xffff_f801_4500_0000);
        assert_eq!(got.ipa, 0x1_0048_1cdc);
    }

    #[test]
    fn invalid_descriptor_reports_walk_failure_with_steps() {
        let mem = TestMem::new(0, 0x2000);
        let ctx = Stage1Context {
            sctlr_el1: 1,
            tcr_el1: 24,
            ttbr0_el1: 0x1000,
            ttbr1_el1: 0,
            mair_el1: 0,
        };
        let err = translate(&mem, &ctx, 0x1234).unwrap_err();

        assert!(err.reason.contains("descriptor kind invalid"));
        assert_eq!(err.steps.len(), 1);
        assert_eq!(err.steps[0].kind, "invalid");
    }

    #[test]
    fn rejects_mmu_disabled_context() {
        let mem = TestMem::new(0, 0x2000);
        let ctx = Stage1Context {
            sctlr_el1: 0,
            tcr_el1: 24,
            ttbr0_el1: 0x1000,
            ttbr1_el1: 0,
            mair_el1: 0,
        };
        let err = translate(&mem, &ctx, 0x1234).unwrap_err();

        assert_eq!(err.reason, "SCTLR_EL1.M is clear");
    }

    #[test]
    fn rejects_non_4k_granule_context() {
        let mem = TestMem::new(0, 0x2000);
        let ctx = Stage1Context {
            sctlr_el1: 1,
            tcr_el1: 24 | (0b01 << 14),
            ttbr0_el1: 0x1000,
            ttbr1_el1: 0,
            mair_el1: 0,
        };
        let err = translate(&mem, &ctx, 0x1234).unwrap_err();

        assert!(err.reason.contains("TG0 is not 4 KiB granule"));
    }
}
