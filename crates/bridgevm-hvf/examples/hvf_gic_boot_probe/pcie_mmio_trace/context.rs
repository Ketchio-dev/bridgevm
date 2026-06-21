use bridgevm_hvf::fwcfg::GuestMemoryMut;
use bridgevm_hvf::machine;
use bridgevm_hvf::pcie;
use bridgevm_hvf::platform_virt::{MmioOp, MmioOutcome, VirtPlatform};

use super::PcieTraceTarget;

const OBSERVED_XHCI_BAR0_LOW_BASE: u64 = 0x3efe_8000;
const OBSERVED_XHCI_BAR0_LOW_SIZE: u64 = 0x4000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PcieBarReadbacks {
    pub(super) bar0: u32,
    pub(super) bar1: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PcieConfigSnapshot {
    bdf: (u8, u8, u8),
    command_status: u32,
    bars: PcieBarReadbacks,
    msix_control: Option<u16>,
}

impl PcieConfigSnapshot {
    pub(crate) const fn xhci(
        command_status: u32,
        bars: PcieBarReadbacks,
        msix_control: Option<u16>,
    ) -> Self {
        Self {
            bdf: pcie::XHCI_BDF,
            command_status,
            bars,
            msix_control,
        }
    }

    pub(super) fn bar0_mmio64_contains(self, ipa: u64) -> bool {
        let base = (u64::from(self.bars.bar1) << 32) | u64::from(self.bars.bar0 & !0xf);
        base != 0
            && ipa
                .checked_sub(base)
                .is_some_and(|offset| offset < u64::from(pcie::XHCI_BAR0_SIZE))
    }

    fn describe_for_ipa(self, ipa: u64) -> String {
        let (bus, device, function) = self.bdf;
        let command = (self.command_status & u32::from(u16::MAX)) as u16;
        let status = self.command_status >> 16;
        let memory = command & pcie::CMD_MEMORY_SPACE != 0;
        let bus_master = command & pcie::CMD_BUS_MASTER != 0;
        let base = (u64::from(self.bars.bar1) << 32) | u64::from(self.bars.bar0 & !0xf);
        let contains_ipa = self.bar0_mmio64_contains(ipa);
        match self.msix_control {
            Some(msix_control) => format!(
                "xhci={bus:02x}:{device:02x}.{function} command={command:#06x} memory={memory} bus_master={bus_master} status={status:#06x} bar0={:#010x} bar1={:#010x} base={base:#x} contains_ipa={contains_ipa} msix_ctrl={msix_control:#06x}",
                self.bars.bar0, self.bars.bar1
            ),
            None => format!(
                "xhci={bus:02x}:{device:02x}.{function} command={command:#06x} memory={memory} bus_master={bus_master} status={status:#06x} bar0={:#010x} bar1={:#010x} base={base:#x} contains_ipa={contains_ipa}",
                self.bars.bar0, self.bars.bar1
            ),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PcieTraceContext {
    PciConfig(PcieConfigSnapshot),
}

impl PcieTraceContext {
    pub(super) fn describe_for_ipa(self, ipa: u64) -> String {
        match self {
            Self::PciConfig(snapshot) => snapshot.describe_for_ipa(ipa),
        }
    }
}

pub(crate) fn targetless_xhci_trace_context(
    platform: &mut VirtPlatform,
    mem: &mut dyn GuestMemoryMut,
    device: &'static str,
    ipa: u64,
    target: Option<PcieTraceTarget>,
    outcome: &MmioOutcome,
) -> Option<PcieTraceContext> {
    if device != "pcie-mmio-32" || target.is_some() {
        return None;
    }
    if !matches!(outcome, MmioOutcome::KnownUnimplemented(name) if *name == "pcie-mmio-32") {
        return None;
    }
    let observed_xhci_range =
        OBSERVED_XHCI_BAR0_LOW_BASE..OBSERVED_XHCI_BAR0_LOW_BASE + OBSERVED_XHCI_BAR0_LOW_SIZE;
    let snapshot = xhci_config_snapshot(platform, mem)?;
    if !observed_xhci_range.contains(&ipa) && !snapshot.bar0_mmio64_contains(ipa) {
        return None;
    }
    Some(PcieTraceContext::PciConfig(snapshot))
}

fn xhci_config_snapshot(
    platform: &mut VirtPlatform,
    mem: &mut dyn GuestMemoryMut,
) -> Option<PcieConfigSnapshot> {
    let command_status = u32::try_from(pcie_cfg_read(
        platform,
        mem,
        pcie::XHCI_BDF,
        pcie::REG_COMMAND_STATUS,
        4,
    )?)
    .ok()?;
    let bar0 = u32::try_from(pcie_cfg_read(
        platform,
        mem,
        pcie::XHCI_BDF,
        pcie::REG_BAR0,
        4,
    )?)
    .ok()?;
    let bar1 = u32::try_from(pcie_cfg_read(
        platform,
        mem,
        pcie::XHCI_BDF,
        pcie::REG_BAR0 + 4,
        4,
    )?)
    .ok()?;
    let msix_control = pcie_cfg_read(
        platform,
        mem,
        pcie::XHCI_BDF,
        u16::from(pcie::XHCI_MSIX_CAP_OFFSET) + 2,
        2,
    )
    .and_then(|value| u16::try_from(value).ok());
    Some(PcieConfigSnapshot::xhci(
        command_status,
        PcieBarReadbacks { bar0, bar1 },
        msix_control,
    ))
}

fn pcie_cfg_read(
    platform: &mut VirtPlatform,
    mem: &mut dyn GuestMemoryMut,
    bdf: (u8, u8, u8),
    reg: u16,
    size: u8,
) -> Option<u64> {
    match platform.on_mmio(pcie_ecam_gpa(bdf, reg), MmioOp::Read { size }, mem) {
        MmioOutcome::ReadValue(value) => Some(value),
        MmioOutcome::WriteAck | MmioOutcome::KnownUnimplemented(_) | MmioOutcome::Unmapped => None,
    }
}

fn pcie_ecam_gpa(bdf: (u8, u8, u8), reg: u16) -> u64 {
    let (bus, device, function) = bdf;
    machine::PCIE_ECAM.base
        + u64::from(bus)
            * u64::from(pcie::DEVICES_PER_BUS)
            * u64::from(pcie::FUNCS_PER_DEVICE)
            * pcie::CFG_SPACE_SIZE
        + u64::from(device) * u64::from(pcie::FUNCS_PER_DEVICE) * pcie::CFG_SPACE_SIZE
        + u64::from(function) * pcie::CFG_SPACE_SIZE
        + u64::from(reg)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bridgevm_hvf::dtb::VirtFdtConfig;

    struct NullGuestMemory;

    impl GuestMemoryMut for NullGuestMemory {
        fn write_bytes(&mut self, _gpa: u64, _data: &[u8]) -> bool {
            false
        }

        fn read_bytes(&self, _gpa: u64, _len: usize) -> Option<Vec<u8>> {
            None
        }
    }

    #[test]
    fn targetless_observed_xhci_mmio_captures_config_snapshot() {
        let mut platform = VirtPlatform::new(VirtFdtConfig {
            cpu_count: 1,
            ram_size: 512 * 1024 * 1024,
        });
        let mut mem = NullGuestMemory;

        let context = targetless_xhci_trace_context(
            &mut platform,
            &mut mem,
            "pcie-mmio-32",
            OBSERVED_XHCI_BAR0_LOW_BASE + 0x440,
            None,
            &MmioOutcome::KnownUnimplemented("pcie-mmio-32"),
        );

        assert!(matches!(context, Some(PcieTraceContext::PciConfig(_))));
    }

    #[test]
    fn non_xhci_targetless_mmio_does_not_get_xhci_snapshot() {
        let mut platform = VirtPlatform::new(VirtFdtConfig {
            cpu_count: 1,
            ram_size: 512 * 1024 * 1024,
        });
        let mut mem = NullGuestMemory;

        let context = targetless_xhci_trace_context(
            &mut platform,
            &mut mem,
            "pcie-mmio-32",
            machine::PCIE_MMIO_32.base,
            None,
            &MmioOutcome::KnownUnimplemented("pcie-mmio-32"),
        );

        assert_eq!(context, None);
    }

    #[test]
    fn bar0_mmio64_contains_matches_normal_base() {
        let snapshot = PcieConfigSnapshot::xhci(
            0,
            PcieBarReadbacks {
                bar0: 0x0001_0000,
                bar1: 0,
            },
            None,
        );

        assert!(snapshot.bar0_mmio64_contains(0x0001_0100));
        assert!(!snapshot.bar0_mmio64_contains(0x0001_0000 + u64::from(pcie::XHCI_BAR0_SIZE)));
    }

    #[test]
    fn bar0_mmio64_contains_rejects_overflowing_bar_readbacks() {
        for bars in [
            PcieBarReadbacks {
                bar0: u32::MAX,
                bar1: u32::MAX,
            },
            PcieBarReadbacks {
                bar0: 0xffff_f000,
                bar1: u32::MAX,
            },
        ] {
            let snapshot = PcieConfigSnapshot::xhci(0, bars, None);

            assert!(!snapshot.bar0_mmio64_contains(0x1000));
        }
    }
}
