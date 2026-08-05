//! Positional disk I/O must not disturb the file's shared offset.
//!
//! `seek` mutates the offset held by the open file description, which every
//! clone of a `File` shares. Two paths that each seek-then-read are individually
//! correct and still race. `read_exact_at` carries the offset per call, so it
//! cannot.
//!
//! These fail if the disk reverts to seek-based I/O: they park the offset,
//! perform disk reads and writes, and require the offset to be where it was.

use crate::nvme::disk::{DiskBackend, RawFileDisk};
use std::io::{Seek, SeekFrom, Write};

fn image(tag: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("bv-posio-{}-{}", tag, std::process::id()));
    let mut bytes = vec![0u8; 8192];
    for (i, b) in bytes.iter_mut().enumerate() {
        *b = (i % 251) as u8;
    }
    let mut file = std::fs::File::create(&path).expect("create");
    file.write_all(&bytes).expect("write");
    file.sync_all().expect("sync");
    path
}

fn raw(disk: &mut DiskBackend) -> &mut RawFileDisk {
    let DiskBackend::RawFile(raw) = disk else {
        panic!("expected a raw-file disk");
    };
    raw
}

#[test]
fn a_read_does_not_move_the_shared_file_offset() {
    let path = image("read");
    let mut disk = DiskBackend::raw_file(&path, false).expect("open");
    raw(&mut disk)
        .file
        .seek(SeekFrom::Start(4096))
        .expect("park the offset");

    let mut dst = [0u8; 512];
    disk.read_at_into(1024, &mut dst).expect("read");
    assert_eq!(
        dst[0],
        (1024 % 251) as u8,
        "read landed at the wrong offset"
    );

    let after = raw(&mut disk).file.stream_position().expect("tell");
    assert_eq!(
        after, 4096,
        "the read moved the shared file offset from 4096 to {after}; \
         that is seek-based I/O, which races with any other user of the same File"
    );
}

#[test]
fn a_write_back_write_does_not_move_the_shared_file_offset() {
    let path = image("write");
    let mut disk = DiskBackend::raw_file(&path, true).expect("open");
    raw(&mut disk)
        .file
        .seek(SeekFrom::Start(4096))
        .expect("park the offset");

    disk.write_at(1024, &[0xab; 512]).expect("write");

    let after = raw(&mut disk).file.stream_position().expect("tell");
    assert_eq!(
        after, 4096,
        "the write moved the shared file offset to {after}"
    );

    let mut back = [0u8; 512];
    disk.read_at_into(1024, &mut back).expect("read back");
    assert_eq!(back, [0xab; 512], "the write landed at the wrong offset");
}

#[test]
fn a_copy_on_write_fault_does_not_move_the_shared_file_offset() {
    let path = image("cow");
    // write_back = false takes the overlay path, which reads a whole chunk from
    // the backing file before modifying it -- a third seek site.
    let mut disk = DiskBackend::raw_file(&path, false).expect("open");
    raw(&mut disk)
        .file
        .seek(SeekFrom::Start(4096))
        .expect("park the offset");

    disk.write_at(1024, &[0xcd; 64]).expect("overlay write");

    let after = raw(&mut disk).file.stream_position().expect("tell");
    assert_eq!(
        after, 4096,
        "faulting a copy-on-write chunk moved the shared file offset to {after}"
    );

    let mut back = [0u8; 64];
    disk.read_at_into(1024, &mut back).expect("read back");
    assert_eq!(
        back, [0xcd; 64],
        "the overlay write landed at the wrong offset"
    );
}
