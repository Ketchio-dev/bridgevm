//! Split out of probe_mmio.rs by responsibility.

use crate::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MmioAccessKind {
    Read,
    Write,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MmioAccess {
    pub(crate) ipa: u64,
    pub(crate) kind: MmioAccessKind,
    pub(crate) value: Option<u64>,
    pub(crate) width: u8,
}

impl MmioAccess {
    pub(crate) fn read(ipa: u64, width: u8) -> Self {
        Self {
            ipa,
            kind: MmioAccessKind::Read,
            value: None,
            width,
        }
    }

    pub(crate) fn write(ipa: u64, value: u64, width: u8) -> Self {
        Self {
            ipa,
            kind: MmioAccessKind::Write,
            value: Some(value),
            width,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MmioAction {
    ReadValue(u64),
    WriteAccepted { value: u64, byte: u8 },
    Unhandled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MmioRange {
    pub(crate) start: u64,
    pub(crate) bytes: u64,
}

impl MmioRange {
    pub(crate) fn contains(&self, ipa: u64) -> bool {
        ipa >= self.start && ipa < self.start.saturating_add(self.bytes)
    }
}

pub(crate) trait MmioDevice {
    fn as_any_mut(&mut self) -> &mut dyn Any;
    fn range(&self) -> MmioRange;
    fn handle(&mut self, access: MmioAccess) -> MmioAction;
}

pub(crate) const PL011_UART_MODEL: &str = "PL011 UART skeleton";
pub(crate) const PL011_DR_OFFSET: u64 = 0x00;
pub(crate) const PL011_FR_OFFSET: u64 = 0x18;
pub(crate) const PL011_REGISTER_WINDOW_BYTES: u64 = 0x1000;
pub(crate) const PL031_DR_OFFSET: u64 = 0x00;
pub(crate) const PL031_REGISTER_WINDOW_BYTES: u64 = 0x1000;
pub(crate) const GICD_CTLR_OFFSET: u64 = 0x000;
pub(crate) const GICD_TYPER_OFFSET: u64 = 0x004;
pub(crate) const GICD_IIDR_OFFSET: u64 = 0x008;
pub(crate) const GICD_STATUSR_OFFSET: u64 = 0x010;
pub(crate) const GICD_IGROUPR_BASE_OFFSET: u64 = 0x080;
pub(crate) const GICD_ISENABLER_BASE_OFFSET: u64 = 0x100;
pub(crate) const GICD_ICENABLER_BASE_OFFSET: u64 = 0x180;
pub(crate) const GICD_ISPENDR_BASE_OFFSET: u64 = 0x200;
pub(crate) const GICD_ICPENDR_BASE_OFFSET: u64 = 0x280;
pub(crate) const GICD_ISACTIVER_BASE_OFFSET: u64 = 0x300;
pub(crate) const GICD_ICACTIVER_BASE_OFFSET: u64 = 0x380;
pub(crate) const GICD_IPRIORITYR_BASE_OFFSET: u64 = 0x400;
pub(crate) const GICD_ICFGR_BASE_OFFSET: u64 = 0xc00;
pub(crate) const GICD_IGRPMODR_BASE_OFFSET: u64 = 0xd00;
pub(crate) const GICD_IROUTER_BASE_OFFSET: u64 = 0x6000;
pub(crate) const GICD_CTLR_ENABLE_GRP1NS: u32 = 1 << 1;
pub(crate) const GICR_CTLR_OFFSET: u64 = 0x0000;
pub(crate) const GICR_IIDR_OFFSET: u64 = 0x0004;
pub(crate) const GICR_TYPER_OFFSET: u64 = 0x0008;
pub(crate) const GICR_STATUSR_OFFSET: u64 = 0x0010;
pub(crate) const GICR_WAKER_OFFSET: u64 = 0x0014;
pub(crate) const GICR_PROPBASER_OFFSET: u64 = 0x0070;
pub(crate) const GICR_PENDBASER_OFFSET: u64 = 0x0078;
pub(crate) const GICR_SGI_BASE_OFFSET: u64 = 0x1_0000;
pub(crate) const GICR_SGI_IGROUPR0_OFFSET: u64 = GICR_SGI_BASE_OFFSET + 0x080;
pub(crate) const GICR_SGI_ISENABLER0_OFFSET: u64 = GICR_SGI_BASE_OFFSET + 0x100;
pub(crate) const GICR_SGI_ICENABLER0_OFFSET: u64 = GICR_SGI_BASE_OFFSET + 0x180;
pub(crate) const GICR_SGI_ISPENDR0_OFFSET: u64 = GICR_SGI_BASE_OFFSET + 0x200;
pub(crate) const GICR_SGI_ICPENDR0_OFFSET: u64 = GICR_SGI_BASE_OFFSET + 0x280;
pub(crate) const GICR_SGI_ISACTIVER0_OFFSET: u64 = GICR_SGI_BASE_OFFSET + 0x300;
pub(crate) const GICR_SGI_ICACTIVER0_OFFSET: u64 = GICR_SGI_BASE_OFFSET + 0x380;
pub(crate) const GICR_SGI_IPRIORITYR_BASE_OFFSET: u64 = GICR_SGI_BASE_OFFSET + 0x400;
pub(crate) const GICR_SGI_ICFGR0_OFFSET: u64 = GICR_SGI_BASE_OFFSET + 0xc00;
pub(crate) const GICR_SGI_IGRPMODR0_OFFSET: u64 = GICR_SGI_BASE_OFFSET + 0xd00;
pub(crate) const GICV3_SUPPORTED_INTERRUPT_COUNT: usize = 64;
pub(crate) const GICV3_INTERRUPT_REGISTER_COUNT: usize = GICV3_SUPPORTED_INTERRUPT_COUNT / 32;
pub(crate) const GICV3_PRIORITY_REGISTER_COUNT: usize = GICV3_SUPPORTED_INTERRUPT_COUNT / 4;
pub(crate) const GICV3_CONFIG_REGISTER_COUNT: usize = GICV3_SUPPORTED_INTERRUPT_COUNT / 16;
pub(crate) const GICV3_IIDR_VALUE: u64 = 0x4252_564d;
pub(crate) const GICD_TYPER_VALUE: u64 = 1 | (5 << 19);
pub(crate) const GICR_TYPER_VALUE: u64 = 1 << 4;
pub(crate) const GICV3_DEFAULT_PRIORITY_WORD: u32 = 0xa0a0_a0a0;
pub(crate) const GICR_WAKER_PROCESSOR_SLEEP: u64 = 1 << 1;
pub(crate) const GICR_WAKER_CHILDREN_ASLEEP: u64 = 1 << 2;
pub(crate) const WINDOWS_ARM_VIRTUAL_TIMER_PPI: u32 = 11;
pub(crate) const WINDOWS_ARM_VIRTUAL_TIMER_INTERRUPT_ID: u32 = 16 + WINDOWS_ARM_VIRTUAL_TIMER_PPI;
pub(crate) const AARCH64_SYSREG_TRAP_EXCEPTION_CLASS: u64 = 0x18;
pub(crate) const ICC_PMR_EL1_SYSREG: u16 = 0xc230;
pub(crate) const ICC_IAR0_EL1_SYSREG: u16 = 0xc640;
pub(crate) const ICC_EOIR0_EL1_SYSREG: u16 = 0xc641;
pub(crate) const ICC_HPPIR0_EL1_SYSREG: u16 = 0xc642;
pub(crate) const ICC_BPR0_EL1_SYSREG: u16 = 0xc643;
pub(crate) const ICC_AP0R0_EL1_SYSREG: u16 = 0xc644;
pub(crate) const ICC_AP0R1_EL1_SYSREG: u16 = 0xc645;
pub(crate) const ICC_AP0R2_EL1_SYSREG: u16 = 0xc646;
pub(crate) const ICC_AP0R3_EL1_SYSREG: u16 = 0xc647;
pub(crate) const ICC_AP1R0_EL1_SYSREG: u16 = 0xc648;
pub(crate) const ICC_AP1R1_EL1_SYSREG: u16 = 0xc649;
pub(crate) const ICC_AP1R2_EL1_SYSREG: u16 = 0xc64a;
pub(crate) const ICC_AP1R3_EL1_SYSREG: u16 = 0xc64b;
pub(crate) const ICC_DIR_EL1_SYSREG: u16 = 0xc659;
pub(crate) const ICC_RPR_EL1_SYSREG: u16 = 0xc65b;
pub(crate) const ICC_SGI1R_EL1_SYSREG: u16 = 0xc65d;
pub(crate) const ICC_IAR1_EL1_SYSREG: u16 = 0xc660;
pub(crate) const ICC_EOIR1_EL1_SYSREG: u16 = 0xc661;
pub(crate) const ICC_HPPIR1_EL1_SYSREG: u16 = 0xc662;
pub(crate) const ICC_BPR1_EL1_SYSREG: u16 = 0xc663;
pub(crate) const ICC_CTLR_EL1_SYSREG: u16 = 0xc664;
pub(crate) const ICC_CTLR_EL1_EOIMODE: u64 = 1 << 1;
pub(crate) const ICC_SRE_EL1_SYSREG: u16 = 0xc665;
pub(crate) const ICC_IGRPEN0_EL1_SYSREG: u16 = 0xc666;
pub(crate) const ICC_IGRPEN1_EL1_SYSREG: u16 = 0xc667;
pub(crate) const GICV3_SPURIOUS_INTERRUPT_ID: u32 = 1023;

#[cfg(test)]
#[path = "bus_tests/mod.rs"]
mod tests;
