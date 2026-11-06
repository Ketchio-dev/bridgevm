//! A guest-visible dump of the HDA controller and codec. layout_parity.rs compares the
//! microphone-off layout with main's. The same file makes the fixture on main's tree:
//! `BV_HDA_LAYOUT_OUT=<file> cargo test -p bridgevm-hvf --lib hda::tests::layout_dump -- --ignored`.

use super::super::*;
use crate::platform_virt::FlatGuestRam;
use std::fmt::Write as _;

const DUMP_RAM: u64 = 0x1000_0000;

fn registers(ctrl: &HdaController, out: &mut String, label: &str) {
    for offset in (0x00u64..0xc0).chain(0x2080..0x20c0) {
        if (0x30..0x34).contains(&offset) {
            continue; // WALCLK counts time
        }
        let _ = writeln!(
            out,
            "{label} {offset:#06x} {:#04x}",
            ctrl.mmio_read(offset, 1)
        );
    }
}

fn verb12(nid: u8, verb: u16, payload: u8) -> u32 {
    (u32::from(nid) << 20) | (u32::from(verb) << 8) | u32::from(payload)
}

fn verb4(nid: u8, verb: u8, payload: u16) -> u32 {
    (u32::from(nid) << 20) | (u32::from(verb) << 16) | u32::from(payload)
}

/// Every register byte at power-on, after CRST and after programming SD0 and the
/// next descriptor's offsets; the INTCTL bits that stick; and the codec's parameter
/// and GET responses for nodes 0 to 7, plus a second codec address.
pub(super) fn layout_dump(ctrl: &mut HdaController) -> String {
    let mut mem = FlatGuestRam::new(DUMP_RAM, 0x10000);
    let mut out = String::new();
    registers(ctrl, &mut out, "power-on");
    ctrl.mmio_write(0x08, 4, 1, &mut mem); // GCTL.CRST
    registers(ctrl, &mut out, "crst");
    ctrl.mmio_write(0x20, 4, 0xffff_ffff, &mut mem);
    let _ = writeln!(out, "intctl-all {:#010x}", ctrl.mmio_read(0x20, 4));
    ctrl.mmio_write(0x20, 4, 0, &mut mem);
    for base in [0x80u64, 0xa0] {
        ctrl.mmio_write(base + 0x08, 4, 0x1000, &mut mem); // CBL
        ctrl.mmio_write(base + 0x0c, 2, 1, &mut mem); // LVI
        ctrl.mmio_write(base + 0x12, 2, 0x0011, &mut mem); // FMT
        ctrl.mmio_write(base + 0x18, 4, DUMP_RAM + 0x1000, &mut mem); // BDPL
        ctrl.mmio_write(base + 0x1c, 4, 0, &mut mem); // BDPU
        ctrl.mmio_write(base, 1, 0x1c, &mut mem); // CTL: IOCE, FEIE, DEIE without RUN
        registers(ctrl, &mut out, &format!("programmed-{base:#04x}"));
    }
    for nid in 0..8u8 {
        for parameter in 0..=0x14u8 {
            let response = ctrl.codec_verb(verb12(nid, 0xf00, parameter));
            let _ = writeln!(out, "param nid={nid} {parameter:#04x} {response:#010x}");
        }
        for (verb, payload) in [
            (0xf01, 0),
            (0xf02, 0),
            (0xf02, 4),
            (0xf05, 0),
            (0xf06, 0),
            (0xf07, 0),
            (0xf08, 0),
            (0xf09, 0),
            (0xf0c, 0),
            (0xf0d, 0),
            (0xf0f, 0),
            (0xf1c, 0),
            (0xf20, 0),
        ] {
            let response = ctrl.codec_verb(verb12(nid, verb, payload));
            let _ = writeln!(out, "get nid={nid} {verb:#05x}/{payload} {response:#010x}");
        }
        for (verb, payload) in [
            (0xa, 0u16),
            (0xb, 0x8000),
            (0xb, 0xa000),
            (0xb, 0x0000),
            (0xb, 0x2000),
        ] {
            let response = ctrl.codec_verb(verb4(nid, verb, payload));
            let _ = writeln!(
                out,
                "get4 nid={nid} {verb:#x}/{payload:#06x} {response:#010x}"
            );
        }
    }
    let other = ctrl.codec_verb((1 << 28) | verb12(0, 0xf00, 0));
    let _ = writeln!(out, "codec1 param 0x00 {other:#010x}");
    out
}

#[test]
#[ignore = "writes the dump to BV_HDA_LAYOUT_OUT; run on main's tree to make the fixture"]
fn write_layout_dump() {
    let path = std::env::var_os("BV_HDA_LAYOUT_OUT").expect("BV_HDA_LAYOUT_OUT");
    let mut ctrl = HdaController::with_pcm_output_path::<&std::path::Path>(None);
    std::fs::write(path, layout_dump(&mut ctrl)).unwrap();
}
