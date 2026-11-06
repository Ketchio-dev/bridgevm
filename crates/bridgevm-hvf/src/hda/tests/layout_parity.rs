//! With the microphone setting off, the guest sees main's HDA device: every register
//! byte and codec response equals main bb1b1e08's (main_layout_bb1b1e08.txt, written by
//! layout_dump.rs on that tree), and playback interrupts and positions as main's do.

use super::super::*;
use super::layout_dump::layout_dump;
use crate::fwcfg::GuestMemoryMut;
use crate::platform_virt::FlatGuestRam;

const MAIN: &str = include_str!("main_layout_bb1b1e08.txt");
const RAM: u64 = 0x1000_0000;

fn microphone_off() -> HdaController {
    let mut ctrl = HdaController::with_pcm_output_path::<&std::path::Path>(None);
    ctrl.set_microphone(false);
    ctrl
}

#[test]
fn without_the_microphone_every_register_and_codec_response_is_mains() {
    let dump = layout_dump(&mut microphone_off());
    let differences: Vec<(&str, &str)> = MAIN
        .lines()
        .zip(dump.lines())
        .filter(|(main, off)| main != off)
        .take(12)
        .collect();
    assert_eq!(MAIN.lines().count(), dump.lines().count(), "dump length");
    assert!(
        differences.is_empty(),
        "first differences (main, microphone off): {differences:#?}"
    );
}

#[test]
fn without_the_microphone_playback_is_sd0_with_bit_0_and_position_entry_0() {
    let mut ctrl = microphone_off();
    let mut mem = FlatGuestRam::new(RAM, 0x10000);
    for index in 0..2u64 {
        let mut descriptor = [0u8; 16];
        descriptor[..8].copy_from_slice(&(RAM + 0x2000 + index * 192).to_le_bytes());
        descriptor[8..12].copy_from_slice(&192u32.to_le_bytes());
        descriptor[12..16].copy_from_slice(&BDL_IOC.to_le_bytes());
        assert!(mem.write_bytes(RAM + 0x1000 + index * 16, &descriptor));
    }
    for (offset, size, value) in [
        (0x08, 4, 1),                // GCTL.CRST
        (0x70, 4, RAM + 0x3000 + 1), // DPLBASE, enabled
        (0x20, 4, 0x8000_0001),      // INTCTL: GIE and SD0
        (0x98, 4, RAM + 0x1000),     // SD0 BDPL
        (0x88, 4, 384),              // SD0 CBL
        (0x8c, 2, 1),                // SD0 LVI
        (0x92, 2, 0x0011),           // SD0 FMT: 48 kHz, 16-bit, stereo
        (0x80, 1, 0x06),             // SD0 CTL: RUN, IOCE
    ] {
        ctrl.mmio_write(offset, size, value, &mut mem);
    }
    ctrl.poll_for_duration(&mut mem, std::time::Duration::from_millis(1));
    assert_eq!(ctrl.mmio_read(0x84, 4), 192, "SD0 LPIB");
    assert_eq!(ctrl.mmio_read(0x2084, 4), 192, "SD0 LPIB alias");
    assert_eq!(ctrl.mmio_read(0xa4, 4), 0, "no second descriptor");
    assert_eq!(
        ctrl.mmio_read(0x83, 1) & u64::from(SDSTS_BCIS),
        u64::from(SDSTS_BCIS)
    );
    assert_eq!(ctrl.mmio_read(0x24, 4), 0x8000_0001, "INTSTS: GIS and SD0");
    let entry = |at: u64| {
        u32::from_le_bytes(
            mem.read_bytes(RAM + 0x3000 + at, 4)
                .unwrap()
                .try_into()
                .unwrap(),
        )
    };
    assert_eq!(
        (entry(0), entry(8)),
        (192, 0),
        "playback is position entry 0"
    );
}

#[test]
fn the_layout_survives_a_controller_reset() {
    let mut ctrl = microphone_off();
    let mut mem = FlatGuestRam::new(RAM, 0x1000);
    ctrl.mmio_write(0x08, 4, 1, &mut mem);
    ctrl.mmio_write(0x08, 4, 0, &mut mem);
    ctrl.mmio_write(0x08, 4, 1, &mut mem);
    assert_eq!(ctrl.mmio_read(0x00, 2), 0x1001, "GCAP");
    let mut on = HdaController::with_pcm_output_path::<&std::path::Path>(None);
    on.mmio_write(0x08, 4, 1, &mut mem);
    assert_eq!(on.mmio_read(0x00, 2), 0x1101, "with the microphone");
}
