//! Opt-in guest-RAM window dump for post-mortem disassembly of the terminal PC.
//!
//! Enabled only when BRIDGEVM_PC_DUMP is set, so normal and sealed-gate runs are
//! unaffected. The value is `path`, or `path:base_hex:len_hex` to choose an
//! explicit guest-physical window (the Boot Manager runs identity-mapped in boot
//! services, so a guest-virtual PC is also its guest-physical address). Without a
//! base/len it dumps 0x4000 bytes around the terminal PC.

use bridgevm_hvf::machine::bridgevm_pc as board;

pub(super) fn maybe_dump(ram: &[u8], pc: u64) {
    let Ok(spec) = std::env::var("BRIDGEVM_PC_DUMP") else {
        return;
    };
    let parts: Vec<&str> = spec.split(':').collect();
    let path = parts[0];
    if path.is_empty() {
        return;
    }
    let hex = |s: &str| u64::from_str_radix(s.trim_start_matches("0x"), 16).ok();
    let base = parts
        .get(1)
        .and_then(|s| hex(s))
        .unwrap_or((pc & !0xfff).saturating_sub(0x1000));
    let len = parts.get(2).and_then(|s| hex(s)).unwrap_or(0x4000) as usize;
    let off = base.wrapping_sub(board::RAM_BASE) as usize;
    if off + len <= ram.len() {
        let _ = std::fs::write(path, &ram[off..off + len]);
        eprintln!("dump base={base:#x} len={len:#x} pc={pc:#x} -> {path}");
    }
}
