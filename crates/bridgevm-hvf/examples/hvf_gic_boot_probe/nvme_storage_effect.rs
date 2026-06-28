use bridgevm_hvf::nvme::NvmeCommandTrace;
use bridgevm_hvf::platform_virt::NvmePcieLiveness;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NvmeTargetEffectClass {
    PresentSuccessfulIoWrite,
    AbsentNoIoQueueCreated,
    AbsentIoQueueCreatedNoIoCommand,
    AbsentNoWriteCommand,
    AbsentWriteNotSuccessful,
    AbsentFlushOnly,
}

impl NvmeTargetEffectClass {
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::PresentSuccessfulIoWrite => "present_successful_io_write",
            Self::AbsentNoIoQueueCreated => "absent_no_io_queue_created",
            Self::AbsentIoQueueCreatedNoIoCommand => "absent_io_queue_created_no_io_command",
            Self::AbsentNoWriteCommand => "absent_no_write_command",
            Self::AbsentWriteNotSuccessful => "absent_write_not_successful",
            Self::AbsentFlushOnly => "absent_flush_only",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct NvmeStorageEffectSummary {
    pub(super) io_write_success_count: usize,
    pub(super) io_write_command_count: usize,
    pub(super) io_flush_success_count: usize,
    pub(super) io_command_count: usize,
    pub(super) admin_create_io_cq_count: usize,
    pub(super) admin_create_io_sq_count: usize,
}

impl NvmeStorageEffectSummary {
    pub(super) const fn exact_target_storage_evidence(self) -> &'static str {
        if self.io_write_success_count != 0 {
            "present"
        } else {
            "absent"
        }
    }

    pub(super) const fn target_effect_class(self) -> NvmeTargetEffectClass {
        if self.io_write_success_count != 0 {
            NvmeTargetEffectClass::PresentSuccessfulIoWrite
        } else if self.io_write_command_count != 0 {
            NvmeTargetEffectClass::AbsentWriteNotSuccessful
        } else if self.io_flush_success_count != 0 {
            NvmeTargetEffectClass::AbsentFlushOnly
        } else if self.io_command_count != 0 {
            NvmeTargetEffectClass::AbsentNoWriteCommand
        } else if self.admin_create_io_sq_count != 0 {
            NvmeTargetEffectClass::AbsentIoQueueCreatedNoIoCommand
        } else {
            NvmeTargetEffectClass::AbsentNoIoQueueCreated
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct NvmePcieLivenessSnapshot {
    pub(super) nvme_advertised: bool,
    pub(super) nvme_ecam_touched: bool,
    pub(super) nvme_command_memory_enabled: bool,
    pub(super) nvme_command_bus_master_enabled: bool,
    pub(super) nvme_bar0_assigned: bool,
    pub(super) nvme_mmio_reached: bool,
    pub(super) nvme_cc_enabled: bool,
    pub(super) nvme_admin_doorbell_rung: bool,
}

impl From<NvmePcieLiveness> for NvmePcieLivenessSnapshot {
    fn from(value: NvmePcieLiveness) -> Self {
        Self {
            nvme_advertised: value.nvme_advertised,
            nvme_ecam_touched: value.nvme_ecam_touched,
            nvme_command_memory_enabled: value.nvme_command_memory_enabled,
            nvme_command_bus_master_enabled: value.nvme_command_bus_master_enabled,
            nvme_bar0_assigned: value.nvme_bar0_assigned,
            nvme_mmio_reached: value.nvme_mmio_reached,
            nvme_cc_enabled: value.nvme_cc_enabled,
            nvme_admin_doorbell_rung: value.nvme_admin_doorbell_rung,
        }
    }
}

pub(super) fn nvme_storage_effect_summary_line(trace: &[NvmeCommandTrace]) -> String {
    let summary = nvme_storage_effect_summary(trace);
    format!(
        "storage target effect summary: io_write_success_count={} io_write_command_count={} io_flush_success_count={} io_command_count={} admin_create_io_cq_count={} admin_create_io_sq_count={} exact_target_storage_evidence={} target_effect_class={}",
        summary.io_write_success_count,
        summary.io_write_command_count,
        summary.io_flush_success_count,
        summary.io_command_count,
        summary.admin_create_io_cq_count,
        summary.admin_create_io_sq_count,
        summary.exact_target_storage_evidence(),
        summary.target_effect_class().as_str()
    )
}

pub(super) fn nvme_pcie_liveness_attribution_line(
    liveness: NvmePcieLivenessSnapshot,
    summary: NvmeStorageEffectSummary,
) -> String {
    format!(
        "nvme_pcie_liveness: nvme_advertised={} nvme_ecam_touched={} nvme_command_memory_enabled={} nvme_command_bus_master_enabled={} nvme_bar0_assigned={} nvme_mmio_reached={} nvme_cc_enabled={} nvme_admin_doorbell_rung={} nvme_admin_create_io_cq_completed={} nvme_admin_create_io_sq_completed={} nvme_io_command_processed={} nvme_io_write_success_processed={} exact_target_storage_evidence={}",
        bool_key_value(liveness.nvme_advertised),
        bool_key_value(liveness.nvme_ecam_touched),
        bool_key_value(liveness.nvme_command_memory_enabled),
        bool_key_value(liveness.nvme_command_bus_master_enabled),
        bool_key_value(liveness.nvme_bar0_assigned),
        bool_key_value(liveness.nvme_mmio_reached),
        bool_key_value(liveness.nvme_cc_enabled),
        bool_key_value(liveness.nvme_admin_doorbell_rung),
        bool_key_value(summary.admin_create_io_cq_count != 0),
        bool_key_value(summary.admin_create_io_sq_count != 0),
        bool_key_value(summary.io_command_count != 0),
        bool_key_value(summary.io_write_success_count != 0),
        summary.exact_target_storage_evidence()
    )
}

pub(super) fn nvme_storage_effect_summary(trace: &[NvmeCommandTrace]) -> NvmeStorageEffectSummary {
    let mut summary = NvmeStorageEffectSummary {
        io_write_success_count: 0,
        io_write_command_count: 0,
        io_flush_success_count: 0,
        io_command_count: 0,
        admin_create_io_cq_count: 0,
        admin_create_io_sq_count: 0,
    };
    for event in trace {
        match (event.sqid, event.opcode) {
            (sqid, 0x01) if sqid != 0 => {
                summary.io_write_command_count += 1;
                summary.io_command_count += 1;
            }
            (sqid, _) if sqid != 0 => summary.io_command_count += 1,
            _ => {}
        }
        if event.status != 0x0000 || !event.completion_posted {
            continue;
        }
        match (event.sqid, event.opcode) {
            (0, 0x05) => summary.admin_create_io_cq_count += 1,
            (0, 0x01) => summary.admin_create_io_sq_count += 1,
            (sqid, 0x01) if sqid != 0 => summary.io_write_success_count += 1,
            (sqid, 0x00) if sqid != 0 => summary.io_flush_success_count += 1,
            _ => {}
        }
    }
    summary
}

const fn bool_key_value(value: bool) -> &'static str {
    if value {
        "true"
    } else {
        "false"
    }
}

#[cfg(test)]
#[path = "nvme_storage_effect_tests.rs"]
mod tests;
