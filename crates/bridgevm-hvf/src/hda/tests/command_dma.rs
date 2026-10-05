//! Guest MMIO ring-base rewrites must never wrap command DMA into low RAM.

use super::super::*;
use super::helpers::{verb, write};
use crate::fwcfg::GuestMemoryMut;
use crate::platform_virt::FlatGuestRam;
use std::path::Path;

const CORB: u64 = 0x1000;
const RIRB: u64 = 0x2000;

fn running(count: u16) -> (HdaController, FlatGuestRam) {
    let mut ctrl = HdaController::with_pcm_output_path::<&Path>(None);
    let mut mem = FlatGuestRam::new(0, 0x10000);
    for index in 0..64 {
        assert!(mem.write_bytes(CORB + index * 4, &verb(0, 0, 0xf00, 0).to_le_bytes()));
    }
    for (offset, size, value) in [
        (REG_GCTL, 4, 1),
        (REG_CORBLBASE, 4, CORB),
        (REG_RIRBLBASE, 4, RIRB),
        (REG_RIRBCTL, 1, u64::from(RIRBCTL_DMA)),
        (REG_CORBCTL, 1, u64::from(CORBCTL_RUN)),
        (REG_CORBWP, 2, u64::from(count)),
    ] {
        write(&mut ctrl, &mut mem, offset, size, value);
    }
    assert_eq!(ctrl.mmio_read(REG_CORBRP, 2), u64::from(count));
    assert_eq!(ctrl.mmio_read(REG_RIRBWP, 2), u64::from(count));
    assert_eq!(ctrl.mmio_read(REG_CORBSTS, 1), 0);
    write(
        &mut ctrl,
        &mut mem,
        REG_RIRBSTS,
        1,
        u64::from(RIRBSTS_RINTFL),
    );
    assert_eq!(ctrl.mmio_read(REG_RIRBSTS, 1), 0);
    (ctrl, mem)
}

#[test]
fn rewritten_corb_base_cannot_wrap_next_command_fetch() {
    let (mut ctrl, mut mem) = running(31);
    // Low RAM also detects wrapped access if overflow checks are disabled.
    assert!(mem.write_bytes(0, &verb(0, 0, 0xf00, 0).to_le_bytes()));
    let responses = mem.read_bytes(RIRB, 256 * 8).unwrap();
    write(&mut ctrl, &mut mem, REG_CORBLBASE, 4, u64::from(u32::MAX));
    write(&mut ctrl, &mut mem, REG_CORBUBASE, 4, u64::from(u32::MAX));
    write(&mut ctrl, &mut mem, REG_CORBWP, 2, 32);
    assert_ne!(ctrl.mmio_read(REG_CORBSTS, 1) & u64::from(CORBSTS_CMEI), 0);
    assert_eq!(ctrl.mmio_read(REG_CORBRP, 2), 31);
    assert_eq!(ctrl.mmio_read(REG_RIRBWP, 2), 31);
    assert_eq!(mem.read_bytes(RIRB, 256 * 8).unwrap(), responses);
    assert_eq!(ctrl.mmio_read(REG_RIRBSTS, 1), 0);
}

#[test]
fn rewritten_rirb_base_cannot_wrap_next_response_store() {
    let (mut ctrl, mut mem) = running(15);
    assert!(mem.write_bytes(0, &[0xa5; 8]));
    write(&mut ctrl, &mut mem, REG_RIRBLBASE, 4, u64::from(u32::MAX));
    write(&mut ctrl, &mut mem, REG_RIRBUBASE, 4, u64::from(u32::MAX));
    write(&mut ctrl, &mut mem, REG_CORBWP, 2, 16);
    assert_ne!(ctrl.mmio_read(REG_RIRBSTS, 1) & u64::from(RIRBSTS_OIS), 0);
    assert_eq!(ctrl.mmio_read(REG_RIRBWP, 2), 15);
    assert_eq!(ctrl.mmio_read(REG_CORBRP, 2), 16);
    assert_eq!(mem.read_bytes(0, 8).unwrap(), [0xa5; 8]);
    assert_eq!(ctrl.mmio_read(REG_CORBSTS, 1), 0);
}

#[test]
fn ordinary_ring_traffic_remains_successful_across_both_boundaries() {
    let (_ctrl, mem) = running(32);
    let response = mem.read_bytes(RIRB + 32 * 8, 8).unwrap();
    assert_eq!(
        u32::from_le_bytes(response[..4].try_into().unwrap()),
        0x1af4_0022
    );
}
