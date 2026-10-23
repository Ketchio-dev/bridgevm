//! The host service tick's platform entry polls HDA at its own host time and
//! takes only HDA's MSI; the activity handle survives platform resets.

use super::super::*;
use super::helpers::{pcie_cfg_gpa, platform_with_devices};
use crate::fwcfg::GuestMemoryMut;
use crate::hda::*;
use crate::machine;
use crate::msix::MsixMessage;
use crate::pcie;
use std::sync::Arc;
use std::time::{Duration, Instant};

const BAR: u64 = machine::PCIE_MMIO_32.base + 0x70_000;
const BDL: u64 = machine::RAM_BASE + 0x1000;
const PCM: u64 = machine::RAM_BASE + 0x2000;
const ADDRESS: u64 = 0x0000_0001_0808_4000;
const DATA: u32 = 0x61;
const HDA_MESSAGE: MsixMessage = MsixMessage {
    vector: 0,
    address: ADDRESS,
    data: DATA,
};
const OTHER_DEVICE: MsixMessage = MsixMessage {
    vector: 3,
    address: ADDRESS,
    data: DATA + 1,
};

/// HDA with MSI enabled and two 10 ms IOC entries, running.
fn running() -> (VirtPlatform, FlatGuestRam) {
    let mut platform = platform_with_devices(VirtPlatformDeviceConfig {
        hda_present: true,
        ..VirtPlatformDeviceConfig::default()
    });
    let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0x1_0000);
    let msi = u16::from(pcie::HDA_MSI_CAP_OFFSET);
    let cfg = |reg| pcie_cfg_gpa(pcie::HDA_BDF.1, pcie::HDA_BDF.2, reg);
    let command = u64::from(pcie::CMD_MEMORY_SPACE | pcie::CMD_BUS_MASTER);
    for index in 0..2u64 {
        let mut descriptor = [0u8; 16];
        descriptor[..8].copy_from_slice(&(PCM + index * 1920).to_le_bytes());
        descriptor[8..12].copy_from_slice(&1920u32.to_le_bytes());
        descriptor[12..16].copy_from_slice(&BDL_IOC.to_le_bytes());
        assert!(mem.write_bytes(BDL + index * 16, &descriptor));
    }
    for (address, size, value) in [
        (cfg(pcie::REG_BAR0), 4, BAR),
        (cfg(pcie::REG_COMMAND_STATUS), 2, command),
        (cfg(msi + 4), 4, ADDRESS as u32 as u64),
        (cfg(msi + 8), 4, ADDRESS >> 32),
        (cfg(msi + 12), 2, u64::from(DATA)),
        (cfg(msi + 2), 2, 1),
        (BAR + REG_GCTL, 4, 1),
        (BAR + REG_INTCTL, 4, u64::from(INTCTL_GIE | INTCTL_STREAM0)),
        (BAR + REG_SD_BDPL, 4, BDL),
        (BAR + REG_SD_CBL, 4, 3840),
        (BAR + REG_SD_LVI, 2, 1),
        (BAR + REG_SD_FMT, 2, 0x0011),
        (BAR + REG_SD_CTL, 1, u64::from(SDCTL_RUN | SDCTL_IOCE)),
    ] {
        let op = MmioOp::Write { size, value };
        assert_eq!(
            platform.on_mmio(address, op, &mut mem),
            MmioOutcome::WriteAck
        );
    }
    (platform, mem)
}

#[test]
fn the_service_poll_takes_only_hdas_msi_at_its_own_host_time() {
    let (mut platform, mut mem) = running();
    let start = Instant::now();
    platform.pending_msix.push(OTHER_DEVICE);
    assert_eq!(
        platform.poll_hda_taking_msi(&mut mem, start),
        None,
        "time base"
    );
    let lpib = |platform: &mut VirtPlatform, mem: &mut FlatGuestRam| {
        let op = MmioOp::Read { size: 4 };
        platform.on_mmio(BAR + REG_SD_LPIB, op, mem)
    };
    let now = start + Duration::from_millis(5);
    assert_eq!(platform.poll_hda_taking_msi(&mut mem, now), None);
    assert_eq!(platform.host_now, Some(now));
    assert_eq!(lpib(&mut platform, &mut mem), MmioOutcome::ReadValue(960));
    let now = start + Duration::from_millis(10);
    assert_eq!(
        platform.poll_hda_taking_msi(&mut mem, now),
        Some(HDA_MESSAGE),
        "the IOC's MSI goes to the caller"
    );
    assert_eq!(lpib(&mut platform, &mut mem), MmioOutcome::ReadValue(1920));
    // A vCPU drain right after neither repeats HDA's message nor lost the other.
    platform.poll_hda(&mut mem);
    assert_eq!(platform.take_pending_msix(), [OTHER_DEVICE]);
    let now = start + Duration::from_millis(20);
    assert_eq!(
        platform.poll_hda_taking_msi(&mut mem, now),
        None,
        "BCIS unacked"
    );
}

#[test]
fn a_platform_reset_keeps_the_activity_handle_and_clears_it() {
    let (mut platform, mut mem) = running();
    let activity = platform.hda_stream_activity().expect("HDA present");
    assert!(activity.running());
    platform.reset().expect("reset without a TPM");
    assert!(!activity.running());
    let after = platform.hda_stream_activity().expect("HDA present");
    assert!(Arc::ptr_eq(&activity, &after));
    assert_eq!(platform.poll_hda_taking_msi(&mut mem, Instant::now()), None);
}

#[test]
fn without_hda_there_is_no_activity_and_no_message() {
    let mut platform = platform_with_devices(VirtPlatformDeviceConfig::default());
    let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0x1000);
    assert!(platform.hda_stream_activity().is_none());
    assert_eq!(platform.poll_hda_taking_msi(&mut mem, Instant::now()), None);
    assert!(platform.take_pending_msix().is_empty());
}
