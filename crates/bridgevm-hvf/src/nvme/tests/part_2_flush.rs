//! Flush and durability: FLUSH, the volatile write cache, and raw-file sync.
//!
//! Split out of part_2.rs. These share one question -- when the guest is told
//! its writes are durable, did the host actually make them so.

use super::super::*;
use super::helpers::*;
use crate::fwcfg::GuestMemoryMut;
use crate::pcie::NVME_MSIX_VECTOR_COUNT;
use std::fs;
use std::io;

#[test]
fn flush_command_completes_for_namespace_and_broadcast_nsid() {
    let (mut ctrl, mut mem) = enabled_controller();
    create_io_queue_pair(&mut ctrl, &mut mem, 0, CREATE_IO_CQ_PC_BIT);

    let flush = encode_sqe(NVM_OP_FLUSH, 0x76, NSID, 0, 0, 0, 0);
    assert!(mem.write_bytes(IO_SQ_BASE, &flush));
    ctrl.mmio_write(REG_DOORBELL_BASE + 2 * 4, 4, 1);
    ctrl.process(&mut mem);
    assert_eq!(
        completion_status(&read_completion(&mem, IO_CQ_BASE, 0)),
        SC_SUCCESS
    );

    let broadcast_flush = encode_sqe(NVM_OP_FLUSH, 0x77, u32::MAX, 0, 0, 0, 0);
    assert!(mem.write_bytes(IO_SQ_BASE + SQ_ENTRY_SIZE, &broadcast_flush));
    ctrl.mmio_write(REG_DOORBELL_BASE + 2 * 4, 4, 2);
    ctrl.process(&mut mem);
    assert_eq!(
        completion_status(&read_completion(&mem, IO_CQ_BASE, 1)),
        SC_SUCCESS
    );
}

#[test]
fn disabling_the_write_cache_flushes_every_namespace_and_reports_failure() {
    // Disabling the volatile write cache tells the guest its writes are now
    // durable, so the flush that makes that true has to actually happen on
    // every namespace, and a failure has to reach the guest. Reporting success
    // after a failed sync tells the guest data is safe when it is not.
    let primary = temp_path("vwc-primary");
    let secondary = temp_path("vwc-secondary");
    fs::write(&primary, vec![0u8; LBA_SIZE * 8]).unwrap();
    fs::write(&secondary, vec![0u8; LBA_SIZE * 8]).unwrap();

    let (mut ctrl, mut mem) = enabled_controller_with_raw_file(&primary, true, 0x10000);
    ctrl.attach_second_namespace_raw_file(&secondary, true)
        .unwrap();

    let disable = encode_sqe(
        ADMIN_OP_SET_FEATURES,
        0x80,
        0,
        0,
        u32::from(FEATURE_VOLATILE_WRITE_CACHE),
        0,
        0,
    );
    submit_admin(&mut ctrl, &mut mem, 0, &disable);
    assert_eq!(
        completion_status(&read_completion(&mem, ACQ_BASE, 0)),
        SC_SUCCESS
    );
    assert_eq!(raw_file_sync_attempts(&ctrl.disk), 1);
    assert_eq!(
        raw_file_sync_attempts(ctrl.disk2.as_ref().unwrap()),
        1,
        "the second namespace must be flushed too"
    );

    // Re-enabling then disabling again with the primary sync failing must
    // surface the error rather than claim the cache was flushed.
    ctrl.volatile_write_cache_enabled = true;
    set_raw_file_sync_failure(&mut ctrl.disk, Some(io::ErrorKind::Other));
    submit_admin(&mut ctrl, &mut mem, 1, &disable);
    assert_eq!(
        completion_status(&read_completion(&mem, ACQ_BASE, 1)),
        SC_INTERNAL_DEVICE_ERROR,
        "a failed sync must not be reported to the guest as success"
    );
    set_raw_file_sync_failure(&mut ctrl.disk, None);

    // The same must hold when it is the second namespace that fails.
    ctrl.volatile_write_cache_enabled = true;
    set_raw_file_sync_failure(ctrl.disk2.as_mut().unwrap(), Some(io::ErrorKind::Other));
    submit_admin(&mut ctrl, &mut mem, 2, &disable);
    assert_eq!(
        completion_status(&read_completion(&mem, ACQ_BASE, 2)),
        SC_INTERNAL_DEVICE_ERROR,
        "a failed second-namespace sync must reach the guest"
    );
}

#[test]
fn raw_file_flush_syncs_selected_and_broadcast_write_back_namespaces() {
    let primary = temp_path("flush-primary");
    let secondary = temp_path("flush-secondary");
    fs::write(&primary, vec![0u8; LBA_SIZE * 8]).unwrap();
    fs::write(&secondary, vec![0u8; LBA_SIZE * 8]).unwrap();

    let (mut ctrl, mut mem) = enabled_controller_with_raw_file(&primary, true, 0x10000);
    ctrl.attach_second_namespace_raw_file(&secondary, true)
        .unwrap();
    create_io_queue_pair(&mut ctrl, &mut mem, 0, CREATE_IO_CQ_PC_BIT);

    assert_eq!(
        submit_io(
            &mut ctrl,
            &mut mem,
            0,
            &encode_sqe(NVM_OP_FLUSH, 0x76, NSID, 0, 0, 0, 0),
        ),
        SC_SUCCESS
    );
    assert_eq!(raw_file_sync_attempts(&ctrl.disk), 1);
    assert_eq!(raw_file_sync_attempts(ctrl.disk2.as_ref().unwrap()), 0);

    assert_eq!(
        submit_io(
            &mut ctrl,
            &mut mem,
            1,
            &encode_sqe(NVM_OP_FLUSH, 0x77, NSID2, 0, 0, 0, 0),
        ),
        SC_SUCCESS
    );
    assert_eq!(raw_file_sync_attempts(&ctrl.disk), 1);
    assert_eq!(raw_file_sync_attempts(ctrl.disk2.as_ref().unwrap()), 1);

    assert_eq!(
        submit_io(
            &mut ctrl,
            &mut mem,
            2,
            &encode_sqe(NVM_OP_FLUSH, 0x78, u32::MAX, 0, 0, 0, 0),
        ),
        SC_SUCCESS
    );
    assert_eq!(raw_file_sync_attempts(&ctrl.disk), 2);
    assert_eq!(raw_file_sync_attempts(ctrl.disk2.as_ref().unwrap()), 2);

    drop(ctrl);
    fs::remove_file(primary).ok();
    fs::remove_file(secondary).ok();
}

#[test]
fn raw_file_flush_skips_read_only_overlay_without_failing_guest_command() {
    let source = temp_path("flush-read-only-overlay");
    fs::write(&source, vec![0u8; LBA_SIZE * 8]).unwrap();
    let (mut ctrl, mut mem) = enabled_controller_with_raw_file(&source, false, 0x10000);
    create_io_queue_pair(&mut ctrl, &mut mem, 0, CREATE_IO_CQ_PC_BIT);

    assert_eq!(
        submit_io(
            &mut ctrl,
            &mut mem,
            0,
            &encode_sqe(NVM_OP_FLUSH, 0x79, NSID, 0, 0, 0, 0),
        ),
        SC_SUCCESS
    );
    assert_eq!(raw_file_sync_attempts(&ctrl.disk), 0);
    ctrl.flush_disk().unwrap();
    assert_eq!(raw_file_sync_attempts(&ctrl.disk), 0);

    drop(ctrl);
    fs::remove_file(source).ok();
}

#[test]
fn raw_file_sync_failures_reach_host_and_guest_flush_callers() {
    let primary = temp_path("flush-failure-primary");
    let secondary = temp_path("flush-failure-secondary");
    fs::write(&primary, vec![0u8; LBA_SIZE * 8]).unwrap();
    fs::write(&secondary, vec![0u8; LBA_SIZE * 8]).unwrap();

    let (mut ctrl, mut mem) = enabled_controller_with_raw_file(&primary, true, 0x10000);
    ctrl.attach_second_namespace_raw_file(&secondary, true)
        .unwrap();
    set_raw_file_sync_failure(&mut ctrl.disk, Some(io::ErrorKind::Other));

    let error = ctrl.flush_disk().unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::Other);
    assert_eq!(raw_file_sync_attempts(&ctrl.disk), 1);

    create_io_queue_pair(&mut ctrl, &mut mem, 0, CREATE_IO_CQ_PC_BIT);
    assert_eq!(
        submit_io(
            &mut ctrl,
            &mut mem,
            0,
            &encode_sqe(NVM_OP_FLUSH, 0x7a, NSID, 0, 0, 0, 0),
        ),
        SC_INTERNAL_DEVICE_ERROR
    );

    // A broadcast must still try every namespace even when the primary
    // namespace fails, then report the aggregate failure to the guest.
    assert_eq!(
        submit_io(
            &mut ctrl,
            &mut mem,
            1,
            &encode_sqe(NVM_OP_FLUSH, 0x7b, u32::MAX, 0, 0, 0, 0),
        ),
        SC_INTERNAL_DEVICE_ERROR
    );
    assert_eq!(raw_file_sync_attempts(&ctrl.disk), 3);
    assert_eq!(raw_file_sync_attempts(ctrl.disk2.as_ref().unwrap()), 1);

    set_raw_file_sync_failure(&mut ctrl.disk, None);
    set_raw_file_sync_failure(ctrl.disk2.as_mut().unwrap(), Some(io::ErrorKind::Other));
    let error = ctrl.flush_second_namespace_disk().unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::Other);

    drop(ctrl);
    fs::remove_file(primary).ok();
    fs::remove_file(secondary).ok();
}

#[test]
fn io_completion_queue_uses_interrupt_vector_from_cdw11_high_half() {
    let (mut ctrl, mut mem) = enabled_controller();
    let cdw10 = (u32::from(QDEPTH - 1) << 16) | 1;
    let cq_cdw11 = CREATE_IO_CQ_PC_BIT | CREATE_IO_CQ_IEN_BIT | (1u32 << CREATE_IO_CQ_IV_SHIFT);

    submit_admin(
        &mut ctrl,
        &mut mem,
        0,
        &encode_sqe(ADMIN_OP_CREATE_IO_CQ, 1, 0, IO_CQ_BASE, cdw10, cq_cdw11, 0),
    );
    assert_eq!(
        completion_status(&read_completion(&mem, ACQ_BASE, 0)),
        SC_SUCCESS
    );

    submit_admin(
        &mut ctrl,
        &mut mem,
        1,
        &encode_sqe(
            ADMIN_OP_CREATE_IO_SQ,
            2,
            0,
            IO_SQ_BASE,
            cdw10,
            1u32 << 16,
            0,
        ),
    );
    assert_eq!(
        completion_status(&read_completion(&mem, ACQ_BASE, 1)),
        SC_SUCCESS
    );

    let read_cmd = encode_sqe(NVM_OP_READ, 0x44, NSID, DATA_BASE, 0, 0, 0);
    assert!(mem.write_bytes(IO_SQ_BASE, &read_cmd));
    ctrl.mmio_write(REG_DOORBELL_BASE + 2 * 4, 4, 1);

    assert_eq!(
        ctrl.process(&mut mem),
        vec![NvmeCompletionEvent { cqid: 1, vector: 1 }],
        "CQ interrupt vector is CDW11[31:16], not the low PC/IEN bits"
    );
    assert_eq!(
        completion_status(&read_completion(&mem, IO_CQ_BASE, 0)),
        SC_SUCCESS
    );
}

#[test]
fn create_io_completion_queue_accepts_all_advertised_io_vectors() {
    for vector in 1..NVME_MSIX_VECTOR_COUNT {
        let (mut ctrl, mut mem) = enabled_controller();
        let cdw10 = (u32::from(QDEPTH - 1) << 16) | 1;
        let cq_cdw11 = CREATE_IO_CQ_PC_BIT
            | CREATE_IO_CQ_IEN_BIT
            | (u32::from(vector) << CREATE_IO_CQ_IV_SHIFT);

        submit_admin(
            &mut ctrl,
            &mut mem,
            0,
            &encode_sqe(ADMIN_OP_CREATE_IO_CQ, 1, 0, IO_CQ_BASE, cdw10, cq_cdw11, 0),
        );
        assert_eq!(
            completion_status(&read_completion(&mem, ACQ_BASE, 0)),
            SC_SUCCESS,
            "CREATE IO CQ should accept MSI-X vector {vector}"
        );
    }
}

#[test]
fn create_io_queues_reject_depth_beyond_advertised_mqes() {
    let (mut ctrl, mut mem) = enabled_controller();
    let oversized_cdw10 = (u32::from(MAX_QUEUE_ENTRIES) << 16) | 1;
    submit_admin(
        &mut ctrl,
        &mut mem,
        0,
        &encode_sqe(
            ADMIN_OP_CREATE_IO_CQ,
            1,
            0,
            IO_CQ_BASE,
            oversized_cdw10,
            CREATE_IO_CQ_PC_BIT,
            0,
        ),
    );
    assert_eq!(
        completion_status(&read_completion(&mem, ACQ_BASE, 0)),
        SC_INVALID_FIELD
    );

    let valid_cdw10 = (u32::from(QDEPTH - 1) << 16) | 1;
    submit_admin(
        &mut ctrl,
        &mut mem,
        1,
        &encode_sqe(
            ADMIN_OP_CREATE_IO_CQ,
            1,
            0,
            IO_CQ_BASE,
            valid_cdw10,
            CREATE_IO_CQ_PC_BIT,
            0,
        ),
    );
    assert_eq!(
        completion_status(&read_completion(&mem, ACQ_BASE, 1)),
        SC_SUCCESS
    );

    submit_admin(
        &mut ctrl,
        &mut mem,
        2,
        &encode_sqe(
            ADMIN_OP_CREATE_IO_SQ,
            2,
            0,
            IO_SQ_BASE,
            (u32::from(MAX_QUEUE_ENTRIES) << 16) | 2,
            1u32 << 16,
            0,
        ),
    );
    assert_eq!(
        completion_status(&read_completion(&mem, ACQ_BASE, 2)),
        SC_INVALID_FIELD
    );

    submit_admin(
        &mut ctrl,
        &mut mem,
        3,
        &encode_sqe(
            ADMIN_OP_CREATE_IO_CQ,
            3,
            0,
            IO_CQ_BASE,
            (u32::from(u16::MAX) << 16) | 1,
            CREATE_IO_CQ_PC_BIT,
            0,
        ),
    );
    assert_eq!(
        completion_status(&read_completion(&mem, ACQ_BASE, 3)),
        SC_INVALID_FIELD
    );

    submit_admin(
        &mut ctrl,
        &mut mem,
        4,
        &encode_sqe(
            ADMIN_OP_CREATE_IO_SQ,
            4,
            0,
            IO_SQ_BASE,
            (u32::from(u16::MAX) << 16) | 2,
            1u32 << 16,
            0,
        ),
    );
    assert_eq!(
        completion_status(&read_completion(&mem, ACQ_BASE, 4)),
        SC_INVALID_FIELD
    );
}
