use super::*;
use crate::nvme_storage_effect::{NvmePcieLivenessSnapshot, NvmeStorageEffectSummary};
use std::ffi::OsString;
use std::sync::Mutex;

static ENV_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn storage_effect_receipt_fails_closed_for_admin_only_summary() {
    let input = admin_only_input();

    let receipt = render_storage_effect_receipt(input);

    assert_receipt_contains(
        &receipt,
        "nvme_disk_configured=true nvme_write_back=true nvme_snapshot_path_configured=false \
         nvme_io_write_success_count=0 nvme_io_flush_success_count=0 \
         nvme_admin_create_io_sq_count=1 exact_target_storage_evidence=absent \
         scratch_mutation=unknown",
    );
}

#[test]
fn storage_effect_receipt_marks_exact_evidence_only_for_io_write() {
    let input = receipt_input(configured_disk(false, true), summary(2, 1, 1));

    let receipt = render_storage_effect_receipt(input);

    assert_receipt_contains(
        &receipt,
        "nvme_disk_configured=true nvme_write_back=false nvme_snapshot_path_configured=true \
         nvme_io_write_success_count=2 nvme_io_flush_success_count=1 \
         nvme_io_write_command_count=2 nvme_io_command_count=3 \
         exact_target_storage_evidence=present \
         target_effect_class=present_successful_io_write",
    );
}

#[test]
fn storage_effect_receipt_keeps_flush_only_evidence_absent() {
    let input = receipt_input(configured_disk(false, false), summary(0, 1, 0));

    let receipt = render_storage_effect_receipt(input);

    assert_receipt_contains(
        &receipt,
        "nvme_io_write_success_count=0 nvme_io_flush_success_count=1 \
         exact_target_storage_evidence=absent",
    );
}

#[test]
fn storage_effect_receipt_records_absent_write_not_successful_class() {
    let input = receipt_input(
        configured_disk(true, false),
        summary_with_commands(0, 1, 0, 1, 0, 0),
    );

    let receipt = render_storage_effect_receipt(input);

    assert_receipt_contains(
        &receipt,
        "nvme_io_write_success_count=0 nvme_io_write_command_count=1 \
         nvme_io_command_count=1 exact_target_storage_evidence=absent \
         target_effect_class=absent_write_not_successful",
    );
}

#[test]
fn storage_effect_receipt_records_nvme_pcie_liveness_stages() {
    let input = StorageEffectReceiptInput {
        liveness: NvmePcieLivenessSnapshot {
            nvme_advertised: true,
            nvme_ecam_touched: true,
            nvme_command_memory_enabled: true,
            nvme_command_bus_master_enabled: true,
            nvme_bar0_assigned: true,
            nvme_mmio_reached: true,
            nvme_cc_enabled: true,
            nvme_admin_doorbell_rung: true,
        },
        ..receipt_input(
            configured_disk(true, false),
            summary_with_commands(0, 0, 0, 1, 1, 1),
        )
    };

    let receipt = render_storage_effect_receipt(input);

    assert_receipt_contains(
        &receipt,
        "nvme_advertised=true nvme_ecam_touched=true \
         nvme_command_memory_enabled=true nvme_command_bus_master_enabled=true \
         nvme_bar0_assigned=true nvme_mmio_reached=true nvme_cc_enabled=true \
         nvme_admin_doorbell_rung=true nvme_admin_create_io_cq_completed=true \
         nvme_admin_create_io_sq_completed=true nvme_io_command_processed=true \
         nvme_io_write_success_processed=false exact_target_storage_evidence=absent",
    );
}

#[test]
fn storage_effect_receipt_records_absent_and_present_scratch_when_provided() {
    let absent = StorageEffectReceiptInput {
        scratch_mutation: ScratchMutation::Absent,
        ..receipt_input(NvmeDiskReceiptConfig::NotConfigured, summary(0, 0, 0))
    };
    let present = StorageEffectReceiptInput {
        scratch_mutation: ScratchMutation::Present,
        ..absent
    };

    let absent_receipt = render_storage_effect_receipt(absent);

    assert_receipt_contains(
        &absent_receipt,
        "nvme_disk_configured=false nvme_io_write_success_count=0 \
         nvme_io_flush_success_count=0 exact_target_storage_evidence=absent \
         target_effect_class=absent_no_io_queue_created scratch_mutation=absent",
    );
    assert!(render_storage_effect_receipt(present).contains("scratch_mutation=present"));
}

#[test]
fn storage_effect_receipt_from_env_is_optional_without_receipt_env() {
    let _guard = ENV_LOCK.lock().unwrap();
    let previous = std::env::var_os(RECEIPT_PATH_ENV);
    std::env::remove_var(RECEIPT_PATH_ENV);
    let input = admin_only_input();

    let written = write_storage_effect_receipt_from_env(input);

    restore_env(previous);
    assert_eq!(written.unwrap(), None);
}

#[test]
fn storage_effect_receipt_from_env_writes_only_when_receipt_env_is_set() {
    let _guard = ENV_LOCK.lock().unwrap();
    let previous = std::env::var_os(RECEIPT_PATH_ENV);
    let path = temp_path("storage-effect-receipt.txt");
    std::env::set_var(RECEIPT_PATH_ENV, &path);
    let input = admin_only_input();

    let written = write_storage_effect_receipt_from_env(input);

    restore_env(previous);
    assert_eq!(written.unwrap(), Some(path.clone()));
    let receipt = fs::read_to_string(&path).unwrap();
    assert!(receipt.contains("exact_target_storage_evidence=absent"));
    fs::remove_file(path).unwrap();
}

#[test]
fn storage_effect_receipt_write_error_is_visible_and_keeps_admin_only_absent() {
    let dir = temp_path("storage-effect-receipt-dir");
    fs::create_dir(&dir).unwrap();
    let input = admin_only_input();

    let error = write_storage_effect_receipt(&dir, input).unwrap_err();

    assert!(error.to_string().contains("write storage-effect receipt"));
    assert!(render_storage_effect_receipt(input).contains("exact_target_storage_evidence=absent"));
    fs::remove_dir(dir).unwrap();
}

fn admin_only_input() -> StorageEffectReceiptInput {
    receipt_input(configured_disk(true, false), summary(0, 0, 1))
}

fn receipt_input(
    nvme_disk: NvmeDiskReceiptConfig,
    summary: NvmeStorageEffectSummary,
) -> StorageEffectReceiptInput {
    StorageEffectReceiptInput {
        nvme_disk,
        liveness: NvmePcieLivenessSnapshot::default(),
        summary,
        scratch_mutation: ScratchMutation::Unknown,
    }
}

fn configured_disk(write_back: bool, snapshot_path_configured: bool) -> NvmeDiskReceiptConfig {
    NvmeDiskReceiptConfig::Configured {
        write_back,
        snapshot_path_configured,
    }
}

fn summary(writes: usize, flushes: usize, admin_sqs: usize) -> NvmeStorageEffectSummary {
    summary_with_commands(writes, writes, flushes, writes + flushes, 0, admin_sqs)
}

fn summary_with_commands(
    writes: usize,
    write_commands: usize,
    flushes: usize,
    io_commands: usize,
    admin_cqs: usize,
    admin_sqs: usize,
) -> NvmeStorageEffectSummary {
    NvmeStorageEffectSummary {
        io_write_success_count: writes,
        io_write_command_count: write_commands,
        io_flush_success_count: flushes,
        io_command_count: io_commands,
        admin_create_io_cq_count: admin_cqs,
        admin_create_io_sq_count: admin_sqs,
    }
}

fn assert_receipt_contains(receipt: &str, fields: &str) {
    for field in fields.split_whitespace() {
        assert!(receipt.contains(field), "missing {field} in {receipt}");
    }
}

fn temp_path(name: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "bridgevm-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    path
}

fn restore_env(previous: Option<OsString>) {
    match previous {
        Some(value) => std::env::set_var(RECEIPT_PATH_ENV, value),
        None => std::env::remove_var(RECEIPT_PATH_ENV),
    }
}
