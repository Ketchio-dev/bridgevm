//! The copy-on-write overlay ceiling.
//!
//! A read-only disk holds every written chunk in RAM. Without a ceiling a
//! guest can walk a 64 GiB image and consume 64 GiB of host memory, so the
//! limit has to be a device error the guest sees rather than a host OOM.

use super::DEFAULT_OVERLAY_QUOTA_BYTES;
use crate::nvme::disk::DiskBackend;
use crate::nvme::FILE_OVERLAY_CHUNK_SIZE;
use std::fs;
use std::io::ErrorKind;

fn scratch(tag: &str) -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!("bv-overlay-{}-{}", tag, std::process::id()));
    let _ = fs::remove_file(&p);
    p
}

/// A read-only backend over `len` zero bytes.
fn read_only_disk(tag: &str, len: usize) -> (DiskBackend, std::path::PathBuf) {
    let path = scratch(tag);
    fs::write(&path, vec![0u8; len]).unwrap();
    let disk = DiskBackend::raw_file(&path, false).expect("open");
    (disk, path)
}

#[test]
fn a_new_read_only_disk_starts_under_the_default_quota() {
    let (mut disk, path) = read_only_disk("default", 64 * 1024);
    let DiskBackend::RawFile(raw) = &mut disk else {
        panic!("expected a raw file backend");
    };
    assert_eq!(raw.overlay_quota_bytes, DEFAULT_OVERLAY_QUOTA_BYTES);
    assert_eq!(raw.overlay_bytes, 0);
    raw.write_at(0, &[1; 8])
        .expect("a first write is well under it");
    assert!(raw.overlay_bytes < raw.overlay_quota_bytes);
    let _ = fs::remove_file(path);
}

#[test]
fn writes_under_the_quota_are_accepted() {
    let (mut disk, path) = read_only_disk("under", 64 * 1024);
    let DiskBackend::RawFile(raw) = &mut disk else {
        panic!("expected a raw file backend");
    };
    raw.overlay_quota_bytes = FILE_OVERLAY_CHUNK_SIZE * 4;

    for i in 0..4u64 {
        raw.write_at(i * FILE_OVERLAY_CHUNK_SIZE, &[0xab; 16])
            .expect("write within quota");
    }
    assert_eq!(raw.overlay_bytes, FILE_OVERLAY_CHUNK_SIZE * 4);
    let _ = fs::remove_file(path);
}

#[test]
fn the_write_that_would_cross_the_quota_is_refused() {
    let (mut disk, path) = read_only_disk("cross", 64 * 1024);
    let DiskBackend::RawFile(raw) = &mut disk else {
        panic!("expected a raw file backend");
    };
    raw.overlay_quota_bytes = FILE_OVERLAY_CHUNK_SIZE * 2;

    raw.write_at(0, &[1; 8]).expect("first chunk");
    raw.write_at(FILE_OVERLAY_CHUNK_SIZE, &[2; 8])
        .expect("second chunk");

    let err = raw
        .write_at(FILE_OVERLAY_CHUNK_SIZE * 2, &[3; 8])
        .expect_err("the third chunk must be refused");
    assert_eq!(err.kind(), ErrorKind::OutOfMemory);
    assert!(
        err.to_string().contains("quota"),
        "the error should name the quota: {err}"
    );
    let _ = fs::remove_file(path);
}

#[test]
fn a_refused_write_does_not_grow_the_overlay() {
    let (mut disk, path) = read_only_disk("no-growth", 64 * 1024);
    let DiskBackend::RawFile(raw) = &mut disk else {
        panic!("expected a raw file backend");
    };
    raw.overlay_quota_bytes = FILE_OVERLAY_CHUNK_SIZE;
    raw.write_at(0, &[1; 8]).expect("first chunk");

    let before = raw.overlay_bytes;
    assert!(raw.write_at(FILE_OVERLAY_CHUNK_SIZE, &[2; 8]).is_err());
    assert_eq!(
        raw.overlay_bytes, before,
        "a refusal must not account bytes"
    );
    assert_eq!(raw.overlay.len(), 1);
    let _ = fs::remove_file(path);
}

#[test]
fn rewriting_a_chunk_already_held_stays_within_the_quota() {
    // The ceiling counts distinct chunks, not writes. A guest rewriting one
    // sector repeatedly must not be refused once the chunk is resident.
    let (mut disk, path) = read_only_disk("rewrite", 64 * 1024);
    let DiskBackend::RawFile(raw) = &mut disk else {
        panic!("expected a raw file backend");
    };
    raw.overlay_quota_bytes = FILE_OVERLAY_CHUNK_SIZE;

    for round in 0..32u8 {
        raw.write_at(0, &[round; 8]).expect("rewrite is not new");
    }
    assert_eq!(raw.overlay_bytes, FILE_OVERLAY_CHUNK_SIZE);
    let mut back = [0u8; 8];
    raw.read_at_into(0, &mut back).unwrap();
    assert_eq!(back, [31u8; 8]);
    let _ = fs::remove_file(path);
}

#[test]
fn data_written_before_the_quota_was_reached_is_still_readable() {
    // Hitting the ceiling must not corrupt what the guest already wrote.
    let (mut disk, path) = read_only_disk("survives", 64 * 1024);
    let DiskBackend::RawFile(raw) = &mut disk else {
        panic!("expected a raw file backend");
    };
    raw.overlay_quota_bytes = FILE_OVERLAY_CHUNK_SIZE;
    raw.write_at(0, &[0x5a; 32]).expect("first chunk");
    assert!(raw.write_at(FILE_OVERLAY_CHUNK_SIZE, &[1; 8]).is_err());

    let mut back = [0u8; 32];
    raw.read_at_into(0, &mut back).unwrap();
    assert_eq!(back, [0x5a; 32]);
    let _ = fs::remove_file(path);
}
