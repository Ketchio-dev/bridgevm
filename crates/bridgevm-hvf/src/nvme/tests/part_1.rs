//! Split test module.

use super::super::*;
use super::helpers::*;
use crate::fwcfg::GuestMemoryMut;
use crate::msix::MsixMessage;
use crate::pcie::NVME_MSIX_PBA_OFFSET;
use crate::pcie::NVME_MSIX_TABLE_OFFSET;
use std::fs;

#[test]
fn cap_advertises_configured_mqes_and_zero_dstrd() {
    let ctrl = NvmeController::new(0);
    let cap = ctrl.mmio_read(REG_CAP, 8);
    // MQES is 0-based; we advertise MAX_QUEUE_ENTRIES.
    let mqes = (cap & 0xffff) as u16 + 1;
    assert_eq!(mqes, MAX_QUEUE_ENTRIES);
    // DSTRD (bits 35:32) must be 0 ⇒ 4-byte doorbell stride.
    assert_eq!((cap >> 32) & 0xf, 0);
    // NVM command set bit (37) must be set.
    assert_ne!(cap & (1 << 37), 0);
}

#[test]
fn doorbell_decode_stays_inside_the_modelled_aperture() {
    assert!(is_modelled_doorbell(REG_DOORBELL_BASE));
    assert!(is_modelled_doorbell(REG_DOORBELL_END - 4));
    assert!(!is_modelled_doorbell(REG_DOORBELL_END));
    assert!(!is_modelled_doorbell(REG_DOORBELL_BASE + 2));

    let (mut ctrl, _mem) = enabled_controller();
    ctrl.mmio_write(REG_DOORBELL_END, 4, 7);
    let admin_sq = ctrl.sqs[0].as_ref().expect("admin SQ installed");
    assert_eq!(
        admin_sq.tail_doorbell, 0,
        "BAR offsets beyond the modelled doorbells must not be treated as SQ0TDBL"
    );
}

#[test]
fn msix_table_and_pba_live_in_bar0_without_overlapping_doorbells() {
    let mut ctrl = NvmeController::new(0);
    let table = u64::from(NVME_MSIX_TABLE_OFFSET);
    let pba = u64::from(NVME_MSIX_PBA_OFFSET);

    assert_eq!(ctrl.mmio_read(table + 12, 4), 1, "vectors start masked");
    ctrl.mmio_write(table, 8, 0x0808_0000);
    ctrl.mmio_write(table + 8, 4, 35);

    assert_eq!(ctrl.raise_msix(0, true, false), None);
    assert_eq!(ctrl.mmio_read(pba, 8), 1, "masked vector sets PBA bit");

    ctrl.mmio_write(table + 12, 4, 0);
    assert_eq!(
        ctrl.drain_pending_msix(true, false),
        vec![MsixMessage {
            vector: 0,
            address: 0x0808_0000,
            data: 35,
        }]
    );
    assert_eq!(ctrl.mmio_read(pba, 8), 0);
}

#[test]
fn cap_low_half_readable_as_32_bits() {
    let ctrl = NvmeController::new(0);
    let lo = ctrl.mmio_read(REG_CAP, 4);
    let mqes = (lo & 0xffff) as u16 + 1;
    assert_eq!(mqes, MAX_QUEUE_ENTRIES);
}

#[test]
fn vs_reads_1_4_0() {
    let ctrl = NvmeController::new(0);
    assert_eq!(ctrl.mmio_read(REG_VS, 4), u64::from(NVME_VERSION_1_4_0));
    assert_eq!(ctrl.mmio_read(REG_VS, 4), 0x0001_0400);
}

#[test]
fn disk_image_constructor_pads_and_snapshots_media() {
    let mut ctrl = NvmeController::with_disk_image(vec![0xaa; LBA_SIZE + 7]);
    assert_eq!(ctrl.disk_image().len(), LBA_SIZE * 2);
    assert_eq!(ctrl.block_count(), 2);
    assert_eq!(ctrl.disk_image()[0], 0xaa);
    assert_eq!(ctrl.disk_image()[LBA_SIZE + 6], 0xaa);
    assert_eq!(ctrl.disk_image()[LBA_SIZE + 7], 0);

    ctrl.load_disk_image(vec![0xbb; 3]);
    assert_eq!(ctrl.disk_image().len(), LBA_SIZE);
    assert_eq!(ctrl.block_count(), 1);
    assert_eq!(&ctrl.disk_image()[..3], &[0xbb; 3]);
}

#[test]
fn raw_file_backend_uses_sparse_overlay_and_exports_snapshot() {
    let source = temp_path("raw-overlay-source");
    let snapshot = temp_path("raw-overlay-snapshot");
    let slba = 5u64;
    let start = slba as usize * LBA_SIZE;
    let mut disk = vec![0u8; LBA_SIZE * 16];
    let original: Vec<u8> = (0..LBA_SIZE).map(|i| 0x20 | (i % 0x20) as u8).collect();
    disk[start..start + LBA_SIZE].copy_from_slice(&original);
    fs::write(&source, &disk).unwrap();

    let (mut ctrl, mut mem) = enabled_controller_with_raw_file(&source, false, 0x10000);
    assert_eq!(ctrl.block_count(), 16);
    assert!(ctrl.disk_image_if_memory().is_none());
    create_io_queue_pair(&mut ctrl, &mut mem, 0, CREATE_IO_CQ_PC_BIT);

    let read = encode_sqe(
        NVM_OP_READ,
        0x30,
        NSID,
        DATA_BASE,
        slba as u32,
        (slba >> 32) as u32,
        0,
    );
    assert!(mem.write_bytes(IO_SQ_BASE, &read));
    ctrl.mmio_write(REG_DOORBELL_BASE + 2 * 4, 4, 1);
    ctrl.process(&mut mem);
    assert_eq!(mem.read_bytes(DATA_BASE, LBA_SIZE).unwrap(), original);

    let replacement: Vec<u8> = (0..LBA_SIZE).map(|i| 0x80 | (i % 0x40) as u8).collect();
    assert!(mem.write_bytes(DATA_BASE, &replacement));
    let write = encode_sqe(
        NVM_OP_WRITE,
        0x31,
        NSID,
        DATA_BASE,
        slba as u32,
        (slba >> 32) as u32,
        0,
    );
    assert!(mem.write_bytes(IO_SQ_BASE + SQ_ENTRY_SIZE, &write));
    ctrl.mmio_write(REG_DOORBELL_BASE + 2 * 4, 4, 2);
    ctrl.process(&mut mem);
    assert_eq!(
        completion_status(&read_completion(&mem, IO_CQ_BASE, 1)),
        SC_SUCCESS
    );

    assert_eq!(
        &fs::read(&source).unwrap()[start..start + LBA_SIZE],
        original.as_slice(),
        "read-only file backend keeps guest writes in the overlay"
    );
    assert_eq!(
        ctrl.export_disk_image(&snapshot).unwrap(),
        disk.len() as u64
    );
    assert_eq!(
        &fs::read(&snapshot).unwrap()[start..start + LBA_SIZE],
        replacement.as_slice(),
        "snapshot export applies overlay writes"
    );

    fs::remove_file(source).ok();
    fs::remove_file(snapshot).ok();
}

#[test]
fn raw_file_backend_write_back_updates_source_file() {
    let source = temp_path("raw-writeback-source");
    let slba = 3u64;
    let start = slba as usize * LBA_SIZE;
    fs::write(&source, vec![0u8; LBA_SIZE * 8]).unwrap();

    let (mut ctrl, mut mem) = enabled_controller_with_raw_file(&source, true, 0x10000);
    create_io_queue_pair(&mut ctrl, &mut mem, 0, CREATE_IO_CQ_PC_BIT);

    let replacement: Vec<u8> = (0..LBA_SIZE).map(|i| 0x40 | (i % 0x20) as u8).collect();
    assert!(mem.write_bytes(DATA_BASE, &replacement));
    let write = encode_sqe(
        NVM_OP_WRITE,
        0x32,
        NSID,
        DATA_BASE,
        slba as u32,
        (slba >> 32) as u32,
        0,
    );
    assert!(mem.write_bytes(IO_SQ_BASE, &write));
    ctrl.mmio_write(REG_DOORBELL_BASE + 2 * 4, 4, 1);
    ctrl.process(&mut mem);
    ctrl.flush_disk().unwrap();

    assert_eq!(
        &fs::read(&source).unwrap()[start..start + LBA_SIZE],
        replacement.as_slice(),
        "write-back file backend persists guest writes to the source"
    );

    fs::remove_file(source).ok();
}

#[test]
fn second_namespace_raw_file_overlay_exports_snapshot() {
    let source = temp_path("raw-nsid2-overlay-source");
    let snapshot = temp_path("raw-nsid2-overlay-snapshot");
    let slba = 2u64;
    let start = slba as usize * LBA_SIZE;
    let original: Vec<u8> = (0..LBA_SIZE).map(|i| 0x10 | (i % 0x10) as u8).collect();
    let replacement: Vec<u8> = (0..LBA_SIZE).map(|i| 0x90 | (i % 0x30) as u8).collect();
    let mut disk = vec![0u8; LBA_SIZE * 8];
    disk[start..start + LBA_SIZE].copy_from_slice(&original);
    fs::write(&source, &disk).unwrap();

    let (mut ctrl, mut mem) = enabled_controller_with_mem_len(0x10000);
    ctrl.attach_second_namespace_raw_file(&source, false)
        .unwrap();
    create_io_queue_pair(&mut ctrl, &mut mem, 0, CREATE_IO_CQ_PC_BIT);

    assert!(mem.write_bytes(DATA_BASE, &replacement));
    let write = encode_sqe(
        NVM_OP_WRITE,
        0x72,
        NSID2,
        DATA_BASE,
        slba as u32,
        (slba >> 32) as u32,
        0,
    );
    assert!(mem.write_bytes(IO_SQ_BASE, &write));
    ctrl.mmio_write(REG_DOORBELL_BASE + 2 * 4, 4, 1);
    ctrl.process(&mut mem);

    assert_eq!(
        &fs::read(&source).unwrap()[start..start + LBA_SIZE],
        original.as_slice(),
        "read-only NSID2 raw file keeps guest writes in the overlay"
    );
    assert_eq!(
        ctrl.export_second_namespace_disk_image(&snapshot).unwrap(),
        disk.len() as u64
    );
    assert_eq!(
        &fs::read(&snapshot).unwrap()[start..start + LBA_SIZE],
        replacement.as_slice(),
        "NSID2 snapshot export applies overlay writes"
    );

    fs::remove_file(source).ok();
    fs::remove_file(snapshot).ok();
}

#[test]
fn second_namespace_raw_file_write_back_updates_source_file() {
    let source = temp_path("raw-nsid2-writeback-source");
    let slba = 4u64;
    let start = slba as usize * LBA_SIZE;
    fs::write(&source, vec![0u8; LBA_SIZE * 8]).unwrap();

    let (mut ctrl, mut mem) = enabled_controller_with_mem_len(0x10000);
    ctrl.attach_second_namespace_raw_file(&source, true)
        .unwrap();
    create_io_queue_pair(&mut ctrl, &mut mem, 0, CREATE_IO_CQ_PC_BIT);

    let replacement: Vec<u8> = (0..LBA_SIZE).map(|i| 0x50 | (i % 0x20) as u8).collect();
    assert!(mem.write_bytes(DATA_BASE, &replacement));
    let write = encode_sqe(
        NVM_OP_WRITE,
        0x73,
        NSID2,
        DATA_BASE,
        slba as u32,
        (slba >> 32) as u32,
        0,
    );
    assert!(mem.write_bytes(IO_SQ_BASE, &write));
    ctrl.mmio_write(REG_DOORBELL_BASE + 2 * 4, 4, 1);
    ctrl.process(&mut mem);
    ctrl.flush_second_namespace_disk().unwrap();

    assert_eq!(
        &fs::read(&source).unwrap()[start..start + LBA_SIZE],
        replacement.as_slice(),
        "NSID2 write-back raw file persists guest writes to the source"
    );

    fs::remove_file(source).ok();
}

#[test]
fn enabling_cc_sets_csts_rdy() {
    let mut ctrl = NvmeController::new(0);
    assert_eq!(
        ctrl.mmio_read(REG_CSTS, 4) & 1,
        0,
        "RDY clear before enable"
    );
    ctrl.mmio_write(
        REG_AQA,
        4,
        (u32::from(QDEPTH - 1) << 16 | u32::from(QDEPTH - 1)).into(),
    );
    ctrl.mmio_write(REG_ASQ, 8, ASQ_BASE);
    ctrl.mmio_write(REG_ACQ, 8, ACQ_BASE);
    ctrl.mmio_write(REG_CC, 4, u64::from(CC_EN_BIT));
    assert_eq!(ctrl.mmio_read(REG_CSTS, 4) & 1, 1, "RDY follows CC.EN");
    // Disabling clears RDY again.
    ctrl.mmio_write(REG_CC, 4, 0);
    assert_eq!(ctrl.mmio_read(REG_CSTS, 4) & 1, 0);
}

#[test]
fn identify_controller_produces_completion_and_valid_struct() {
    let (mut ctrl, mut mem) = enabled_controller();
    // IDENTIFY, CNS=1 (controller), data → DATA_BASE.
    let sqe = encode_sqe(
        ADMIN_OP_IDENTIFY,
        0x55,
        0,
        DATA_BASE,
        IDENTIFY_CNS_CONTROLLER,
        0,
        0,
    );
    submit_admin(&mut ctrl, &mut mem, 0, &sqe);

    // A completion landed in slot 0 of the admin CQ, success, matching CID.
    let cqe = read_completion(&mem, ACQ_BASE, 0);
    assert_eq!(completion_status(&cqe), SC_SUCCESS);
    let cid = u16::from_le_bytes([cqe[12], cqe[13]]);
    assert_eq!(cid, 0x55);
    // Phase tag set on first lap.
    assert_eq!(cqe[14] & 1, 1);

    // The identify struct is 4 KiB and carries NN = 1 namespace and the
    // expected SQES/CQES entry-size encoding.
    let id = mem.read_bytes(DATA_BASE, PAGE_SIZE).unwrap();
    assert_eq!(id.len(), PAGE_SIZE);
    let oacs = u16::from_le_bytes([id[256], id[257]]);
    assert_eq!(
        oacs & 1,
        1,
        "OACS advertises Security Send/Receive like QEMU's default NVMe"
    );
    let nn = u32::from_le_bytes([id[516], id[517], id[518], id[519]]);
    assert_eq!(nn, 1, "one namespace");
    assert_eq!(
        id[259],
        MAX_ASYNC_EVENT_REQUESTS - 1,
        "AERL advertises the retained async-event request slots"
    );
    assert_eq!(id[512], 0x66, "SQES = 64-byte entries");
    assert_eq!(id[513], 0x44, "CQES = 16-byte entries");
    assert_eq!(
        id[525], VWC_QEMU_DEFAULT,
        "VWC advertises QEMU's present cache plus broadcast-NSID flush support"
    );
    assert!(
        id[768..1024].starts_with(b"nqn.2026-06.dev.bridgevm:bridgevm-hvf:nvme0\0"),
        "SUBNQN must be present and NUL-terminated for Linux"
    );
}

#[test]
fn identify_command_set_controller_completes_for_nvm_csi() {
    let (mut ctrl, mut mem) = enabled_controller();
    let sqe = encode_sqe(
        ADMIN_OP_IDENTIFY,
        0x57,
        0xffff_ffff,
        DATA_BASE,
        IDENTIFY_CNS_COMMAND_SET_CONTROLLER,
        u32::from(COMMAND_SET_NVM) << 24,
        0,
    );
    submit_admin(&mut ctrl, &mut mem, 0, &sqe);
    assert_eq!(
        completion_status(&read_completion(&mem, ACQ_BASE, 0)),
        SC_SUCCESS
    );

    let id = mem.read_bytes(DATA_BASE, PAGE_SIZE).unwrap();
    assert_eq!(id, vec![0u8; PAGE_SIZE]);
}

#[test]
fn identify_command_set_controller_rejects_unknown_csi() {
    let (mut ctrl, mut mem) = enabled_controller();
    let sqe = encode_sqe(
        ADMIN_OP_IDENTIFY,
        0x58,
        0xffff_ffff,
        DATA_BASE,
        IDENTIFY_CNS_COMMAND_SET_CONTROLLER,
        0xff << 24,
        0,
    );
    submit_admin(&mut ctrl, &mut mem, 0, &sqe);
    assert_eq!(
        completion_status(&read_completion(&mem, ACQ_BASE, 0)),
        SC_INVALID_FIELD
    );
}

#[test]
fn process_reports_admin_completion_vector_zero() {
    let (mut ctrl, mut mem) = enabled_controller();
    let sqe = encode_sqe(
        ADMIN_OP_IDENTIFY,
        0x56,
        0,
        DATA_BASE,
        IDENTIFY_CNS_CONTROLLER,
        0,
        0,
    );
    assert!(mem.write_bytes(ASQ_BASE, &sqe));
    ctrl.mmio_write(REG_DOORBELL_BASE, 4, 1);

    assert_eq!(
        ctrl.process(&mut mem),
        vec![NvmeCompletionEvent { cqid: 0, vector: 0 }]
    );
    let trace = ctrl.recent_command_trace();
    assert_eq!(trace.len(), 1);
    assert_eq!(trace[0].sqid, 0);
    assert_eq!(trace[0].cqid, 0);
    assert_eq!(trace[0].sq_head, 0);
    assert_eq!(trace[0].sq_tail, 1);
    assert_eq!(trace[0].sq_entry_gpa, ASQ_BASE);
    assert_eq!(trace[0].opcode, ADMIN_OP_IDENTIFY);
    assert_eq!(trace[0].command_id, 0x56);
    assert_eq!(trace[0].prp1, DATA_BASE);
    assert_eq!(trace[0].cdw10, IDENTIFY_CNS_CONTROLLER);
    assert_eq!(trace[0].status, SC_SUCCESS);
    assert!(trace[0].completion_posted);
    assert_eq!(
        trace[0].completion,
        Some(NvmeCompletionTrace { cqid: 0, vector: 0 })
    );
}

#[test]
fn process_into_reuses_caller_completion_storage() {
    let (mut ctrl, mut mem) = enabled_controller();
    let sqe = encode_sqe(
        ADMIN_OP_IDENTIFY,
        0x57,
        0,
        DATA_BASE,
        IDENTIFY_CNS_CONTROLLER,
        0,
        0,
    );
    assert!(mem.write_bytes(ASQ_BASE, &sqe));
    ctrl.mmio_write(REG_DOORBELL_BASE, 4, 1);

    let mut completions = Vec::with_capacity(4);
    let completion_capacity = completions.capacity();
    let completion_ptr = completions.as_ptr();
    ctrl.process_into(&mut mem, &mut completions);

    assert_eq!(
        completions,
        vec![NvmeCompletionEvent { cqid: 0, vector: 0 }]
    );
    assert_eq!(completions.capacity(), completion_capacity);
    assert_eq!(completions.as_ptr(), completion_ptr);

    completions.clear();
    ctrl.process_into(&mut mem, &mut completions);
    assert!(completions.is_empty());
    assert_eq!(completions.capacity(), completion_capacity);
    assert_eq!(completions.as_ptr(), completion_ptr);
}

#[test]
fn process_into_drains_only_pending_doorbelled_submission_queue() {
    let qid = MAX_IO_QUEUE_PAIRS;
    let high_io_cq = 0x4000_8000;
    let high_io_sq = 0x4000_9000;
    let (mut ctrl, mut mem) = enabled_controller_with_mem_len(0x20000);
    let cdw10 = (u32::from(QDEPTH - 1) << 16) | u32::from(qid);
    let cq_cdw11 = CREATE_IO_CQ_PC_BIT | CREATE_IO_CQ_IEN_BIT | (1u32 << CREATE_IO_CQ_IV_SHIFT);

    submit_admin(
        &mut ctrl,
        &mut mem,
        0,
        &encode_sqe(ADMIN_OP_CREATE_IO_CQ, 1, 0, high_io_cq, cdw10, cq_cdw11, 0),
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
            high_io_sq,
            cdw10,
            u32::from(qid) << 16,
            0,
        ),
    );
    assert_eq!(
        completion_status(&read_completion(&mem, ACQ_BASE, 1)),
        SC_SUCCESS
    );

    let read_cmd = encode_sqe(NVM_OP_READ, 0x70, NSID, DATA_BASE, 0, 0, 0);
    assert!(mem.write_bytes(high_io_sq, &read_cmd));
    let mut completions = Vec::new();
    ctrl.process_into(&mut mem, &mut completions);
    assert!(
        completions.is_empty(),
        "no SQ doorbell means no pending work"
    );

    ctrl.mmio_write(REG_DOORBELL_BASE + u64::from(qid) * 8, 4, 1);
    ctrl.process_into(&mut mem, &mut completions);

    assert_eq!(
        completions,
        vec![NvmeCompletionEvent {
            cqid: qid,
            vector: 1,
        }]
    );
    assert_eq!(
        completion_status(&read_completion(&mem, high_io_cq, 0)),
        SC_SUCCESS
    );

    completions.clear();
    ctrl.process_into(&mut mem, &mut completions);
    assert!(completions.is_empty(), "drained SQ bit is cleared");
}

#[test]
fn second_namespace_is_listed_sized_and_bounds_checked() {
    let (mut ctrl, mut mem) = enabled_controller();
    // 2 MiB blank install target as NSID 2.
    let target_bytes = 2 * 1024 * 1024usize;
    ctrl.attach_second_namespace(target_bytes);
    assert!(ctrl.has_second_namespace());

    // Active namespace list (after NSID 0) reports NSID 1 then NSID 2 then 0.
    let sqe = encode_sqe(
        ADMIN_OP_IDENTIFY,
        0x10,
        0,
        DATA_BASE,
        IDENTIFY_CNS_ACTIVE_NAMESPACE_LIST,
        0,
        0,
    );
    submit_admin(&mut ctrl, &mut mem, 0, &sqe);
    assert_eq!(
        completion_status(&read_completion(&mem, ACQ_BASE, 0)),
        SC_SUCCESS
    );
    let list = mem.read_bytes(DATA_BASE, PAGE_SIZE).unwrap();
    assert_eq!(u32::from_le_bytes(list[0..4].try_into().unwrap()), NSID);
    assert_eq!(u32::from_le_bytes(list[4..8].try_into().unwrap()), NSID2);
    assert_eq!(u32::from_le_bytes(list[8..12].try_into().unwrap()), 0);

    // Identify Namespace for NSID 2 reports the target's block count.
    let sqe = encode_sqe(
        ADMIN_OP_IDENTIFY,
        0x11,
        NSID2,
        DATA_BASE,
        IDENTIFY_CNS_NAMESPACE,
        0,
        0,
    );
    submit_admin(&mut ctrl, &mut mem, 1, &sqe);
    assert_eq!(
        completion_status(&read_completion(&mem, ACQ_BASE, 1)),
        SC_SUCCESS
    );
    let ns = mem.read_bytes(DATA_BASE, PAGE_SIZE).unwrap();
    let nsze = u64::from_le_bytes(ns[0..8].try_into().unwrap());
    assert_eq!(nsze, (target_bytes / LBA_SIZE) as u64);

    // Transfer bounds are enforced per namespace: reading the first LBA past
    // the small 1 MiB NSID-1 disk fails, but that LBA is valid on the 2 MiB
    // NSID 2. `block_count_for` reflects each namespace's own size.
    assert_eq!(ctrl.block_count_for(NSID), (1 << 20) / LBA_SIZE as u64);
    assert_eq!(
        ctrl.block_count_for(NSID2),
        (target_bytes / LBA_SIZE) as u64
    );
    assert_eq!(ctrl.block_count_for(3), 0, "unallocated namespace");
    let over_ns1_lba = (1 << 20) / LBA_SIZE as u32; // first LBA past NSID 1
    let read = SubmissionEntry {
        opcode: NVM_OP_READ,
        command_id: 0,
        nsid: NSID,
        prp1: 0,
        prp2: 0,
        cdw10: over_ns1_lba,
        cdw11: 0,
        cdw12: 0, // NLB 0-based => 1 block
        cdw13: 0,
        cdw14: 0,
        cdw15: 0,
    };
    assert!(transfer_range(&read, ctrl.block_count_for(NSID) * LBA_SIZE as u64).is_none());
    assert!(transfer_range(&read, ctrl.block_count_for(NSID2) * LBA_SIZE as u64).is_some());
}

#[test]
fn async_event_request_is_accepted_and_left_pending() {
    let (mut ctrl, mut mem) = enabled_controller();
    let sqe = encode_sqe(ADMIN_OP_ASYNC_EVENT_REQUEST, 0x77, 0, 0, 0, 0, 0);
    assert!(mem.write_bytes(ASQ_BASE, &sqe));
    ctrl.mmio_write(REG_DOORBELL_BASE, 4, 1);

    assert_eq!(ctrl.process(&mut mem), Vec::<NvmeCompletionEvent>::new());
    let admin_sq = ctrl.sqs[0].as_ref().expect("admin SQ installed");
    assert_eq!(admin_sq.head, 1, "AER consumes an SQ entry");
    assert_eq!(read_completion(&mem, ACQ_BASE, 0), [0u8; 16]);
    assert_eq!(ctrl.pending_async_event_requests, 1);

    let trace = ctrl.recent_command_trace();
    assert_eq!(trace.len(), 1);
    assert_eq!(trace[0].opcode, ADMIN_OP_ASYNC_EVENT_REQUEST);
    assert_eq!(trace[0].command_id, 0x77);
    assert_eq!(trace[0].status, SC_SUCCESS);
    assert!(!trace[0].completion_posted);
    assert_eq!(trace[0].completion, None);
}

#[test]
fn nvme_reset_preserving_media_clears_controller_state() {
    // Given: both namespaces carry guest-written data and volatile controller
    // state is dirty.
    let (mut ctrl, mut mem) = enabled_controller();
    ctrl.attach_second_namespace(LBA_SIZE * 8);
    create_io_queue_pair(&mut ctrl, &mut mem, 0, CREATE_IO_CQ_PC_BIT);

    let ns1_pattern: Vec<u8> = (0..LBA_SIZE).map(|i| 0x40 | (i % 0x20) as u8).collect();
    assert!(mem.write_bytes(DATA_BASE, &ns1_pattern));
    let ns1_write = encode_sqe(NVM_OP_WRITE, 0x41, NSID, DATA_BASE, 3, 0, 0);
    assert!(mem.write_bytes(IO_SQ_BASE, &ns1_write));
    ctrl.mmio_write(REG_DOORBELL_BASE + 2 * 4, 4, 1);
    ctrl.process(&mut mem);
    assert_eq!(
        completion_status(&read_completion(&mem, IO_CQ_BASE, 0)),
        SC_SUCCESS
    );

    let ns2_pattern: Vec<u8> = (0..LBA_SIZE).map(|i| 0x80 | (i % 0x40) as u8).collect();
    ctrl.backend_for_nsid_mut(NSID2)
        .expect("NSID 2 attached")
        .write_at(0, &ns2_pattern)
        .unwrap();

    let aer = encode_sqe(ADMIN_OP_ASYNC_EVENT_REQUEST, 0x42, 0, 0, 0, 0, 0);
    assert!(mem.write_bytes(ASQ_BASE + 2 * SQ_ENTRY_SIZE, &aer));
    ctrl.mmio_write(REG_DOORBELL_BASE, 4, 3);
    ctrl.process(&mut mem);
    assert_eq!(ctrl.pending_async_event_requests, 1);
    ctrl.mmio_write(NVME_MSIX_TABLE_OFFSET.into(), 8, 0x0808_0000);
    ctrl.mmio_write(u64::from(NVME_MSIX_TABLE_OFFSET) + 8, 4, 35);
    assert_eq!(ctrl.raise_msix(0, true, false), None);
    assert!(!ctrl.recent_command_trace().is_empty());

    // When: the platform reboot path resets controller registers without
    // replacing namespace backing stores.
    ctrl.reset_registers_keep_disks();

    // Then: namespace contents survive but controller-visible volatile state
    // returns to power-on defaults.
    let ns1_start = 3 * LBA_SIZE;
    assert_eq!(
        &ctrl.disk_image()[ns1_start..ns1_start + LBA_SIZE],
        ns1_pattern.as_slice()
    );
    assert_eq!(
        ctrl.backend_for_nsid_mut(NSID2)
            .expect("NSID 2 attached")
            .read_at(0, LBA_SIZE)
            .unwrap(),
        ns2_pattern
    );
    assert_eq!(ctrl.mmio_read(REG_CC, 4), 0);
    assert_eq!(ctrl.mmio_read(REG_CSTS, 4) & u64::from(CSTS_RDY_BIT), 0);
    assert_eq!(ctrl.mmio_read(REG_AQA, 4), 0);
    assert_eq!(ctrl.mmio_read(REG_ASQ, 8), 0);
    assert_eq!(ctrl.mmio_read(REG_ACQ, 8), 0);
    assert_eq!(ctrl.sqs.len(), 1);
    assert!(ctrl.sqs[0].is_none());
    assert_eq!(ctrl.cqs.len(), 1);
    assert!(ctrl.cqs[0].is_none());
    assert_eq!(ctrl.pending_async_event_requests, 0);
    assert!(ctrl.recent_command_trace().is_empty());
    assert_eq!(
        ctrl.drain_pending_msix(true, false),
        Vec::<MsixMessage>::new()
    );
    assert!(ctrl.volatile_write_cache_enabled);
}

#[test]
fn identify_namespace_reports_512b_lba_and_capacity() {
    let (mut ctrl, mut mem) = enabled_controller();
    let sqe = encode_sqe(
        ADMIN_OP_IDENTIFY,
        1,
        NSID,
        DATA_BASE,
        IDENTIFY_CNS_NAMESPACE,
        0,
        0,
    );
    submit_admin(&mut ctrl, &mut mem, 0, &sqe);
    assert_eq!(
        completion_status(&read_completion(&mem, ACQ_BASE, 0)),
        SC_SUCCESS
    );

    let id = mem.read_bytes(DATA_BASE, PAGE_SIZE).unwrap();
    // NSZE = total logical blocks = disk size / 512.
    let nsze = u64::from_le_bytes(id[0..8].try_into().unwrap());
    assert_eq!(nsze, (1 << 20) / LBA_SIZE as u64);
    // LBAF0 LBADS (bits 23:16) = 9 ⇒ 2^9 = 512-byte LBAs.
    let lbaf0 = u32::from_le_bytes([id[128], id[129], id[130], id[131]]);
    assert_eq!((lbaf0 >> 16) & 0xff, 9);
    assert_eq!(&id[104..120], &NS_NGUID);
    assert_eq!(&id[120..128], &NS_EUI64);
}

#[test]
fn identify_active_namespace_list_reports_nsid_one_once() {
    let (mut ctrl, mut mem) = enabled_controller();
    let sqe = encode_sqe(
        ADMIN_OP_IDENTIFY,
        2,
        0,
        DATA_BASE,
        IDENTIFY_CNS_ACTIVE_NAMESPACE_LIST,
        0,
        0,
    );
    submit_admin(&mut ctrl, &mut mem, 0, &sqe);
    assert_eq!(
        completion_status(&read_completion(&mem, ACQ_BASE, 0)),
        SC_SUCCESS
    );

    let list = mem.read_bytes(DATA_BASE, PAGE_SIZE).unwrap();
    assert_eq!(u32::from_le_bytes(list[0..4].try_into().unwrap()), NSID);
    assert_eq!(
        u32::from_le_bytes(list[4..8].try_into().unwrap()),
        0,
        "namespace list must be zero-terminated"
    );

    let sqe = encode_sqe(
        ADMIN_OP_IDENTIFY,
        3,
        NSID,
        DATA_BASE + PAGE_SIZE as u64,
        IDENTIFY_CNS_ACTIVE_NAMESPACE_LIST,
        0,
        0,
    );
    submit_admin(&mut ctrl, &mut mem, 1, &sqe);
    assert_eq!(
        completion_status(&read_completion(&mem, ACQ_BASE, 1)),
        SC_SUCCESS
    );
    let empty = mem
        .read_bytes(DATA_BASE + PAGE_SIZE as u64, PAGE_SIZE)
        .unwrap();
    assert_eq!(
        u32::from_le_bytes(empty[0..4].try_into().unwrap()),
        0,
        "no active namespaces follow NSID 1"
    );
}
