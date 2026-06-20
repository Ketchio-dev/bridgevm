//! Emit the QEMU-`virt`-shaped device tree to a file, for `dtc` verification.
//!
//! Usage: `cargo run -p bridgevm-hvf --example emit_virt_dtb -- <out.dtb> [cpus] [ram_gib]`
//! then `dtc -I dtb -O dts <out.dtb>` to inspect / validate it.

use bridgevm_hvf::dtb::{build_virt_fdt, VirtFdtConfig};

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args
        .next()
        .expect("usage: emit_virt_dtb <out.dtb> [cpus] [ram_gib]");
    let cpu_count = args.next().and_then(|s| s.parse().ok()).unwrap_or(4);
    let ram_gib: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(6);
    let dtb = build_virt_fdt(&VirtFdtConfig {
        cpu_count,
        ram_size: ram_gib * 1024 * 1024 * 1024,
    });
    std::fs::write(&path, &dtb).expect("write dtb");
    eprintln!("wrote {} bytes to {path}", dtb.len());
}
