//! Split test module.

use super::super::*;
use super::helpers::*;
use crate::fwcfg::GuestMemoryMut;
use crate::pcie::NVME_MSIX_VECTOR_COUNT;
use std::fs;
use std::io;

#[test]
fn identify_namespace_descriptor_list_reports_stable_identifiers() {
    let (mut ctrl, mut mem) = enabled_controller();
    let sqe = encode_sqe(
        ADMIN_OP_IDENTIFY,
        4,
        NSID,
        DATA_BASE,
        IDENTIFY_CNS_NAMESPACE_DESCRIPTOR_LIST,
        0,
        0,
    );
    submit_admin(&mut ctrl, &mut mem, 0, &sqe);
    assert_eq!(
        completion_status(&read_completion(&mem, ACQ_BASE, 0)),
        SC_SUCCESS
    );

    let desc = mem.read_bytes(DATA_BASE, PAGE_SIZE).unwrap();
    assert_eq!(desc[0], 0x03, "first descriptor is UUID");
    assert_eq!(desc[1], 16, "UUID descriptor length");
    assert_eq!(
        &desc[4..20],
        &NS_UUID,
        "UUID descriptor carries the stable namespace UUID"
    );
    assert_eq!(desc[20], 0x02, "second descriptor is NGUID");
    assert_eq!(desc[21], 16, "NGUID descriptor length");
    assert_eq!(&desc[24..40], &NS_NGUID);
    assert_eq!(desc[40], 0x01, "third descriptor is EUI64");
    assert_eq!(desc[41], 8, "EUI64 descriptor length");
    assert_eq!(&desc[44..52], &NS_EUI64);
    assert_eq!(desc[52], 0, "zero descriptor length terminates the list");
}

#[test]
fn get_log_page_smart_health_completes() {
    let (mut ctrl, mut mem) = enabled_controller();
    let numd = (512u32 / 4) - 1;
    let cdw10 = (numd << 16) | u32::from(LOG_PAGE_SMART_HEALTH);
    let sqe = encode_sqe(
        ADMIN_OP_GET_LOG_PAGE,
        5,
        0xffff_ffff,
        DATA_BASE,
        cdw10,
        0,
        0,
    );
    submit_admin(&mut ctrl, &mut mem, 0, &sqe);
    assert_eq!(
        completion_status(&read_completion(&mem, ACQ_BASE, 0)),
        SC_SUCCESS
    );

    let smart = mem.read_bytes(DATA_BASE, 512).unwrap();
    assert_eq!(smart[0], 0, "no critical warning bits set");
    assert_eq!(
        u16::from_le_bytes([smart[1], smart[2]]),
        300,
        "composite temperature is reported in Kelvin"
    );
    assert_eq!(smart[3], 100, "available spare percentage");
}

#[test]
fn get_log_page_firmware_slot_info_completes() {
    let (mut ctrl, mut mem) = enabled_controller();
    let numd = (512u32 / 4) - 1;
    let cdw10 = (numd << 16) | u32::from(LOG_PAGE_FIRMWARE_SLOT_INFO);
    let sqe = encode_sqe(
        ADMIN_OP_GET_LOG_PAGE,
        6,
        0xffff_ffff,
        DATA_BASE,
        cdw10,
        0,
        0,
    );
    submit_admin(&mut ctrl, &mut mem, 0, &sqe);
    assert_eq!(
        completion_status(&read_completion(&mem, ACQ_BASE, 0)),
        SC_SUCCESS
    );

    let log = mem.read_bytes(DATA_BASE, 512).unwrap();
    assert_eq!(log[0] & 0x7, 1, "active firmware slot is slot 1");
    assert!(
        log[8..72].starts_with(b"BridgeVM NVMe firmware slot 1"),
        "firmware revision slot string is present"
    );
}

#[test]
fn get_log_page_command_effects_completes_with_supported_commands() {
    let (mut ctrl, mut mem) = enabled_controller();
    let numd = (PAGE_SIZE as u32 / 4) - 1;
    let cdw10 = (numd << 16) | u32::from(LOG_PAGE_COMMAND_EFFECTS);
    let sqe = encode_sqe(
        ADMIN_OP_GET_LOG_PAGE,
        7,
        0xffff_ffff,
        DATA_BASE,
        cdw10,
        0,
        0,
    );
    submit_admin(&mut ctrl, &mut mem, 0, &sqe);
    assert_eq!(
        completion_status(&read_completion(&mem, ACQ_BASE, 0)),
        SC_SUCCESS
    );

    let log = mem.read_bytes(DATA_BASE, PAGE_SIZE).unwrap();
    let effect_at = |base: usize, opcode: u8| {
        let off = base + usize::from(opcode) * 4;
        u32::from_le_bytes(log[off..off + 4].try_into().unwrap())
    };
    assert_eq!(effect_at(0, ADMIN_OP_GET_LOG_PAGE), CMD_EFFECT_CSUPP);
    assert_eq!(effect_at(0, ADMIN_OP_IDENTIFY), CMD_EFFECT_CSUPP);
    assert_eq!(effect_at(0, ADMIN_OP_GET_FEATURES), CMD_EFFECT_CSUPP);
    assert_eq!(effect_at(0, ADMIN_OP_SECURITY_SEND), CMD_EFFECT_CSUPP);
    assert_eq!(effect_at(0, ADMIN_OP_SECURITY_RECV), CMD_EFFECT_CSUPP);
    assert_eq!(
        effect_at(1024, NVM_OP_FLUSH),
        CMD_EFFECT_CSUPP | CMD_EFFECT_LBCC
    );
    assert_eq!(
        effect_at(1024, NVM_OP_WRITE),
        CMD_EFFECT_CSUPP | CMD_EFFECT_LBCC
    );
    assert_eq!(effect_at(1024, NVM_OP_READ), CMD_EFFECT_CSUPP);
}

#[test]
fn get_log_page_vendor_logs_match_qemu_invalid_field_dnr() {
    let (mut ctrl, mut mem) = enabled_controller();
    let numd = (512u32 / 4) - 1;
    for (slot, lid) in [0xc0u8, 0xc1].into_iter().enumerate() {
        let sqe = encode_sqe(
            ADMIN_OP_GET_LOG_PAGE,
            0x80 + slot as u16,
            0xffff_ffff,
            DATA_BASE + slot as u64 * PAGE_SIZE_U64,
            (numd << 16) | u32::from(lid),
            0,
            0,
        );
        submit_admin(&mut ctrl, &mut mem, slot as u16, &sqe);
        assert_eq!(
            completion_status(&read_completion(&mem, ACQ_BASE, slot as u16)),
            SC_INVALID_FIELD_DNR,
            "vendor log page {lid:#x} matches QEMU's unsupported default with DNR"
        );
    }
}

#[test]
fn security_receive_protocol_info_matches_qemu_no_spdm_default() {
    let (mut ctrl, mut mem) = enabled_controller();
    let cdw10 = u32::from(SECURITY_PROTOCOL_INFORMATION) << 24;
    let sqe = encode_sqe(
        ADMIN_OP_SECURITY_RECV,
        0x90,
        0,
        DATA_BASE,
        cdw10,
        SECURITY_PROTOCOL_INFO_LIST_LEN as u32,
        0,
    );
    submit_admin(&mut ctrl, &mut mem, 0, &sqe);
    assert_eq!(
        completion_status(&read_completion(&mem, ACQ_BASE, 0)),
        SC_SUCCESS
    );

    assert_eq!(
        mem.read_bytes(DATA_BASE, SECURITY_PROTOCOL_INFO_LIST_LEN)
            .unwrap(),
        vec![0, 0, 0, 0, 0, 0, 0, 2, SECURITY_PROTOCOL_INFORMATION, 0,]
    );
}

#[test]
fn security_receive_rejects_short_or_unsupported_requests() {
    let (mut ctrl, mut mem) = enabled_controller();
    let cases = [
        (
            (u32::from(SECURITY_PROTOCOL_INFORMATION) << 24),
            (SECURITY_PROTOCOL_INFO_LIST_LEN - 1) as u32,
        ),
        (
            (u32::from(SECURITY_PROTOCOL_INFORMATION) << 24) | (1 << 8),
            SECURITY_PROTOCOL_INFO_LIST_LEN as u32,
        ),
        (
            u32::from(SECURITY_PROTOCOL_DMTF_SPDM) << 24,
            SECURITY_PROTOCOL_INFO_LIST_LEN as u32,
        ),
    ];
    for (slot, (cdw10, cdw11)) in cases.into_iter().enumerate() {
        let sqe = encode_sqe(
            ADMIN_OP_SECURITY_RECV,
            0x91 + slot as u16,
            0,
            DATA_BASE,
            cdw10,
            cdw11,
            0,
        );
        submit_admin(&mut ctrl, &mut mem, slot as u16, &sqe);
        assert_eq!(
            completion_status(&read_completion(&mem, ACQ_BASE, slot as u16)),
            SC_INVALID_FIELD_DNR
        );
    }
}

#[test]
fn security_send_reports_invalid_field_without_spdm_socket() {
    let (mut ctrl, mut mem) = enabled_controller();
    let sqe = encode_sqe(
        ADMIN_OP_SECURITY_SEND,
        0x94,
        0,
        DATA_BASE,
        u32::from(SECURITY_PROTOCOL_DMTF_SPDM) << 24,
        0,
        0,
    );
    submit_admin(&mut ctrl, &mut mem, 0, &sqe);
    assert_eq!(
        completion_status(&read_completion(&mem, ACQ_BASE, 0)),
        SC_INVALID_FIELD_DNR
    );
}

#[test]
fn set_features_number_of_queues_completes() {
    let (mut ctrl, mut mem) = enabled_controller();
    // Request more queues than we have; controller grants what it can.
    let cdw11 = (3u32 << 16) | 3; // NCQR=3, NSQR=3 (0-based)
    let sqe = encode_sqe(
        ADMIN_OP_SET_FEATURES,
        7,
        0,
        0,
        u32::from(FEATURE_NUMBER_OF_QUEUES),
        cdw11,
        0,
    );
    submit_admin(&mut ctrl, &mut mem, 0, &sqe);
    let cqe = read_completion(&mem, ACQ_BASE, 0);
    assert_eq!(completion_status(&cqe), SC_SUCCESS);
}

#[test]
fn get_features_number_of_queues_reports_capacity_in_completion_dw0() {
    let (mut ctrl, mut mem) = enabled_controller();
    let sqe = encode_sqe(
        ADMIN_OP_GET_FEATURES,
        8,
        0,
        0,
        u32::from(FEATURE_NUMBER_OF_QUEUES),
        0,
        0,
    );
    submit_admin(&mut ctrl, &mut mem, 0, &sqe);
    let cqe = read_completion(&mem, ACQ_BASE, 0);
    assert_eq!(completion_status(&cqe), SC_SUCCESS);
    let granted = u32::from(MAX_IO_QUEUE_PAIRS - 1);
    assert_eq!(completion_dw0(&cqe), (granted << 16) | granted);
}

#[test]
fn get_features_volatile_write_cache_matches_qemu_default() {
    let (mut ctrl, mut mem) = enabled_controller();
    let current = encode_sqe(
        ADMIN_OP_GET_FEATURES,
        0x70,
        0,
        0,
        u32::from(FEATURE_VOLATILE_WRITE_CACHE),
        0,
        0,
    );
    submit_admin(&mut ctrl, &mut mem, 0, &current);
    let cqe = read_completion(&mem, ACQ_BASE, 0);
    assert_eq!(completion_status(&cqe), SC_SUCCESS);
    assert_eq!(
        completion_dw0(&cqe),
        1,
        "QEMU reports volatile write cache enabled by default"
    );

    let caps = encode_sqe(
        ADMIN_OP_GET_FEATURES,
        0x71,
        0,
        0,
        u32::from(FEATURE_VOLATILE_WRITE_CACHE)
            | (GET_FEATURE_SELECT_CAPABILITIES << GET_FEATURE_SELECT_SHIFT),
        0,
        0,
    );
    submit_admin(&mut ctrl, &mut mem, 1, &caps);
    let cqe = read_completion(&mem, ACQ_BASE, 1);
    assert_eq!(completion_status(&cqe), SC_SUCCESS);
    assert_eq!(
        completion_dw0(&cqe),
        FEATURE_CAP_CHANGEABLE,
        "QEMU reports VWC as a changeable feature"
    );

    let default = encode_sqe(
        ADMIN_OP_GET_FEATURES,
        0x72,
        0,
        0,
        u32::from(FEATURE_VOLATILE_WRITE_CACHE)
            | (GET_FEATURE_SELECT_DEFAULT << GET_FEATURE_SELECT_SHIFT),
        0,
        0,
    );
    submit_admin(&mut ctrl, &mut mem, 2, &default);
    let cqe = read_completion(&mem, ACQ_BASE, 2);
    assert_eq!(completion_status(&cqe), SC_SUCCESS);
    assert_eq!(
        completion_dw0(&cqe),
        0,
        "QEMU reports VWC default as disabled even when current is enabled"
    );

    let saved = encode_sqe(
        ADMIN_OP_GET_FEATURES,
        0x73,
        0,
        0,
        u32::from(FEATURE_VOLATILE_WRITE_CACHE)
            | (GET_FEATURE_SELECT_SAVED << GET_FEATURE_SELECT_SHIFT),
        0,
        0,
    );
    submit_admin(&mut ctrl, &mut mem, 3, &saved);
    let cqe = read_completion(&mem, ACQ_BASE, 3);
    assert_eq!(completion_status(&cqe), SC_SUCCESS);
    assert_eq!(
        completion_dw0(&cqe),
        0,
        "QEMU falls saved VWC back to the default value"
    );
}

#[test]
fn set_features_volatile_write_cache_updates_current_value() {
    let (mut ctrl, mut mem) = enabled_controller();
    let disable = encode_sqe(
        ADMIN_OP_SET_FEATURES,
        0x72,
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

    let current = encode_sqe(
        ADMIN_OP_GET_FEATURES,
        0x73,
        0,
        0,
        u32::from(FEATURE_VOLATILE_WRITE_CACHE),
        0,
        0,
    );
    submit_admin(&mut ctrl, &mut mem, 1, &current);
    let cqe = read_completion(&mem, ACQ_BASE, 1);
    assert_eq!(completion_status(&cqe), SC_SUCCESS);
    assert_eq!(completion_dw0(&cqe), 0);

    let enable = encode_sqe(
        ADMIN_OP_SET_FEATURES,
        0x74,
        0,
        0,
        u32::from(FEATURE_VOLATILE_WRITE_CACHE),
        1,
        0,
    );
    submit_admin(&mut ctrl, &mut mem, 2, &enable);
    assert_eq!(
        completion_status(&read_completion(&mem, ACQ_BASE, 2)),
        SC_SUCCESS
    );

    let current = encode_sqe(
        ADMIN_OP_GET_FEATURES,
        0x75,
        0,
        0,
        u32::from(FEATURE_VOLATILE_WRITE_CACHE),
        0,
        0,
    );
    submit_admin(&mut ctrl, &mut mem, 3, &current);
    let cqe = read_completion(&mem, ACQ_BASE, 3);
    assert_eq!(completion_status(&cqe), SC_SUCCESS);
    assert_eq!(completion_dw0(&cqe), 1);
}

#[test]
fn get_features_apst_returns_zero_table() {
    let (mut ctrl, mut mem) = enabled_controller();
    assert!(mem.write_bytes(DATA_BASE, &[0xaa; 256]));
    let sqe = encode_sqe(
        ADMIN_OP_GET_FEATURES,
        9,
        0,
        DATA_BASE,
        u32::from(FEATURE_AUTONOMOUS_POWER_STATE_TRANSITION),
        0,
        0,
    );
    submit_admin(&mut ctrl, &mut mem, 0, &sqe);
    let cqe = read_completion(&mem, ACQ_BASE, 0);
    assert_eq!(completion_status(&cqe), SC_SUCCESS);
    assert_eq!(completion_dw0(&cqe), 0);
    assert_eq!(mem.read_bytes(DATA_BASE, 256).unwrap(), vec![0u8; 256]);
}

#[test]
fn get_features_unknown_feature_matches_qemu_invalid_field_dnr() {
    let (mut ctrl, mut mem) = enabled_controller();
    for (slot, fid) in [0xd0u8, 0x7f].into_iter().enumerate() {
        let sqe = encode_sqe(
            ADMIN_OP_GET_FEATURES,
            10 + slot as u16,
            0,
            0,
            u32::from(fid),
            0,
            0,
        );
        submit_admin(&mut ctrl, &mut mem, slot as u16, &sqe);
        assert_eq!(
            completion_status(&read_completion(&mem, ACQ_BASE, slot as u16)),
            SC_INVALID_FIELD_DNR,
            "feature {fid:#x} matches QEMU's unsupported default with DNR"
        );
    }
}

#[test]
fn create_io_queues_then_write_read_round_trips_one_lba() {
    let (mut ctrl, mut mem) = enabled_controller();

    // 1) CREATE I/O COMPLETION QUEUE (QID 1, depth QDEPTH, base IO_CQ_BASE).
    let cdw10 = (u32::from(QDEPTH - 1) << 16) | 1; // QSIZE(0-based)<<16 | QID
    let cq_cmd = encode_sqe(
        ADMIN_OP_CREATE_IO_CQ,
        1,
        0,
        IO_CQ_BASE,
        cdw10,
        CREATE_IO_CQ_PC_BIT,
        0,
    );
    submit_admin(&mut ctrl, &mut mem, 0, &cq_cmd);
    assert_eq!(
        completion_status(&read_completion(&mem, ACQ_BASE, 0)),
        SC_SUCCESS
    );

    // 2) CREATE I/O SUBMISSION QUEUE (QID 1 → CQID 1, base IO_SQ_BASE).
    let sq_cmd = encode_sqe(
        ADMIN_OP_CREATE_IO_SQ,
        2,
        0,
        IO_SQ_BASE,
        cdw10,
        1u32 << 16, // CQID = 1 in bits 31:16
        0,
    );
    submit_admin(&mut ctrl, &mut mem, 1, &sq_cmd);
    assert_eq!(
        completion_status(&read_completion(&mem, ACQ_BASE, 1)),
        SC_SUCCESS
    );

    // 3) Stage a known pattern in the guest data buffer and WRITE LBA 7.
    let pattern: Vec<u8> = (0..LBA_SIZE).map(|i| (i % 256) as u8).collect();
    assert!(mem.write_bytes(DATA_BASE, &pattern));
    let slba: u64 = 7;
    let write_cmd = encode_sqe(
        NVM_OP_WRITE,
        0x10,
        NSID,
        DATA_BASE,
        slba as u32,         // CDW10 = SLBA low
        (slba >> 32) as u32, // CDW11 = SLBA high
        0,                   // CDW12 = NLB 0-based ⇒ 1 block
    );
    // I/O SQ 1 tail doorbell is at DOORBELL_BASE + 2*4 (SQ1TDBL).
    let io_sq1_dbl = REG_DOORBELL_BASE + 2 * 4;
    let gpa = IO_SQ_BASE; // slot 0
    assert!(mem.write_bytes(gpa, &write_cmd));
    ctrl.mmio_write(io_sq1_dbl, 4, 1); // tail = 1
    ctrl.process(&mut mem);
    let w_cqe = read_completion(&mem, IO_CQ_BASE, 0);
    assert_eq!(completion_status(&w_cqe), SC_SUCCESS, "WRITE completes ok");

    // 4) Zero the data buffer, then READ LBA 7 back into it.
    assert!(mem.write_bytes(DATA_BASE, &vec![0u8; LBA_SIZE]));
    let read_cmd = encode_sqe(
        NVM_OP_READ,
        0x11,
        NSID,
        DATA_BASE,
        slba as u32,
        (slba >> 32) as u32,
        0,
    );
    assert!(mem.write_bytes(IO_SQ_BASE + SQ_ENTRY_SIZE, &read_cmd)); // slot 1
    ctrl.mmio_write(io_sq1_dbl, 4, 2); // tail = 2
    ctrl.process(&mut mem);
    let r_cqe = read_completion(&mem, IO_CQ_BASE, 1);
    assert_eq!(completion_status(&r_cqe), SC_SUCCESS, "READ completes ok");

    // 5) The data round-trips through the disk backend byte-for-byte.
    let got = mem.read_bytes(DATA_BASE, LBA_SIZE).unwrap();
    assert_eq!(got, pattern, "WRITE then READ of one LBA round-trips");

    let trace = ctrl.recent_command_trace();
    let write_trace = trace
        .iter()
        .find(|event| event.sqid == 1 && event.command_id == 0x10)
        .expect("I/O WRITE command trace is retained");
    assert_eq!(write_trace.opcode, NVM_OP_WRITE);
    assert_eq!(write_trace.status, SC_SUCCESS);
    assert_eq!(write_trace.cdw10, slba as u32);
    assert!(write_trace.completion_posted);
    assert_eq!(write_trace.completion, None);

    let read_trace = trace
        .iter()
        .find(|event| event.sqid == 1 && event.command_id == 0x11)
        .expect("I/O READ command trace is retained");
    assert_eq!(read_trace.opcode, NVM_OP_READ);
    assert_eq!(read_trace.status, SC_SUCCESS);
    assert_eq!(read_trace.cdw10, slba as u32);
    assert!(read_trace.completion_posted);
    assert_eq!(read_trace.completion, None);
}

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
