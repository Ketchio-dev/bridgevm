//! Dataset Management with Deallocate (dataset_management.rs): holes in a raw
//! write-back file, the range list checked at the guest boundary, and no-ops
//! where nothing can be punched.

use super::super::*;
use super::helpers::*;
use crate::fwcfg::GuestMemoryMut;
use std::fs::{self, File, OpenOptions};
use std::os::unix::fs::{FileExt, MetadataExt};
use std::path::{Path, PathBuf};

pub(super) const AD: u32 = 1 << 2;
pub(super) const MIB: u64 = 1 << 20;
/// Logical blocks per MiB.
pub(super) const BPM: u64 = MIB / LBA_SIZE as u64;

/// A range of `blocks` logical blocks from `slba`.
pub(super) fn range(slba: u64, blocks: u64) -> [u8; 16] {
    let mut raw = [0u8; 16];
    raw[4..8].copy_from_slice(&u32::try_from(blocks).unwrap().to_le_bytes());
    raw[8..16].copy_from_slice(&slba.to_le_bytes());
    raw
}

/// One Deallocate of `ranges` from I/O SQ slot `slot`.
pub(super) fn dsm(
    ctrl: &mut NvmeController,
    mem: &mut FakeMem,
    slot: u16,
    ranges: &[[u8; 16]],
) -> u16 {
    dsm_on(ctrl, mem, slot, NSID, ranges)
}

/// The same on namespace `nsid`.
pub(super) fn dsm_on(
    ctrl: &mut NvmeController,
    mem: &mut FakeMem,
    slot: u16,
    nsid: u32,
    ranges: &[[u8; 16]],
) -> u16 {
    let nr = u32::try_from(ranges.len() - 1).unwrap();
    let sqe = dsm_sqe(mem, slot, nsid, ranges, [nr, AD], DATA_BASE + 0x800);
    submit_io(ctrl, mem, slot, &sqe)
}

/// The range list starts mid-page at `prp1`; what does not fit continues at
/// PRP2, two pages away, so only a real PRP walk reads it all.
fn dsm_with(
    ctrl: &mut NvmeController,
    mem: &mut FakeMem,
    slot: u16,
    ranges: &[[u8; 16]],
    dwords: [u32; 2],
    prp1: u64,
) -> u16 {
    let sqe = dsm_sqe(mem, slot, NSID, ranges, dwords, prp1);
    submit_io(ctrl, mem, slot, &sqe)
}

fn dsm_sqe(
    mem: &mut FakeMem,
    slot: u16,
    nsid: u32,
    ranges: &[[u8; 16]],
    [cdw10, cdw11]: [u32; 2],
    prp1: u64,
) -> [u8; 64] {
    let list = ranges.concat();
    let prp2 = DATA_BASE + 0x2000;
    let first = list.len().min(PAGE_SIZE - 0x800);
    assert!(mem.write_bytes(DATA_BASE + 0x800, &list[..first]));
    assert!(mem.write_bytes(prp2, &list[first..]));
    let cid = 0x90 + slot;
    encode_sqe_with_prps(
        NVM_OP_DATASET_MANAGEMENT,
        cid,
        nsid,
        prp1,
        prp2,
        [cdw10, cdw11, 0],
    )
}

/// A sparse image file of `len` bytes.
pub(super) fn image(name: &str, len: u64) -> PathBuf {
    let path = temp_path(name);
    File::create(&path).unwrap().set_len(len).unwrap();
    path
}

/// Non-zero bytes that differ by position.
pub(super) fn pattern(at: u64, len: u64) -> Vec<u8> {
    (at..at + len).map(|i| (i % 251 + 1) as u8).collect()
}

pub(super) fn write_pattern(path: &Path, at: u64, len: u64) {
    let file = OpenOptions::new().write(true).open(path).unwrap();
    file.write_all_at(&pattern(at, len), at).unwrap();
    file.sync_all().unwrap();
}

pub(super) fn read(path: &Path, at: u64, len: u64) -> Vec<u8> {
    let mut bytes = vec![0; usize::try_from(len).unwrap()];
    File::open(path)
        .unwrap()
        .read_exact_at(&mut bytes, at)
        .unwrap();
    bytes
}

/// The bytes at `at..at + len` are still those `write_pattern` wrote.
pub(super) fn intact(path: &Path, at: u64, len: u64) -> bool {
    read(path, at, len) == pattern(at, len)
}

pub(super) fn zeros(path: &Path, at: u64, len: u64) -> bool {
    read(path, at, len).iter().all(|&byte| byte == 0)
}

/// Bytes the host file system has allocated to the file.
pub(super) fn allocated(path: &Path) -> u64 {
    fs::metadata(path).unwrap().blocks() * 512
}

pub(super) fn raw_controller(path: &Path, write_back: bool) -> (NvmeController, FakeMem) {
    let (mut ctrl, mut mem) = enabled_controller_with_raw_file(path, write_back, 0x10000);
    create_io_queue_pair(&mut ctrl, &mut mem, 0, CREATE_IO_CQ_PC_BIT);
    (ctrl, mem)
}

pub(super) fn record_field(name: &str) -> u64 {
    let record = dsm_record();
    let value = record
        .split(' ')
        .find_map(|pair| pair.strip_prefix(name)?.strip_prefix('='))
        .unwrap_or_else(|| panic!("{name} missing from {record}"));
    value.parse().unwrap()
}

#[test]
fn identify_and_the_effects_log_advertise_dataset_management() {
    let ctrl = NvmeController::new(1 << 20);
    let id = ctrl.identify_controller();
    assert_eq!(u16::from_le_bytes([id[520], id[521]]), 1 << 2, "ONCS");
    // DLFEAT bits 2:0 = 0: what a deallocated block reads is not reported.
    assert_eq!(ctrl.identify_namespace(NSID)[33], 0, "DLFEAT");
    let log = ctrl.command_effects_log();
    let off = 1024 + usize::from(NVM_OP_DATASET_MANAGEMENT) * 4;
    assert_eq!(
        u32::from_le_bytes(log[off..off + 4].try_into().unwrap()),
        CMD_EFFECT_CSUPP | CMD_EFFECT_LBCC
    );
}

#[cfg(target_os = "macos")]
#[test]
fn deallocated_ranges_of_a_raw_file_go_back_to_the_host() {
    let path = image("dsm-punch", 64 * MIB);
    write_pattern(&path, 16 * MIB, 8 * MIB);
    let before = allocated(&path);
    let (mut ctrl, mut mem) = raw_controller(&path, true);

    let ranges = [range(16 * BPM, 4 * BPM), range(22 * BPM, 2 * BPM)];
    assert_eq!(dsm(&mut ctrl, &mut mem, 0, &ranges), SC_SUCCESS);

    let after = allocated(&path);
    assert!(
        before - after >= 6 * MIB,
        "allocated {before} bytes before and {after} after deallocating 6 MiB"
    );
    assert!(zeros(&path, 16 * MIB, 4 * MIB));
    assert!(intact(&path, 20 * MIB, 2 * MIB));
    assert!(zeros(&path, 22 * MIB, 2 * MIB));
    // The record counts what this test punched (other tests add to it).
    assert!(record_field("commands") >= 1);
    assert!(record_field("punched_bytes") >= 6 * MIB);
    let _ = fs::remove_file(path);
}

#[cfg(target_os = "macos")]
#[test]
fn partial_host_blocks_at_a_ranges_edges_keep_their_bytes() {
    let path = image("dsm-edges", 64 * MIB);
    write_pattern(&path, 0, MIB);
    let (mut ctrl, mut mem) = raw_controller(&path, true);

    // Bytes 1536..14848: whole 4 KiB blocks only from 4096 to 12288. The
    // second range lies inside one block, so nothing of it is punched.
    let ranges = [range(3, 26), range(41, 4)];
    assert_eq!(dsm(&mut ctrl, &mut mem, 0, &ranges), SC_SUCCESS);

    assert!(
        intact(&path, 0, 4096),
        "before the range and its first edge"
    );
    assert!(zeros(&path, 4096, 8192), "the whole blocks inside");
    assert!(intact(&path, 12288, MIB - 12288), "the last edge and after");
    let _ = fs::remove_file(path);
}

#[test]
fn a_range_outside_the_namespace_rejects_the_whole_command() {
    let path = image("dsm-bounds", MIB);
    write_pattern(&path, 0, MIB);
    let (mut ctrl, mut mem) = raw_controller(&path, true);
    let nsze = BPM;
    let rejected = SC_LBA_OUT_OF_RANGE | SC_DNR;

    // One block past the end, after a valid range: nothing is punched.
    let ranges = [range(0, nsze / 2), range(nsze - 8, 9)];
    assert_eq!(dsm(&mut ctrl, &mut mem, 0, &ranges), rejected);
    assert!(intact(&path, 0, MIB));
    // Start plus length past u64::MAX.
    let ranges = [range(u64::MAX - 3, 8)];
    assert_eq!(dsm(&mut ctrl, &mut mem, 1, &ranges), rejected);
    // A start past the end, even with no blocks.
    let ranges = [range(nsze + 1, 0)];
    assert_eq!(dsm(&mut ctrl, &mut mem, 2, &ranges), rejected);
    assert!(intact(&path, 0, MIB));

    // The length is a count, not 0-based: ending at the last block fits.
    let ranges = [range(nsze - 8, 8)];
    assert_eq!(dsm(&mut ctrl, &mut mem, 3, &ranges), SC_SUCCESS);
    let _ = fs::remove_file(path);
}

#[test]
fn the_range_list_holds_256_ranges_at_most_and_is_read_through_prps() {
    let path = image("dsm-count", MIB);
    write_pattern(&path, 0, MIB);
    let (mut ctrl, mut mem) = raw_controller(&path, true);
    // 256 ranges of 8 blocks cover the whole MiB; the second half of the
    // list comes from PRP2.
    let all: Vec<_> = (0..256).map(|i| range(i * 8, 8)).collect();

    // The bits above NR are reserved: more than 256 ranges.
    assert_eq!(
        dsm_with(&mut ctrl, &mut mem, 0, &all, [0x100, AD], DATA_BASE + 0x800),
        SC_INVALID_FIELD_DNR
    );
    // No range list in guest memory.
    assert_eq!(
        dsm_with(&mut ctrl, &mut mem, 1, &all, [255, AD], 0),
        SC_INVALID_FIELD
    );
    // Without Deallocate the attributes are only hints.
    assert_eq!(
        dsm_with(&mut ctrl, &mut mem, 2, &all, [255, 0b11], DATA_BASE + 0x800),
        SC_SUCCESS
    );
    assert!(intact(&path, 0, MIB));

    assert_eq!(dsm(&mut ctrl, &mut mem, 3, &all), SC_SUCCESS);
    if cfg!(target_os = "macos") {
        assert!(zeros(&path, 0, MIB), "every range was read and punched");
    }
    let _ = fs::remove_file(path);
}

#[test]
fn memory_disks_and_read_only_overlays_keep_their_bytes() {
    let disk = pattern(0, 64 * 1024);
    let (mut ctrl, mut mem) = enabled_controller_with_disk_and_mem_len(disk.clone(), 0x10000);
    create_io_queue_pair(&mut ctrl, &mut mem, 0, CREATE_IO_CQ_PC_BIT);
    assert_eq!(dsm(&mut ctrl, &mut mem, 0, &[range(0, 128)]), SC_SUCCESS);
    assert_eq!(ctrl.disk_image(), &disk[..]);

    // A read-only raw file keeps the guest's writes in its overlay.
    let path = image("dsm-overlay", 8 * MIB);
    write_pattern(&path, 0, MIB);
    let before = allocated(&path);
    let (mut ctrl, mut mem) = raw_controller(&path, false);
    ctrl.disk.write_at(4096, &[0x5a; 8192]).unwrap();
    assert_eq!(dsm(&mut ctrl, &mut mem, 0, &[range(0, BPM)]), SC_SUCCESS);
    assert!(ctrl.disk.read_at(4096, 8192).unwrap() == [0x5a; 8192]);
    assert!(intact(&path, 0, MIB) && zeros(&path, MIB, 7 * MIB));
    assert_eq!(allocated(&path), before);
    let _ = fs::remove_file(path);
}

#[cfg(target_os = "macos")]
#[test]
fn a_file_system_that_cannot_punch_completes_the_command_unchanged() {
    use super::super::dataset_management::test_punch;
    // HFS+ answers ENOTTY; exFAT and network file systems ENOTSUP.
    for (code, name) in [(libc::ENOTTY, "dsm-enotty"), (libc::ENOTSUP, "dsm-enotsup")] {
        let path = image(name, 64 * MIB);
        write_pattern(&path, 16 * MIB, 4 * MIB);
        let before = allocated(&path);
        let (mut ctrl, mut mem) = raw_controller(&path, true);
        test_punch::fail_with(Some(code));
        let status = dsm(&mut ctrl, &mut mem, 0, &[range(16 * BPM, 4 * BPM)]);
        test_punch::fail_with(None);
        assert_eq!(status, SC_SUCCESS, "errno {code}");
        assert!(intact(&path, 16 * MIB, 4 * MIB));
        assert_eq!(allocated(&path), before);
        let _ = fs::remove_file(path);
    }
}

#[cfg(target_os = "macos")]
#[test]
fn a_flush_after_a_deallocate_syncs_the_punched_file() {
    use super::super::flush_mode::{self, SyncKind};
    let path = image("dsm-flush", 64 * MIB);
    write_pattern(&path, 16 * MIB, 8 * MIB);
    let (mut ctrl, mut mem) = raw_controller(&path, true);
    let seen = flush_mode::recorded_syncs(&path).len();
    let syncs = || flush_mode::recorded_syncs(&path).split_off(seen);

    // Inline: the FLUSH syncs the file on the controller thread.
    assert_eq!(
        dsm(&mut ctrl, &mut mem, 0, &[range(16 * BPM, 4 * BPM)]),
        SC_SUCCESS
    );
    let flush = encode_sqe(NVM_OP_FLUSH, 0x31, NSID, 0, 0, 0, 0);
    assert_eq!(submit_io(&mut ctrl, &mut mem, 1, &flush), SC_SUCCESS);
    assert_eq!(syncs(), [SyncKind::Full]);

    // On the flush worker: it syncs a duplicate of the same descriptor, and
    // the FLUSH completes once that sync returns.
    ctrl.set_async_flush(true);
    assert_eq!(
        dsm(&mut ctrl, &mut mem, 2, &[range(20 * BPM, 4 * BPM)]),
        SC_SUCCESS
    );
    let flush = encode_sqe(NVM_OP_FLUSH, 0x33, NSID, 0, 0, 0, 0);
    assert!(mem.write_bytes(IO_SQ_BASE + 3 * SQ_ENTRY_SIZE, &flush));
    ctrl.mmio_write(REG_DOORBELL_BASE + 2 * 4, 4, 4);
    ctrl.process(&mut mem);
    let mut events = Vec::new();
    ctrl.complete_flushes(&mut mem, &mut events, true);
    let entry = read_completion(&mem, IO_CQ_BASE, 3);
    assert_eq!(
        (
            u16::from_le_bytes([entry[12], entry[13]]),
            completion_status(&entry)
        ),
        (0x33, SC_SUCCESS)
    );
    assert_eq!(syncs(), [SyncKind::Full, SyncKind::Full]);
    assert!(zeros(&path, 16 * MIB, 8 * MIB));
    drop(ctrl);
    let _ = fs::remove_file(path);
}
