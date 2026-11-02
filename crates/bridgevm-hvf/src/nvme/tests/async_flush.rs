//! FLUSH on the worker thread (flush_worker.rs): the command completes only
//! after its sync, holds its completion slot meanwhile, and admin commands and
//! resets wait for it.

use super::super::*;
use super::helpers::*;
use crate::fwcfg::GuestMemoryMut;
use std::fs;
use std::path::PathBuf;

const IO_SQ_DOORBELL: u64 = REG_DOORBELL_BASE + 2 * 4;
const IO_CQ_DOORBELL: u64 = REG_DOORBELL_BASE + 3 * 4;

fn async_controller(name: &str) -> (NvmeController, FakeMem, PathBuf) {
    let path = temp_path(name);
    fs::write(&path, vec![0u8; LBA_SIZE * 8]).unwrap();
    let (mut ctrl, mut mem) = enabled_controller_with_raw_file(&path, true, 0x10000);
    ctrl.set_async_flush(true);
    create_io_queue_pair(
        &mut ctrl,
        &mut mem,
        0,
        CREATE_IO_CQ_PC_BIT | CREATE_IO_CQ_IEN_BIT,
    );
    (ctrl, mem, path)
}

fn put_io(mem: &mut FakeMem, slot: u16, sqe: &[u8; 64]) {
    assert!(mem.write_bytes(IO_SQ_BASE + u64::from(slot) * SQ_ENTRY_SIZE, sqe));
}

fn command_id(entry: &[u8; 16]) -> u16 {
    u16::from_le_bytes([entry[12], entry[13]])
}

#[test]
fn a_flush_completes_only_after_its_sync_and_raises_the_cq_interrupt() {
    let (mut ctrl, mut mem, path) = async_controller("async-flush-complete");
    put_io(
        &mut mem,
        0,
        &encode_sqe(NVM_OP_FLUSH, 0x76, NSID, 0, 0, 0, 0),
    );
    ctrl.mmio_write(IO_SQ_DOORBELL, 4, 1);
    assert!(
        ctrl.process(&mut mem).is_empty(),
        "nothing completes before the sync"
    );
    assert_eq!(read_completion(&mem, IO_CQ_BASE, 0), [0; 16]);

    let mut events = Vec::new();
    ctrl.complete_flushes(&mut mem, &mut events, true);
    let entry = read_completion(&mem, IO_CQ_BASE, 0);
    assert_eq!(completion_status(&entry), SC_SUCCESS);
    assert_eq!(command_id(&entry), 0x76);
    assert_eq!(events.len(), 1, "the completion raises its CQ's interrupt");
    assert!(!ctrl.flushes_ready());
    let _ = fs::remove_file(path);
}

#[test]
fn a_flush_in_flight_holds_its_completion_slot() {
    let (mut ctrl, mut mem, path) = async_controller("async-flush-slot");
    let read = |cid| encode_sqe(NVM_OP_READ, cid, NSID, DATA_BASE, 0, 0, 0);
    // Six reads fill six of the CQ's seven usable slots; the guest consumes none.
    for slot in 0..6 {
        put_io(&mut mem, slot, &read(slot));
    }
    ctrl.mmio_write(IO_SQ_DOORBELL, 4, 6);
    ctrl.process(&mut mem);
    // A flush takes the last slot; the read behind it must wait.
    put_io(
        &mut mem,
        6,
        &encode_sqe(NVM_OP_FLUSH, 0x50, NSID, 0, 0, 0, 0),
    );
    put_io(&mut mem, 7, &read(0x51));
    ctrl.mmio_write(IO_SQ_DOORBELL, 4, 0);
    ctrl.process(&mut mem);
    assert_eq!(
        read_completion(&mem, IO_CQ_BASE, 6),
        [0; 16],
        "no read took the held slot"
    );

    let mut events = Vec::new();
    ctrl.complete_flushes(&mut mem, &mut events, true);
    assert_eq!(command_id(&read_completion(&mem, IO_CQ_BASE, 6)), 0x50);
    // The guest consumes all seven; the read then completes in the next slot.
    ctrl.mmio_write(IO_CQ_DOORBELL, 4, 7);
    ctrl.process(&mut mem);
    let entry = read_completion(&mem, IO_CQ_BASE, 7);
    assert_eq!(
        (command_id(&entry), completion_status(&entry)),
        (0x51, SC_SUCCESS)
    );
    let _ = fs::remove_file(path);
}

#[test]
fn an_admin_command_first_completes_the_flushes_in_flight() {
    let (mut ctrl, mut mem, path) = async_controller("async-flush-admin");
    put_io(
        &mut mem,
        0,
        &encode_sqe(NVM_OP_FLUSH, 0x60, NSID, 0, 0, 0, 0),
    );
    ctrl.mmio_write(IO_SQ_DOORBELL, 4, 1);
    ctrl.process(&mut mem);
    // Deleting the SQ must not race the flush's completion.
    let delete_sq = encode_sqe(ADMIN_OP_DELETE_IO_SQ, 0x61, 0, 0, 1, 0, 0);
    submit_admin(&mut ctrl, &mut mem, 2, &delete_sq);
    assert_eq!(command_id(&read_completion(&mem, IO_CQ_BASE, 0)), 0x60);
    assert_eq!(
        completion_status(&read_completion(&mem, ACQ_BASE, 2)),
        SC_SUCCESS
    );
    let _ = fs::remove_file(path);
}

#[test]
fn a_reset_waits_for_the_sync_and_drops_the_completion() {
    let (mut ctrl, mut mem, path) = async_controller("async-flush-reset");
    put_io(
        &mut mem,
        0,
        &encode_sqe(NVM_OP_FLUSH, 0x70, NSID, 0, 0, 0, 0),
    );
    ctrl.mmio_write(IO_SQ_DOORBELL, 4, 1);
    ctrl.process(&mut mem);
    ctrl.mmio_write(REG_CC, 4, 0);
    assert!(!ctrl.flushes_ready() && !ctrl.flushes_in_flight());
    let mut events = Vec::new();
    ctrl.complete_flushes(&mut mem, &mut events, true);
    assert!(events.is_empty());
    assert_eq!(read_completion(&mem, IO_CQ_BASE, 0), [0; 16]);
    let _ = fs::remove_file(path);
}

#[test]
fn memory_disks_and_async_off_flush_inline() {
    let (mut ctrl, mut mem) = enabled_controller();
    ctrl.set_async_flush(true);
    create_io_queue_pair(&mut ctrl, &mut mem, 0, CREATE_IO_CQ_PC_BIT);
    assert_eq!(
        submit_io(
            &mut ctrl,
            &mut mem,
            0,
            &encode_sqe(NVM_OP_FLUSH, 1, NSID, 0, 0, 0, 0)
        ),
        SC_SUCCESS
    );

    let path = temp_path("async-flush-off");
    fs::write(&path, vec![0u8; LBA_SIZE * 8]).unwrap();
    let (mut ctrl, mut mem) = enabled_controller_with_raw_file(&path, true, 0x10000);
    create_io_queue_pair(&mut ctrl, &mut mem, 0, CREATE_IO_CQ_PC_BIT);
    assert_eq!(
        submit_io(
            &mut ctrl,
            &mut mem,
            0,
            &encode_sqe(NVM_OP_FLUSH, 2, NSID, 0, 0, 0, 0)
        ),
        SC_SUCCESS
    );
    assert_eq!(raw_file_sync_attempts(&ctrl.disk), 1);
    let _ = fs::remove_file(path);
}
