//! Emit the generated Path A ACPI blobs for inspection with tools like `iasl`.
//!
//! Usage:
//!   cargo run -p bridgevm-hvf --example emit_acpi_tables -- target/bridgevm-hvf-acpi

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use bridgevm_hvf::acpi::build_acpi;

const ACPI_HEADER_LEN: usize = 36;

fn main() {
    let out_dir = env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target/bridgevm-hvf-acpi"));
    fs::create_dir_all(&out_dir).expect("create output directory");

    let blobs = build_acpi(1);
    write(&out_dir, "rsdp.bin", &blobs.rsdp);
    write(&out_dir, "tables.bin", &blobs.tables);
    write(&out_dir, "table-loader.bin", &blobs.loader);

    let mut offset = 0usize;
    let mut index = 0usize;
    while offset + ACPI_HEADER_LEN <= blobs.tables.len() {
        let sig = std::str::from_utf8(&blobs.tables[offset..offset + 4])
            .expect("ACPI signature is ASCII");
        let len = u32::from_le_bytes(
            blobs.tables[offset + 4..offset + 8]
                .try_into()
                .expect("ACPI length field"),
        ) as usize;
        assert!(len >= ACPI_HEADER_LEN, "table {sig} length too small");
        assert!(
            offset + len <= blobs.tables.len(),
            "table {sig} overruns blob"
        );
        let name = format!("{index:02}-{sig}.aml");
        write(&out_dir, &name, &blobs.tables[offset..offset + len]);
        offset += len;
        index += 1;
    }
    assert_eq!(offset, blobs.tables.len(), "trailing bytes in tables blob");

    println!("wrote ACPI blobs to {}", out_dir.display());
}

fn write(dir: &Path, name: &str, bytes: &[u8]) {
    fs::write(dir.join(name), bytes).unwrap_or_else(|err| panic!("write {name}: {err}"));
}
