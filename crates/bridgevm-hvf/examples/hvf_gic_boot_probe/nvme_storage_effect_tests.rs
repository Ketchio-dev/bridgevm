use super::*;
use bridgevm_hvf::nvme::NvmeCommandTrace;

fn trace(sqid: u16, opcode: u8) -> NvmeCommandTrace {
    NvmeCommandTrace {
        sqid,
        cqid: 0,
        sq_head: 0,
        sq_tail: 0,
        sq_entry_gpa: 0,
        opcode,
        command_id: 0,
        nsid: 0,
        prp1: 0,
        prp2: 0,
        cdw10: 0,
        cdw11: 0,
        cdw12: 0,
        cdw13: 0,
        cdw14: 0,
        cdw15: 0,
        status: 0,
        completion_posted: true,
        completion: None,
    }
}

macro_rules! summary {
    ($write_success:expr, $write_command:expr, $flush_success:expr, $io_command:expr, $admin_cq:expr, $admin_sq:expr) => {
        NvmeStorageEffectSummary {
            io_write_success_count: $write_success,
            io_write_command_count: $write_command,
            io_flush_success_count: $flush_success,
            io_command_count: $io_command,
            admin_create_io_cq_count: $admin_cq,
            admin_create_io_sq_count: $admin_sq,
        }
    };
}

#[test]
fn storage_effect_summary_classifies_target_effect_absence() {
    struct ClassCase {
        name: &'static str,
        trace: Vec<NvmeCommandTrace>,
        expected_summary: NvmeStorageEffectSummary,
        expected_class: NvmeTargetEffectClass,
        expected_evidence: &'static str,
    }

    let cases = [
        ClassCase {
            name: "no_io_queue_created",
            trace: Vec::new(),
            expected_summary: summary!(0, 0, 0, 0, 0, 0),
            expected_class: NvmeTargetEffectClass::AbsentNoIoQueueCreated,
            expected_evidence: "absent",
        },
        ClassCase {
            name: "io_queue_created_no_io_command",
            trace: vec![trace(0, 0x01)],
            expected_summary: summary!(0, 0, 0, 0, 0, 1),
            expected_class: NvmeTargetEffectClass::AbsentIoQueueCreatedNoIoCommand,
            expected_evidence: "absent",
        },
        ClassCase {
            name: "admin_create_io_cq_only",
            trace: vec![trace(0, 0x05)],
            expected_summary: summary!(0, 0, 0, 0, 1, 0),
            expected_class: NvmeTargetEffectClass::AbsentNoIoQueueCreated,
            expected_evidence: "absent",
        },
        ClassCase {
            name: "flush_only",
            trace: vec![trace(1, 0x00)],
            expected_summary: summary!(0, 0, 1, 1, 0, 0),
            expected_class: NvmeTargetEffectClass::AbsentFlushOnly,
            expected_evidence: "absent",
        },
        ClassCase {
            name: "failed_or_pending_io_write",
            trace: vec![
                NvmeCommandTrace {
                    status: 0x0001,
                    ..trace(1, 0x01)
                },
                NvmeCommandTrace {
                    completion_posted: false,
                    ..trace(1, 0x01)
                },
            ],
            expected_summary: summary!(0, 2, 0, 2, 0, 0),
            expected_class: NvmeTargetEffectClass::AbsentWriteNotSuccessful,
            expected_evidence: "absent",
        },
        ClassCase {
            name: "non_write_io_command",
            trace: vec![trace(1, 0x02)],
            expected_summary: summary!(0, 0, 0, 1, 0, 0),
            expected_class: NvmeTargetEffectClass::AbsentNoWriteCommand,
            expected_evidence: "absent",
        },
        ClassCase {
            name: "successful_io_write",
            trace: vec![trace(1, 0x01)],
            expected_summary: summary!(1, 1, 0, 1, 0, 0),
            expected_class: NvmeTargetEffectClass::PresentSuccessfulIoWrite,
            expected_evidence: "present",
        },
        ClassCase {
            name: "admin_write_flush_with_failed_lookalikes",
            trace: vec![
                trace(0, 0x01),
                trace(1, 0x01),
                trace(1, 0x00),
                NvmeCommandTrace {
                    status: 0x0001,
                    ..trace(1, 0x01)
                },
                NvmeCommandTrace {
                    completion_posted: false,
                    ..trace(1, 0x01)
                },
                trace(0, 0x00),
            ],
            expected_summary: summary!(1, 3, 1, 4, 0, 1),
            expected_class: NvmeTargetEffectClass::PresentSuccessfulIoWrite,
            expected_evidence: "present",
        },
    ];

    for case in cases {
        let trace = case.trace;
        let summary = nvme_storage_effect_summary(&trace);
        let line = nvme_storage_effect_summary_line(&trace);

        assert_eq!(summary, case.expected_summary);
        assert_eq!(summary.target_effect_class(), case.expected_class);
        assert_eq!(
            summary.exact_target_storage_evidence(),
            case.expected_evidence
        );
        assert!(line.contains(&format!(
            "io_write_success_count={}",
            case.expected_summary.io_write_success_count
        )));
        assert!(line.contains(&format!(
            "io_write_command_count={}",
            case.expected_summary.io_write_command_count
        )));
        assert!(line.contains(&format!(
            "io_flush_success_count={}",
            case.expected_summary.io_flush_success_count
        )));
        assert!(line.contains(&format!(
            "io_command_count={}",
            case.expected_summary.io_command_count
        )));
        assert!(line.contains(&format!(
            "admin_create_io_cq_count={}",
            case.expected_summary.admin_create_io_cq_count
        )));
        assert!(line.contains(&format!(
            "admin_create_io_sq_count={}",
            case.expected_summary.admin_create_io_sq_count
        )));
        assert!(line.contains(&format!(
            "exact_target_storage_evidence={}",
            case.expected_evidence
        )));
        assert!(line.contains(&format!(
            "target_effect_class={}",
            case.expected_class.as_str()
        )));
        println!(
            "storage_effect_summary_class case={} class={} evidence={}",
            case.name,
            case.expected_class.as_str(),
            case.expected_evidence
        );
    }
}

#[test]
fn storage_effect_summary_class_string_values_are_stable() {
    let cases = [
        (
            NvmeTargetEffectClass::PresentSuccessfulIoWrite,
            "present_successful_io_write",
        ),
        (
            NvmeTargetEffectClass::AbsentNoIoQueueCreated,
            "absent_no_io_queue_created",
        ),
        (
            NvmeTargetEffectClass::AbsentIoQueueCreatedNoIoCommand,
            "absent_io_queue_created_no_io_command",
        ),
        (
            NvmeTargetEffectClass::AbsentNoWriteCommand,
            "absent_no_write_command",
        ),
        (
            NvmeTargetEffectClass::AbsentWriteNotSuccessful,
            "absent_write_not_successful",
        ),
        (NvmeTargetEffectClass::AbsentFlushOnly, "absent_flush_only"),
    ];

    for (class, expected) in cases {
        assert_eq!(class.as_str(), expected);
    }
}

#[test]
fn storage_effect_summary_ignores_failed_or_pending_admin_create_io_sq() {
    let trace = [
        NvmeCommandTrace {
            status: 0x0001,
            ..trace(0, 0x01)
        },
        NvmeCommandTrace {
            completion_posted: false,
            ..trace(0, 0x01)
        },
    ];

    let summary = nvme_storage_effect_summary(&trace);

    assert_eq!(summary, summary!(0, 0, 0, 0, 0, 0));
    assert_eq!(
        summary.target_effect_class(),
        NvmeTargetEffectClass::AbsentNoIoQueueCreated
    );
    assert_eq!(summary.exact_target_storage_evidence(), "absent");
}

#[test]
fn storage_effect_summary_line_records_nvme_pcie_liveness_stages() {
    let trace = [trace(0, 0x05), trace(0, 0x01), trace(1, 0x02)];
    let liveness = NvmePcieLivenessSnapshot {
        nvme_advertised: true,
        nvme_ecam_touched: true,
        nvme_command_memory_enabled: true,
        nvme_command_bus_master_enabled: true,
        nvme_bar0_assigned: true,
        nvme_mmio_reached: true,
        nvme_cc_enabled: true,
        nvme_admin_doorbell_rung: true,
    };

    let line = nvme_pcie_liveness_attribution_line(liveness, nvme_storage_effect_summary(&trace));

    assert!(line.contains("nvme_pcie_liveness:"));
    assert!(line.contains("nvme_advertised=true"));
    assert!(line.contains("nvme_ecam_touched=true"));
    assert!(line.contains("nvme_command_memory_enabled=true"));
    assert!(line.contains("nvme_command_bus_master_enabled=true"));
    assert!(line.contains("nvme_bar0_assigned=true"));
    assert!(line.contains("nvme_mmio_reached=true"));
    assert!(line.contains("nvme_cc_enabled=true"));
    assert!(line.contains("nvme_admin_doorbell_rung=true"));
    assert!(line.contains("nvme_admin_create_io_cq_completed=true"));
    assert!(line.contains("nvme_admin_create_io_sq_completed=true"));
    assert!(line.contains("nvme_io_command_processed=true"));
    assert!(line.contains("nvme_io_write_success_processed=false"));
    assert!(line.contains("exact_target_storage_evidence=absent"));
}
