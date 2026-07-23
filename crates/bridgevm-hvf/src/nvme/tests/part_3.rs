//! Split test module.

use super::super::*;
use super::helpers::*;
use crate::fwcfg::GuestMemoryMut;
use std::fs;

#[test]
fn create_io_queues_reject_qids_beyond_doorbell_aperture() {
    let (mut ctrl, mut mem) = enabled_controller();
    let invalid_qid = u32::from(MAX_IO_QUEUE_PAIRS) + 1;
    let cdw10 = (u32::from(QDEPTH - 1) << 16) | invalid_qid;

    submit_admin(
        &mut ctrl,
        &mut mem,
        0,
        &encode_sqe(
            ADMIN_OP_CREATE_IO_CQ,
            1,
            0,
            IO_CQ_BASE,
            cdw10,
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
            2,
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
            3,
            0,
            IO_SQ_BASE,
            cdw10,
            1u32 << 16,
            0,
        ),
    );
    assert_eq!(
        completion_status(&read_completion(&mem, ACQ_BASE, 2)),
        SC_INVALID_FIELD
    );
}

#[test]
fn read_uses_prp2_for_two_page_transfer() {
    let disk: Vec<u8> = (0..PAGE_SIZE * 4).map(|i| (i % 251) as u8).collect();
    let (mut ctrl, mut mem) = enabled_controller_with_disk_and_mem_len(disk.clone(), 0x10000);
    create_io_queue_pair(&mut ctrl, &mut mem, 0, CREATE_IO_CQ_PC_BIT);

    let second_page = DATA_BASE + PAGE_SIZE_U64;
    let read_cmd = encode_sqe_with_prps(
        NVM_OP_READ,
        0x50,
        NSID,
        DATA_BASE,
        second_page,
        [0, 0, 15], // 16 LBAs = 8192 bytes = PRP1 + direct PRP2 page
    );
    assert!(mem.write_bytes(IO_SQ_BASE, &read_cmd));
    ctrl.mmio_write(REG_DOORBELL_BASE + 2 * 4, 4, 1);
    ctrl.process(&mut mem);

    assert_eq!(
        mem.read_bytes(DATA_BASE, PAGE_SIZE).unwrap(),
        disk[0..PAGE_SIZE]
    );
    assert_eq!(
        mem.read_bytes(second_page, PAGE_SIZE).unwrap(),
        disk[PAGE_SIZE..PAGE_SIZE * 2]
    );
    assert_eq!(
        completion_status(&read_completion(&mem, IO_CQ_BASE, 0)),
        SC_SUCCESS
    );
}

#[test]
fn read_uses_prp_list_for_larger_transfer() {
    let pages = 6usize;
    let disk: Vec<u8> = (0..PAGE_SIZE * pages)
        .map(|i| 0x80 | ((i % 0x40) as u8))
        .collect();
    let (mut ctrl, mut mem) = enabled_controller_with_disk_and_mem_len(disk.clone(), 0x20000);
    create_io_queue_pair(&mut ctrl, &mut mem, 0, CREATE_IO_CQ_PC_BIT);

    let list_base = DATA_BASE + PAGE_SIZE_U64;
    let data0 = DATA_BASE + 2 * PAGE_SIZE_U64;
    let mut list = vec![0u8; PAGE_SIZE];
    for page in 1..pages {
        let gpa = data0 + (page as u64) * PAGE_SIZE_U64;
        let off = (page - 1) * 8;
        list[off..off + 8].copy_from_slice(&gpa.to_le_bytes());
    }
    assert!(mem.write_bytes(list_base, &list));

    let blocks = pages as u32 * (PAGE_SIZE as u32 / LBA_SIZE as u32);
    let read_cmd = encode_sqe_with_prps(
        NVM_OP_READ,
        0x51,
        NSID,
        data0,
        list_base,
        [0, 0, blocks - 1],
    );
    assert!(mem.write_bytes(IO_SQ_BASE, &read_cmd));
    ctrl.mmio_write(REG_DOORBELL_BASE + 2 * 4, 4, 1);
    ctrl.process(&mut mem);

    for page in 0..pages {
        let gpa = data0 + (page as u64) * PAGE_SIZE_U64;
        let start = page * PAGE_SIZE;
        assert_eq!(
            mem.read_bytes(gpa, PAGE_SIZE).unwrap(),
            disk[start..start + PAGE_SIZE],
            "page {page} should be populated through the PRP list"
        );
    }
    assert_eq!(
        completion_status(&read_completion(&mem, IO_CQ_BASE, 0)),
        SC_SUCCESS
    );
}

#[test]
fn read_uses_prp_list_starting_at_prp2_offset() {
    let pages = 4usize;
    let disk: Vec<u8> = (0..PAGE_SIZE * pages)
        .map(|i| 0x20 | ((i % 0x5f) as u8))
        .collect();
    let (mut ctrl, mut mem) = enabled_controller_with_disk_and_mem_len(disk.clone(), 0x20000);
    create_io_queue_pair(&mut ctrl, &mut mem, 0, CREATE_IO_CQ_PC_BIT);

    let list_base = DATA_BASE + PAGE_SIZE_U64;
    let list_ptr = list_base + 0x100;
    let data0 = DATA_BASE + 2 * PAGE_SIZE_U64;
    let mut list = vec![0u8; (pages - 1) * 8];
    for page in 1..pages {
        let gpa = data0 + (page as u64) * PAGE_SIZE_U64;
        let off = (page - 1) * 8;
        list[off..off + 8].copy_from_slice(&gpa.to_le_bytes());
    }
    assert!(mem.write_bytes(list_ptr, &list));

    let blocks = pages as u32 * (PAGE_SIZE as u32 / LBA_SIZE as u32);
    let read_cmd =
        encode_sqe_with_prps(NVM_OP_READ, 0x53, NSID, data0, list_ptr, [0, 0, blocks - 1]);
    assert!(mem.write_bytes(IO_SQ_BASE, &read_cmd));
    ctrl.mmio_write(REG_DOORBELL_BASE + 2 * 4, 4, 1);
    ctrl.process(&mut mem);

    for page in 0..pages {
        let gpa = data0 + (page as u64) * PAGE_SIZE_U64;
        let start = page * PAGE_SIZE;
        assert_eq!(
            mem.read_bytes(gpa, PAGE_SIZE).unwrap(),
            disk[start..start + PAGE_SIZE],
            "page {page} should be populated through the offset PRP list"
        );
    }
    assert_eq!(
        completion_status(&read_completion(&mem, IO_CQ_BASE, 0)),
        SC_SUCCESS
    );
}

#[test]
fn write_uses_prp_list_for_larger_transfer() {
    let pages = 4usize;
    let (mut ctrl, mut mem) =
        enabled_controller_with_disk_and_mem_len(vec![0u8; PAGE_SIZE * pages], 0x18000);
    create_io_queue_pair(&mut ctrl, &mut mem, 0, CREATE_IO_CQ_PC_BIT);

    let list_base = DATA_BASE + PAGE_SIZE_U64;
    let data0 = DATA_BASE + 2 * PAGE_SIZE_U64;
    let replacement: Vec<u8> = (0..PAGE_SIZE * pages)
        .map(|i| 0x40 | ((i % 0x20) as u8))
        .collect();
    assert!(mem.write_bytes(data0, &replacement[0..PAGE_SIZE]));

    let mut list = vec![0u8; PAGE_SIZE];
    for page in 1..pages {
        let gpa = data0 + (page as u64) * PAGE_SIZE_U64;
        assert!(mem.write_bytes(gpa, &replacement[page * PAGE_SIZE..(page + 1) * PAGE_SIZE]));
        let off = (page - 1) * 8;
        list[off..off + 8].copy_from_slice(&gpa.to_le_bytes());
    }
    assert!(mem.write_bytes(list_base, &list));

    let blocks = pages as u32 * (PAGE_SIZE as u32 / LBA_SIZE as u32);
    let write_cmd = encode_sqe_with_prps(
        NVM_OP_WRITE,
        0x52,
        NSID,
        data0,
        list_base,
        [0, 0, blocks - 1],
    );
    assert!(mem.write_bytes(IO_SQ_BASE, &write_cmd));
    ctrl.mmio_write(REG_DOORBELL_BASE + 2 * 4, 4, 1);
    ctrl.process(&mut mem);

    assert_eq!(
        &ctrl.disk_image()[0..PAGE_SIZE * pages],
        replacement.as_slice()
    );
    assert_eq!(
        completion_status(&read_completion(&mem, IO_CQ_BASE, 0)),
        SC_SUCCESS
    );
}

#[test]
fn read_out_of_range_lba_fails() {
    let (mut ctrl, mut mem) = enabled_controller();
    // Create I/O CQ + SQ (QID 1) as above.
    let cdw10 = (u32::from(QDEPTH - 1) << 16) | 1;
    submit_admin(
        &mut ctrl,
        &mut mem,
        0,
        &encode_sqe(
            ADMIN_OP_CREATE_IO_CQ,
            1,
            0,
            IO_CQ_BASE,
            cdw10,
            CREATE_IO_CQ_PC_BIT,
            0,
        ),
    );
    submit_admin(
        &mut ctrl,
        &mut mem,
        1,
        &encode_sqe(ADMIN_OP_CREATE_IO_SQ, 2, 0, IO_SQ_BASE, cdw10, 1 << 16, 0),
    );
    // Read a block far past the end of the 1 MiB disk.
    let bad_lba = 1u64 << 40;
    let read_cmd = encode_sqe(
        NVM_OP_READ,
        0x22,
        NSID,
        DATA_BASE,
        bad_lba as u32,
        (bad_lba >> 32) as u32,
        0,
    );
    assert!(mem.write_bytes(IO_SQ_BASE, &read_cmd));
    ctrl.mmio_write(REG_DOORBELL_BASE + 2 * 4, 4, 1);
    ctrl.process(&mut mem);
    let cqe = read_completion(&mem, IO_CQ_BASE, 0);
    assert_eq!(completion_status(&cqe), SC_INVALID_FIELD);
}

#[test]
fn unknown_admin_opcode_reports_invalid_opcode() {
    let (mut ctrl, mut mem) = enabled_controller();
    let sqe = encode_sqe(0xfe, 0x99, 0, DATA_BASE, 0, 0, 0);
    submit_admin(&mut ctrl, &mut mem, 0, &sqe);
    let cqe = read_completion(&mem, ACQ_BASE, 0);
    assert_eq!(completion_status(&cqe), SC_INVALID_OPCODE);
    // Completion still references the submitting command id.
    assert_eq!(u16::from_le_bytes([cqe[12], cqe[13]]), 0x99);
}

#[test]
fn read_into_matches_read_bytes_and_rejects_unbacked() {
    let mut mem = FakeMem::new(MEM_BASE, 0x2000);
    let pattern: Vec<u8> = (0..0x400u32).map(|i| (i * 7) as u8).collect();
    assert!(mem.write_bytes(MEM_BASE + 0x100, &pattern));

    // Zero-copy fill matches the allocating accessor byte-for-byte.
    let mut dst = vec![0u8; pattern.len()];
    assert!(mem.read_into(MEM_BASE + 0x100, &mut dst));
    assert_eq!(
        dst,
        mem.read_bytes(MEM_BASE + 0x100, pattern.len()).unwrap()
    );
    assert_eq!(dst, pattern);

    // The default trait implementation (routed through read_bytes) agrees.
    let mut via_default = vec![0u8; pattern.len()];
    assert!(
        GuestMemoryMut::read_bytes(&mem, MEM_BASE + 0x100, pattern.len())
            .map(|bytes| via_default.copy_from_slice(&bytes))
            .is_some()
    );
    assert_eq!(via_default, pattern);

    // Out-of-range spans are rejected, not truncated.
    let mut oob = vec![0u8; 0x10];
    assert!(!mem.read_into(MEM_BASE + 0x1ff8, &mut oob));
}

#[test]
fn scatter_read_buffered_is_byte_identical_to_disk() {
    let (gathered, disk) = scatter_read_gathered(5, false);
    assert_eq!(
        gathered, disk,
        "buffered scatter read must reproduce the disk exactly"
    );
}

#[test]
fn scatter_read_direct_dma_is_byte_identical_to_disk() {
    let (gathered, disk) = scatter_read_gathered(5, true);
    assert_eq!(
        gathered, disk,
        "direct-DMA scatter read must reproduce the disk exactly"
    );
}

#[test]
fn scatter_read_direct_and_buffered_agree() {
    let (buffered, _) = scatter_read_gathered(6, false);
    let (direct, _) = scatter_read_gathered(6, true);
    assert_eq!(
        buffered, direct,
        "direct DMA and buffered fallback must be byte-identical"
    );
}

#[test]
fn forced_buffered_io_bypasses_an_available_host_pointer() {
    let disk: Vec<u8> = (0..PAGE_SIZE)
        .map(|i| (i.wrapping_mul(13).wrapping_add(5)) as u8)
        .collect();
    let mut ctrl = NvmeController::with_disk_image(disk.clone());
    ctrl.set_direct_dma_enabled(false);
    let mut mem = FakeMem::new(MEM_BASE, 0x10000);
    mem.enable_host_ptr();

    let blocks = PAGE_SIZE as u32 / LBA_SIZE as u32;
    let read = SubmissionEntry::from_bytes(&encode_sqe(
        NVM_OP_READ,
        0x6f,
        NSID,
        DATA_BASE,
        0,
        0,
        blocks - 1,
    ));
    assert_eq!(ctrl.io_read(&read, &mut mem), SC_SUCCESS);
    assert_eq!(mem.read_bytes(DATA_BASE, PAGE_SIZE).unwrap(), disk);
    assert!(
        ctrl.io_scratch.len() >= PAGE_SIZE,
        "the forced-buffered path must populate reusable staging storage"
    );

    ctrl.reset_registers_keep_disks();
    assert!(!ctrl.direct_dma_enabled());
    ctrl.load_disk_image(vec![0u8; PAGE_SIZE]);
    assert!(!ctrl.direct_dma_enabled());
}

#[test]
fn io_read_write_reuses_prp_span_and_segment_scratch() {
    let pages = 3usize;
    let disk: Vec<u8> = (0..PAGE_SIZE * pages)
        .map(|i| (i.wrapping_mul(17).wrapping_add(3)) as u8)
        .collect();
    let mut ctrl = NvmeController::with_disk_image(disk.clone());
    let mut mem = FakeMem::new(MEM_BASE, 0x40000);
    let data_gpas = [
        DATA_BASE + 0x2000,
        DATA_BASE + 0x2000 + 2 * PAGE_SIZE_U64,
        DATA_BASE + 0x2000 + 4 * PAGE_SIZE_U64,
    ];
    let list_base = DATA_BASE;
    let mut list = [0u8; 16];
    list[0..8].copy_from_slice(&data_gpas[1].to_le_bytes());
    list[8..16].copy_from_slice(&data_gpas[2].to_le_bytes());
    assert!(mem.write_bytes(list_base, &list));

    let blocks = pages as u32 * (PAGE_SIZE as u32 / LBA_SIZE as u32);
    let read_cmd = encode_sqe_with_prps(
        NVM_OP_READ,
        0x70,
        NSID,
        data_gpas[0],
        list_base,
        [0, 0, blocks - 1],
    );
    let read_cmd = SubmissionEntry::from_bytes(&read_cmd);
    assert_eq!(ctrl.io_read(&read_cmd, &mut mem), SC_SUCCESS);
    assert!(ctrl.prp_spans_scratch.is_empty());
    assert!(ctrl.io_segments_scratch.is_empty());
    assert!(ctrl.prp_spans_scratch.capacity() >= pages);
    assert!(ctrl.io_segments_scratch.capacity() >= pages);
    let span_scratch = (
        ctrl.prp_spans_scratch.as_ptr(),
        ctrl.prp_spans_scratch.capacity(),
    );
    let segment_scratch = (
        ctrl.io_segments_scratch.as_ptr(),
        ctrl.io_segments_scratch.capacity(),
    );

    let mut gathered = Vec::with_capacity(PAGE_SIZE * pages);
    for &gpa in &data_gpas {
        gathered.extend_from_slice(&mem.read_bytes(gpa, PAGE_SIZE).unwrap());
    }
    assert_eq!(gathered, disk);

    let mut expected = Vec::with_capacity(PAGE_SIZE * pages);
    for (page, &gpa) in data_gpas.iter().enumerate() {
        let chunk: Vec<u8> = (0..PAGE_SIZE)
            .map(|i| (0x80 | (page as u8 & 0x0f)).wrapping_add((i % 0x20) as u8))
            .collect();
        assert!(mem.write_bytes(gpa, &chunk));
        expected.extend_from_slice(&chunk);
    }
    let write_cmd = encode_sqe_with_prps(
        NVM_OP_WRITE,
        0x71,
        NSID,
        data_gpas[0],
        list_base,
        [0, 0, blocks - 1],
    );
    let write_cmd = SubmissionEntry::from_bytes(&write_cmd);
    assert_eq!(ctrl.io_write(&write_cmd, &mut mem), SC_SUCCESS);
    assert!(ctrl.prp_spans_scratch.is_empty());
    assert!(ctrl.io_segments_scratch.is_empty());
    assert_eq!(
        (
            ctrl.prp_spans_scratch.as_ptr(),
            ctrl.prp_spans_scratch.capacity()
        ),
        span_scratch
    );
    assert_eq!(
        (
            ctrl.io_segments_scratch.as_ptr(),
            ctrl.io_segments_scratch.capacity()
        ),
        segment_scratch
    );
    assert_eq!(&ctrl.disk_image()[..PAGE_SIZE * pages], expected.as_slice());
}

#[test]
fn scatter_write_buffered_is_byte_identical() {
    let (disk, expected) = scatter_write_result(5, false);
    assert_eq!(
        disk, expected,
        "buffered scatter write must land byte-identical"
    );
}

#[test]
fn scatter_write_direct_dma_is_byte_identical() {
    let (disk, expected) = scatter_write_result(5, true);
    assert_eq!(
        disk, expected,
        "direct-DMA scatter write must land byte-identical"
    );
}

#[test]
fn single_sector_read_write_direct_dma_roundtrips() {
    let (mut ctrl, mut mem) =
        enabled_controller_with_disk_and_mem_len(vec![0u8; LBA_SIZE * 8], 0x10000);
    mem.enable_host_ptr();
    create_io_queue_pair(&mut ctrl, &mut mem, 0, CREATE_IO_CQ_PC_BIT);

    let payload: Vec<u8> = (0..LBA_SIZE).map(|i| 0xc0 | (i % 0x20) as u8).collect();
    assert!(mem.write_bytes(DATA_BASE, &payload));
    let write = encode_sqe(NVM_OP_WRITE, 0x62, NSID, DATA_BASE, 2, 0, 0);
    assert!(mem.write_bytes(IO_SQ_BASE, &write));
    ctrl.mmio_write(REG_DOORBELL_BASE + 2 * 4, 4, 1);
    ctrl.process(&mut mem);

    let read_gpa = DATA_BASE + PAGE_SIZE_U64;
    let read = encode_sqe(NVM_OP_READ, 0x63, NSID, read_gpa, 2, 0, 0);
    assert!(mem.write_bytes(IO_SQ_BASE + SQ_ENTRY_SIZE, &read));
    ctrl.mmio_write(REG_DOORBELL_BASE + 2 * 4, 4, 2);
    ctrl.process(&mut mem);
    assert_eq!(mem.read_bytes(read_gpa, LBA_SIZE).unwrap(), payload);
}

#[test]
fn read_crossing_prp_list_page_boundary_reproduces_disk() {
    // A tiny PRP list page (offset near end of a page => 2 slots) forces the
    // list to chain into a second list page mid-transfer.
    let pages = 4usize;
    let disk: Vec<u8> = (0..PAGE_SIZE * pages).map(|i| (i % 0xf1) as u8).collect();
    let (mut ctrl, mut mem) = enabled_controller_with_disk_and_mem_len(disk.clone(), 0x40000);
    create_io_queue_pair(&mut ctrl, &mut mem, 0, CREATE_IO_CQ_PC_BIT);

    let data0 = DATA_BASE + 0x8000;
    let data1 = data0 + PAGE_SIZE_U64;
    let data2 = data1 + PAGE_SIZE_U64;
    let data3 = data2 + PAGE_SIZE_U64;
    // list A: 2 slots (16 bytes) at the tail of its page.
    let list_a = DATA_BASE + (PAGE_SIZE_U64 - 16);
    // list B: page-aligned second list page.
    let list_b = DATA_BASE + PAGE_SIZE_U64;
    // list A: [data1, ->list_b]; list B: [data2, data3].
    let mut a = [0u8; 16];
    a[0..8].copy_from_slice(&data1.to_le_bytes());
    a[8..16].copy_from_slice(&list_b.to_le_bytes());
    assert!(mem.write_bytes(list_a, &a));
    let mut b = [0u8; 16];
    b[0..8].copy_from_slice(&data2.to_le_bytes());
    b[8..16].copy_from_slice(&data3.to_le_bytes());
    assert!(mem.write_bytes(list_b, &b));

    let blocks = pages as u32 * (PAGE_SIZE as u32 / LBA_SIZE as u32);
    let read_cmd = encode_sqe_with_prps(NVM_OP_READ, 0x64, NSID, data0, list_a, [0, 0, blocks - 1]);
    assert!(mem.write_bytes(IO_SQ_BASE, &read_cmd));
    ctrl.mmio_write(REG_DOORBELL_BASE + 2 * 4, 4, 1);
    ctrl.process(&mut mem);
    assert_eq!(
        completion_status(&read_completion(&mem, IO_CQ_BASE, 0)),
        SC_SUCCESS
    );
    for (page, &g) in [data0, data1, data2, data3].iter().enumerate() {
        let s = page * PAGE_SIZE;
        assert_eq!(
            mem.read_bytes(g, PAGE_SIZE).unwrap(),
            disk[s..s + PAGE_SIZE],
            "page {page} across the chained PRP list"
        );
    }
}

#[test]
fn transfer_crossing_namespace_end_is_rejected_but_last_sector_succeeds() {
    let sectors = 8usize;
    let (mut ctrl, mut mem) =
        enabled_controller_with_disk_and_mem_len(vec![0u8; LBA_SIZE * sectors], 0x10000);
    create_io_queue_pair(&mut ctrl, &mut mem, 0, CREATE_IO_CQ_PC_BIT);

    // Reading the exact last sector is in range.
    let last = (sectors - 1) as u32;
    let ok = encode_sqe(NVM_OP_READ, 0x65, NSID, DATA_BASE, last, 0, 0);
    assert!(mem.write_bytes(IO_SQ_BASE, &ok));
    ctrl.mmio_write(REG_DOORBELL_BASE + 2 * 4, 4, 1);
    ctrl.process(&mut mem);
    assert_eq!(
        completion_status(&read_completion(&mem, IO_CQ_BASE, 0)),
        SC_SUCCESS
    );

    // Two sectors starting at the last sector runs one sector past the end.
    let over = encode_sqe(NVM_OP_READ, 0x66, NSID, DATA_BASE, last, 0, 1);
    assert!(mem.write_bytes(IO_SQ_BASE + SQ_ENTRY_SIZE, &over));
    ctrl.mmio_write(REG_DOORBELL_BASE + 2 * 4, 4, 2);
    ctrl.process(&mut mem);
    assert_eq!(
        completion_status(&read_completion(&mem, IO_CQ_BASE, 1)),
        SC_INVALID_FIELD
    );

    // Writes past the end are likewise rejected.
    assert!(mem.write_bytes(DATA_BASE, &vec![0xffu8; LBA_SIZE * 2]));
    let over_w = encode_sqe(NVM_OP_WRITE, 0x67, NSID, DATA_BASE, last, 0, 1);
    assert!(mem.write_bytes(IO_SQ_BASE + 2 * SQ_ENTRY_SIZE, &over_w));
    ctrl.mmio_write(REG_DOORBELL_BASE + 2 * 4, 4, 3);
    ctrl.process(&mut mem);
    assert_eq!(
        completion_status(&read_completion(&mem, IO_CQ_BASE, 2)),
        SC_INVALID_FIELD
    );
}

#[test]
fn write_back_persists_to_source_file_synchronously_without_flush() {
    let source = temp_path("dma-writeback-sync");
    fs::write(&source, vec![0u8; LBA_SIZE * 64]).unwrap();
    let (mut ctrl, mut mem) = enabled_controller_with_raw_file(&source, true, 0x20000);
    create_io_queue_pair(&mut ctrl, &mut mem, 0, CREATE_IO_CQ_PC_BIT);

    // Two contiguous pages (coalesce to a single pwrite of 8 KiB).
    let payload: Vec<u8> = (0..PAGE_SIZE * 2)
        .map(|i| 0xa0u8.wrapping_add((i % 0x33) as u8))
        .collect();
    assert!(mem.write_bytes(DATA_BASE, &payload));
    let slba = 4u64;
    let blocks = (PAGE_SIZE * 2 / LBA_SIZE) as u32;
    let write = encode_sqe_with_prps(
        NVM_OP_WRITE,
        0x71,
        NSID,
        DATA_BASE,
        DATA_BASE + PAGE_SIZE_U64,
        [slba as u32, (slba >> 32) as u32, blocks - 1],
    );
    assert!(mem.write_bytes(IO_SQ_BASE, &write));
    ctrl.mmio_write(REG_DOORBELL_BASE + 2 * 4, 4, 1);
    ctrl.process(&mut mem);
    assert_eq!(
        completion_status(&read_completion(&mem, IO_CQ_BASE, 0)),
        SC_SUCCESS
    );

    // Read the source through an independent handle WITHOUT flushing first:
    // the write-through contract requires bytes to already be on disk.
    let start = slba as usize * LBA_SIZE;
    let on_disk = fs::read(&source).unwrap();
    assert_eq!(
        &on_disk[start..start + payload.len()],
        payload.as_slice(),
        "write-back must reach the host file synchronously, before any flush"
    );
    fs::remove_file(source).ok();
}

#[test]
fn overlay_write_merges_into_coalesced_read() {
    let source = temp_path("dma-overlay-merge");
    let sectors = 32usize;
    let base: Vec<u8> = (0..LBA_SIZE * sectors).map(|i| (i % 253) as u8).collect();
    fs::write(&source, &base).unwrap();
    let (mut ctrl, mut mem) = enabled_controller_with_raw_file(&source, false, 0x20000);
    create_io_queue_pair(&mut ctrl, &mut mem, 0, CREATE_IO_CQ_PC_BIT);

    // Overwrite sector 5 through the sparse overlay (read-only backend).
    let repl: Vec<u8> = (0..LBA_SIZE).map(|i| 0xf0 | (i % 0x0f) as u8).collect();
    assert!(mem.write_bytes(DATA_BASE, &repl));
    let w = encode_sqe(NVM_OP_WRITE, 0x81, NSID, DATA_BASE, 5, 0, 0);
    assert!(mem.write_bytes(IO_SQ_BASE, &w));
    ctrl.mmio_write(REG_DOORBELL_BASE + 2 * 4, 4, 1);
    ctrl.process(&mut mem);

    // Coalesced 2-page read over sectors 0..16 must reflect the overlay at 5.
    let read_gpa = DATA_BASE + 0x4000;
    let blocks = (PAGE_SIZE * 2 / LBA_SIZE) as u32;
    let read = encode_sqe_with_prps(
        NVM_OP_READ,
        0x82,
        NSID,
        read_gpa,
        read_gpa + PAGE_SIZE_U64,
        [0, 0, blocks - 1],
    );
    assert!(mem.write_bytes(IO_SQ_BASE + SQ_ENTRY_SIZE, &read));
    ctrl.mmio_write(REG_DOORBELL_BASE + 2 * 4, 4, 2);
    ctrl.process(&mut mem);
    assert_eq!(
        completion_status(&read_completion(&mem, IO_CQ_BASE, 1)),
        SC_SUCCESS
    );

    let mut expected = base[0..PAGE_SIZE * 2].to_vec();
    expected[5 * LBA_SIZE..6 * LBA_SIZE].copy_from_slice(&repl);
    assert_eq!(
        mem.read_bytes(read_gpa, PAGE_SIZE * 2).unwrap(),
        expected,
        "coalesced read must merge the sparse overlay over the whole span"
    );
    fs::remove_file(source).ok();
}

#[test]
fn overlay_chunk_starting_before_read_offset_merges_into_partial_read() {
    let source = temp_path("dma-overlay-partial-read");
    let sectors = 16usize;
    let base: Vec<u8> = (0..LBA_SIZE * sectors)
        .map(|i| 0x10u8.wrapping_add((i % 0x6d) as u8))
        .collect();
    fs::write(&source, &base).unwrap();
    let mut backend = DiskBackend::raw_file(&source, false).unwrap();
    let offset = (LBA_SIZE * 5) as u64;
    let replacement: Vec<u8> = (0..LBA_SIZE)
        .map(|i| 0xc0u8.wrapping_add((i % 0x21) as u8))
        .collect();

    backend.write_at(offset, &replacement).unwrap();

    let mut readback = vec![0u8; LBA_SIZE];
    backend.read_at_into(offset, &mut readback).unwrap();

    assert_eq!(
        readback, replacement,
        "overlay chunk base is before the read offset and must still merge"
    );
    fs::remove_file(source).ok();
}

#[test]
#[ignore = "micro-benchmark; run with `--ignored --nocapture`"]
fn bench_dma_disk_read_coalescing() {
    let path = temp_path("dma-bench");
    let total = 4 * 1024 * 1024usize; // 4 MiB per transfer
    fs::write(&path, vec![0x5au8; total]).unwrap();
    let iters = 200usize;
    let mut backend = DiskBackend::raw_file(&path, false).unwrap();
    let mut dst = vec![0u8; total];

    // Old shape: one allocation + one pread + one copy per 4 KiB PRP page.
    let t0 = std::time::Instant::now();
    for _ in 0..iters {
        let mut off = 0u64;
        while (off as usize) < total {
            let page = backend.read_at(off, PAGE_SIZE).unwrap();
            let s = off as usize;
            dst[s..s + PAGE_SIZE].copy_from_slice(&page);
            off += PAGE_SIZE_U64;
        }
    }
    let old = t0.elapsed();

    // New shape: one coalesced pread into the destination, no allocations.
    let t1 = std::time::Instant::now();
    for _ in 0..iters {
        backend.read_at_into(0, &mut dst).unwrap();
    }
    let new = t1.elapsed();

    let mb = (total * iters) as f64 / (1024.0 * 1024.0);
    eprintln!(
        "nvme dma read {total_kib} KiB/xfer: old per-page {old_mbps:.0} MB/s ({pages} allocs+syscalls/xfer) -> new coalesced {new_mbps:.0} MB/s (1 syscall/xfer), speedup {ratio:.2}x",
        total_kib = total / 1024,
        old_mbps = mb / old.as_secs_f64(),
        pages = total / PAGE_SIZE,
        new_mbps = mb / new.as_secs_f64(),
        ratio = old.as_secs_f64() / new.as_secs_f64(),
    );
    fs::remove_file(path).ok();
}
