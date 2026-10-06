//! CORB memory-error masking through guest PCI configuration and BAR accesses.

use super::super::*;
use super::helpers::{pcie_cfg_gpa, platform_with_devices};
use crate::fwcfg::GuestMemoryMut;
use crate::hda::*;
use crate::machine;
use crate::msix::MsixMessage;
use crate::pcie;

const BAR: u64 = machine::PCIE_MMIO_32.base + 0x70_000;
const CORB: u64 = machine::RAM_BASE + 0x1000;
const RIRB: u64 = machine::RAM_BASE + 0x2000;
const UNBACKED_CORB: u64 = machine::RAM_BASE + 0x1_0000;
const MESSAGE_ADDRESS: u64 = 0x0000_0001_0808_4000;
const MESSAGE_DATA: u32 = 0x61;
const CMEIE: u64 = 1;
const RUN: u64 = 2;
const CIE: u64 = 1 << 30;
const GIE: u64 = 1 << 31;

struct Fixture {
    platform: VirtPlatform,
    mem: FlatGuestRam,
}

impl Fixture {
    fn new() -> Self {
        let mut fixture = Self {
            platform: platform_with_devices(VirtPlatformDeviceConfig {
                hda_present: true,
                ..VirtPlatformDeviceConfig::default()
            }),
            mem: FlatGuestRam::new(machine::RAM_BASE, 0x1_0000),
        };
        fixture.platform.set_hda_pcm_sink(None);
        let msi = u16::from(pcie::HDA_MSI_CAP_OFFSET);
        for (reg, size, value) in [
            (pcie::REG_BAR0, 4, BAR),
            (
                pcie::REG_COMMAND_STATUS,
                2,
                u64::from(pcie::CMD_MEMORY_SPACE | pcie::CMD_BUS_MASTER),
            ),
            (msi + 4, 4, MESSAGE_ADDRESS as u32 as u64),
            (msi + 8, 4, MESSAGE_ADDRESS >> 32),
            (msi + 12, 2, u64::from(MESSAGE_DATA)),
            (msi + 2, 2, 1),
        ] {
            fixture.write_at(
                pcie_cfg_gpa(pcie::HDA_BDF.1, pcie::HDA_BDF.2, reg),
                size,
                value,
            );
        }
        assert!(fixture.mem.write_bytes(RIRB, &[0xa5; 256 * 8]));
        fixture
    }

    fn write_at(&mut self, address: u64, size: u8, value: u64) {
        assert_eq!(
            self.platform
                .on_mmio(address, MmioOp::Write { size, value }, &mut self.mem),
            MmioOutcome::WriteAck
        );
    }

    fn write(&mut self, register: u64, size: u8, value: u64) {
        self.write_at(BAR + register, size, value);
    }

    fn read(&mut self, register: u64, size: u8) -> u64 {
        let MmioOutcome::ReadValue(value) =
            self.platform
                .on_mmio(BAR + register, MmioOp::Read { size }, &mut self.mem)
        else {
            panic!("HDA BAR read must be implemented");
        };
        value
    }

    fn start(&mut self, corb: u64, cmeie: u64, intctl: u64) {
        self.write(REG_GCTL, 4, 1);
        self.write(REG_CORBCTL, 1, 0);
        assert_eq!(self.read(REG_CORBCTL, 1), 0);
        self.write(REG_CORBLBASE, 4, corb as u32 as u64);
        self.write(REG_CORBUBASE, 4, corb >> 32);
        self.write(REG_CORBRP, 2, 0x8000);
        assert_eq!(self.read(REG_CORBRP, 2), 0x8000);
        self.write(REG_CORBRP, 2, 0);
        assert_eq!(self.read(REG_CORBRP, 2), 0);
        self.write(REG_CORBWP, 2, 0);
        self.write(REG_RIRBLBASE, 4, RIRB as u32 as u64);
        self.write(REG_RIRBUBASE, 4, RIRB >> 32);
        self.write(REG_RIRBWP, 2, 0x8000);
        self.write(REG_RIRBCTL, 1, 2);
        self.write(REG_INTCTL, 4, intctl);
        self.write(REG_CORBCTL, 1, RUN | cmeie);
        assert_eq!(self.read(REG_CORBCTL, 1), RUN | cmeie);
        self.write(REG_CORBWP, 2, 1);
    }

    fn fault(&mut self, cmeie: u64, intctl: u64) {
        self.start(UNBACKED_CORB, cmeie, intctl);
        assert_eq!(self.read(REG_CORBSTS, 1), 1);
        assert_eq!(self.read(REG_CORBRP, 2), 0);
        assert_eq!(self.read(REG_RIRBWP, 2), 0);
        assert_eq!(self.read(REG_RIRBSTS, 1), 0);
        assert_eq!(self.mem.read_bytes(RIRB, 256 * 8).unwrap(), [0xa5; 256 * 8]);
    }

    fn level(&self) -> bool {
        self.platform.hda.as_ref().unwrap().interrupt_level()
    }

    fn messages(&mut self) -> Vec<MsixMessage> {
        assert!(self.platform.take_pending_spi_levels().is_empty());
        self.platform.take_pending_msix()
    }

    fn expect_message(&mut self) {
        assert_eq!(
            self.messages(),
            vec![MsixMessage {
                vector: 0,
                address: MESSAGE_ADDRESS,
                data: MESSAGE_DATA,
            }]
        );
    }
}

#[test]
fn masked_corb_fault_latches_status_without_interrupt_or_msi() {
    let mut f = Fixture::new();
    f.fault(0, GIE | CIE);
    assert!(!f.level());
    assert!(f.messages().is_empty());
    assert_eq!(f.read(REG_CORBSTS, 1), 1);
}

#[test]
fn enabled_corb_fault_delivers_one_programmed_msi() {
    let mut f = Fixture::new();
    f.fault(CMEIE, GIE | CIE);
    assert!(f.level());
    f.expect_message();
    assert_eq!(f.read(REG_CORBSTS, 1), 1);
    assert!(f.messages().is_empty());
    f.write(REG_CORBCTL, 1, CMEIE);
    assert!(f.level());
    assert!(f.messages().is_empty());
}

#[test]
fn latched_corb_error_follows_cmeie_without_clearing_status() {
    let mut f = Fixture::new();
    f.fault(0, GIE | CIE);
    assert!(f.messages().is_empty());
    // Stop DMA before changing masks; a fault requires CRST before resuming.
    f.write(REG_CORBCTL, 1, 0);
    f.write(REG_CORBCTL, 1, CMEIE);
    assert!(f.level());
    f.expect_message();
    f.write(REG_CORBCTL, 1, 0);
    assert!(!f.level());
    assert_eq!(f.read(REG_CORBSTS, 1), 1);
    assert!(f.messages().is_empty());
    f.write(REG_CORBCTL, 1, CMEIE);
    assert!(f.level());
    f.expect_message();
}

#[test]
fn corb_error_respects_controller_and_global_interrupt_masks() {
    for intctl in [0, CIE, GIE] {
        let mut f = Fixture::new();
        f.fault(CMEIE, intctl);
        assert!(!f.level());
        assert!(f.messages().is_empty());
        f.write(REG_CORBCTL, 1, CMEIE);
        f.write(REG_INTCTL, 4, GIE | CIE);
        assert!(f.level());
        f.expect_message();
        f.write(REG_INTCTL, 4, intctl);
        assert!(!f.level());
        assert_eq!(f.read(REG_CORBSTS, 1), 1);
        assert!(f.messages().is_empty());
    }
}

#[test]
fn stopped_corb_error_status_is_write_one_to_clear() {
    let mut f = Fixture::new();
    f.fault(CMEIE, GIE | CIE);
    f.expect_message();
    f.write(REG_CORBCTL, 1, CMEIE);
    f.write(REG_CORBSTS, 1, 0);
    assert_eq!(f.read(REG_CORBSTS, 1), 1);
    assert!(f.level());
    assert!(f.messages().is_empty());
    f.write(REG_CORBSTS, 1, 1);
    assert_eq!(f.read(REG_CORBSTS, 1), 0);
    assert!(!f.level());
    assert!(f.messages().is_empty());
}

#[test]
fn controller_reset_clears_corb_error_before_command_recovery() {
    let mut f = Fixture::new();
    f.fault(CMEIE, GIE | CIE);
    f.expect_message();
    f.write(REG_CORBCTL, 1, 0);
    f.write(REG_GCTL, 4, 0);
    assert_eq!(f.read(REG_GCTL, 4), 0);
    assert_eq!(f.read(REG_CORBSTS, 1), 0);
    assert_eq!(f.read(REG_CORBCTL, 1), 0);
    assert!(!f.level());
    assert!(f.messages().is_empty());
    assert!(f.mem.write_bytes(CORB + 4, &0x000f_0000u32.to_le_bytes()));
    f.start(CORB, CMEIE, GIE | CIE);
    assert_eq!(f.read(REG_CORBSTS, 1), 0);
    assert_eq!(f.read(REG_CORBRP, 2), 1);
    assert_eq!(f.read(REG_RIRBWP, 2), 1);
    assert_eq!(
        f.mem.read_bytes(RIRB + 8, 8).unwrap(),
        [0x22, 0, 0xf4, 0x1a, 0, 0, 0, 0]
    );
    assert!(!f.level());
    assert!(f.messages().is_empty());
}
