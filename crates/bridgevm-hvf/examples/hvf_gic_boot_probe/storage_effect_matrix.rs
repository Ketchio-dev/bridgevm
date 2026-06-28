use super::*;
use crate::nvme_storage_effect::{NvmePcieLivenessSnapshot, NvmeStorageEffectSummary};

#[test]
fn storage_effect_matrix_fixture_emits_parseable_receipts() {
    struct MatrixCase {
        name: &'static str,
        input: StorageEffectReceiptInput,
        expected_io_write: bool,
        expected_io_flush: bool,
        expected_fields: &'static str,
    }

    let cases = [
        MatrixCase {
            name: "admin_create_io_sq",
            input: receipt_input(configured_disk(true, false), summary(0, 0, 1)),
            expected_io_write: false,
            expected_io_flush: false,
            expected_fields: "nvme_disk_configured=true nvme_io_write_success_count=0 \
                              nvme_io_flush_success_count=0 nvme_admin_create_io_sq_count=1 \
                              exact_target_storage_evidence=absent \
                              target_effect_class=absent_io_queue_created_no_io_command",
        },
        MatrixCase {
            name: "io_write",
            input: receipt_input(configured_disk(true, false), summary(1, 0, 0)),
            expected_io_write: true,
            expected_io_flush: false,
            expected_fields: "nvme_disk_configured=true nvme_io_write_success_count=1 \
                              nvme_io_flush_success_count=0 exact_target_storage_evidence=present \
                              target_effect_class=present_successful_io_write",
        },
        MatrixCase {
            name: "io_write_not_successful",
            input: receipt_input(
                configured_disk(true, false),
                summary_with_commands(0, 1, 0, 1, 0, 0),
            ),
            expected_io_write: false,
            expected_io_flush: false,
            expected_fields: "nvme_disk_configured=true nvme_io_write_success_count=0 \
                              nvme_io_write_command_count=1 nvme_io_command_count=1 \
                              exact_target_storage_evidence=absent \
                              target_effect_class=absent_write_not_successful",
        },
        MatrixCase {
            name: "io_flush_only",
            input: receipt_input(configured_disk(true, false), summary(0, 1, 0)),
            expected_io_write: false,
            expected_io_flush: true,
            expected_fields: "nvme_disk_configured=true nvme_io_write_success_count=0 \
                              nvme_io_flush_success_count=1 exact_target_storage_evidence=absent \
                              target_effect_class=absent_flush_only",
        },
        MatrixCase {
            name: "io_read_only",
            input: receipt_input(
                configured_disk(true, false),
                summary_with_commands(0, 0, 0, 1, 0, 0),
            ),
            expected_io_write: false,
            expected_io_flush: false,
            expected_fields: "nvme_disk_configured=true nvme_io_write_success_count=0 \
                              nvme_io_write_command_count=0 nvme_io_command_count=1 \
                              exact_target_storage_evidence=absent \
                              target_effect_class=absent_no_write_command",
        },
        MatrixCase {
            name: "io_write_flush",
            input: receipt_input(configured_disk(true, false), summary(1, 1, 0)),
            expected_io_write: true,
            expected_io_flush: true,
            expected_fields: "nvme_disk_configured=true nvme_io_write_success_count=1 \
                              nvme_io_flush_success_count=1 exact_target_storage_evidence=present \
                              target_effect_class=present_successful_io_write",
        },
        MatrixCase {
            name: "no_target",
            input: receipt_input(NvmeDiskReceiptConfig::NotConfigured, summary(0, 0, 0)),
            expected_io_write: false,
            expected_io_flush: false,
            expected_fields: "nvme_disk_configured=false nvme_io_write_success_count=0 \
                              nvme_io_flush_success_count=0 exact_target_storage_evidence=absent \
                              target_effect_class=absent_no_io_queue_created",
        },
    ];

    for case in cases {
        let receipt = render_storage_effect_receipt(case.input);
        assert_eq!(
            case.input.summary.io_write_success_count != 0,
            case.expected_io_write
        );
        assert_eq!(
            case.input.summary.io_flush_success_count != 0,
            case.expected_io_flush
        );
        println!(
            "matrix_case={} io_write={} io_flush={} {}",
            case.name,
            case.expected_io_write,
            case.expected_io_flush,
            receipt.replace('\n', " ").trim()
        );
        assert_receipt_contains(&receipt, case.expected_fields);
        assert!(receipt.contains("scratch_mutation=unknown"));
    }
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

fn summary(
    io_write_success_count: usize,
    io_flush_success_count: usize,
    admin_create_io_sq_count: usize,
) -> NvmeStorageEffectSummary {
    summary_with_commands(
        io_write_success_count,
        io_write_success_count,
        io_flush_success_count,
        io_write_success_count + io_flush_success_count,
        0,
        admin_create_io_sq_count,
    )
}

fn summary_with_commands(
    io_write_success_count: usize,
    io_write_command_count: usize,
    io_flush_success_count: usize,
    io_command_count: usize,
    admin_create_io_cq_count: usize,
    admin_create_io_sq_count: usize,
) -> NvmeStorageEffectSummary {
    NvmeStorageEffectSummary {
        io_write_success_count,
        io_write_command_count,
        io_flush_success_count,
        io_command_count,
        admin_create_io_cq_count,
        admin_create_io_sq_count,
    }
}

fn assert_receipt_contains(receipt: &str, fields: &str) {
    for field in fields.split_whitespace() {
        assert!(receipt.contains(field), "missing {field} in {receipt}");
    }
}
