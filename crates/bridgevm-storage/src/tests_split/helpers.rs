//! Split test module.

use crate::*;
use bridgevm_config::Guest;
use bridgevm_config::VmManifest;
use bridgevm_config::VmMode;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

pub(super) static TEST_ID: AtomicU64 = AtomicU64::new(0);

#[test]
pub(super) fn command_stream_drain_caps_retained_output_and_reaches_eof() {
    let input = vec![b'x'; 32 * 1024];
    let (retained, exceeded) = drain_command_stream(std::io::Cursor::new(input), 1024).unwrap();

    assert_eq!(retained.len(), 1024);
    assert!(exceeded);
}

pub(super) fn temp_store() -> VmStore {
    let mut path = std::env::temp_dir();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let id = TEST_ID.fetch_add(1, Ordering::Relaxed);
    path.push(format!(
        "bridgevm-storage-test-{}-{}-{}",
        std::process::id(),
        nanos,
        id
    ));
    VmStore::new(path)
}

pub(super) fn manifest(name: &str) -> VmManifest {
    VmManifest::new(
        name,
        VmMode::Fast,
        Guest {
            os: "ubuntu".to_string(),
            version: None,
            arch: "arm64".to_string(),
        },
        "80GiB",
    )
}

pub(super) fn write_raw_tar_entry(
    path: &Path,
    entry_name: &str,
    typeflag: u8,
    link_name: Option<&str>,
    contents: &[u8],
) {
    let mut header = [0_u8; 512];
    write_tar_field(&mut header[0..100], entry_name.as_bytes());
    write_tar_octal(&mut header[100..108], 0o644);
    write_tar_octal(&mut header[108..116], 0);
    write_tar_octal(&mut header[116..124], 0);
    write_tar_octal(&mut header[124..136], contents.len() as u64);
    write_tar_octal(&mut header[136..148], 0);
    header[148..156].fill(b' ');
    header[156] = typeflag;
    if let Some(link_name) = link_name {
        write_tar_field(&mut header[157..257], link_name.as_bytes());
    }
    write_tar_field(&mut header[257..263], b"ustar\0");
    write_tar_field(&mut header[263..265], b"00");
    let checksum: u32 = header.iter().map(|byte| u32::from(*byte)).sum();
    write_tar_checksum(&mut header[148..156], checksum);

    let mut file = fs::File::create(path).unwrap();
    file.write_all(&header).unwrap();
    file.write_all(contents).unwrap();
    let padding = (512 - (contents.len() % 512)) % 512;
    file.write_all(&vec![0_u8; padding]).unwrap();
    file.write_all(&[0_u8; 1024]).unwrap();
}

pub(super) fn write_tar_field(field: &mut [u8], value: &[u8]) {
    let len = value.len().min(field.len());
    field[..len].copy_from_slice(&value[..len]);
}

pub(super) fn write_tar_octal(field: &mut [u8], value: u64) {
    let encoded = format!("{:0width$o}\0", value, width = field.len() - 1);
    write_tar_field(field, encoded.as_bytes());
}

pub(super) fn write_tar_checksum(field: &mut [u8], value: u32) {
    let encoded = format!("{value:06o}\0 ");
    write_tar_field(field, encoded.as_bytes());
}
