use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use bridgevm_hvf::media::WritableMedia;
use bridgevm_hvf::platform_virt::VirtPlatform;

use crate::nvme_storage_effect::{
    nvme_storage_effect_summary, NvmePcieLivenessSnapshot, NvmeStorageEffectSummary,
};

const RECEIPT_PATH_ENV: &str = "BRIDGEVM_STORAGE_EFFECT_RECEIPT_OUT";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NvmeDiskReceiptConfig {
    NotConfigured,
    Configured {
        write_back: bool,
        snapshot_path_configured: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ScratchMutation {
    Unknown,
    #[cfg(test)]
    Absent,
    #[cfg(test)]
    Present,
}

impl ScratchMutation {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            #[cfg(test)]
            Self::Absent => "absent",
            #[cfg(test)]
            Self::Present => "present",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct StorageEffectReceiptInput {
    pub(super) nvme_disk: NvmeDiskReceiptConfig,
    pub(super) liveness: NvmePcieLivenessSnapshot,
    pub(super) summary: NvmeStorageEffectSummary,
    pub(super) scratch_mutation: ScratchMutation,
}

impl StorageEffectReceiptInput {
    pub(super) fn from_nvme_media(
        nvme_disk: Option<&WritableMedia>,
        liveness: NvmePcieLivenessSnapshot,
        summary: NvmeStorageEffectSummary,
    ) -> Self {
        Self {
            nvme_disk: match nvme_disk {
                Some(media) => NvmeDiskReceiptConfig::Configured {
                    write_back: media.write_back,
                    snapshot_path_configured: media.snapshot_path.is_some(),
                },
                None => NvmeDiskReceiptConfig::NotConfigured,
            },
            liveness,
            summary,
            scratch_mutation: ScratchMutation::Unknown,
        }
    }
}

pub(super) fn write_storage_effect_receipt_from_env(
    input: StorageEffectReceiptInput,
) -> io::Result<Option<PathBuf>> {
    let Some(path) = std::env::var_os(RECEIPT_PATH_ENV).map(PathBuf::from) else {
        return Ok(None);
    };
    write_storage_effect_receipt(&path, input)?;
    Ok(Some(path))
}

pub(super) fn maybe_write_probe_storage_effect_receipt(
    nvme_disk: Option<&WritableMedia>,
    platform: &VirtPlatform,
) {
    let trace = platform.nvme_command_trace();
    let input = StorageEffectReceiptInput::from_nvme_media(
        nvme_disk,
        NvmePcieLivenessSnapshot::from(platform.nvme_pcie_liveness()),
        nvme_storage_effect_summary(&trace),
    );
    if let Some(path) = write_storage_effect_receipt_from_env(input)
        .unwrap_or_else(|e| panic!("write storage-effect receipt: {e}"))
    {
        println!("storage-effect receipt written: {}", path.display());
    }
}

fn write_storage_effect_receipt(path: &Path, input: StorageEffectReceiptInput) -> io::Result<()> {
    fs::write(path, render_storage_effect_receipt(input)).map_err(|error| {
        io::Error::new(
            error.kind(),
            format!("write storage-effect receipt {}: {error}", path.display()),
        )
    })
}

fn render_storage_effect_receipt(input: StorageEffectReceiptInput) -> String {
    let (nvme_disk_configured, nvme_write_back, nvme_snapshot_path_configured) =
        match input.nvme_disk {
            NvmeDiskReceiptConfig::NotConfigured => ("false", "false", "false"),
            NvmeDiskReceiptConfig::Configured {
                write_back,
                snapshot_path_configured,
            } => (
                "true",
                bool_key_value(write_back),
                bool_key_value(snapshot_path_configured),
            ),
        };
    format!(
        "bridgevm_storage_effect_receipt_v1\nnvme_disk_configured={nvme_disk_configured}\nnvme_write_back={nvme_write_back}\nnvme_snapshot_path_configured={nvme_snapshot_path_configured}\nnvme_advertised={}\nnvme_ecam_touched={}\nnvme_command_memory_enabled={}\nnvme_command_bus_master_enabled={}\nnvme_bar0_assigned={}\nnvme_mmio_reached={}\nnvme_cc_enabled={}\nnvme_admin_doorbell_rung={}\nnvme_admin_create_io_cq_completed={}\nnvme_admin_create_io_sq_completed={}\nnvme_io_command_processed={}\nnvme_io_write_success_processed={}\nnvme_io_write_success_count={}\nnvme_io_write_command_count={}\nnvme_io_flush_success_count={}\nnvme_io_command_count={}\nnvme_admin_create_io_cq_count={}\nnvme_admin_create_io_sq_count={}\nexact_target_storage_evidence={}\ntarget_effect_class={}\nscratch_mutation={}\n",
        bool_key_value(input.liveness.nvme_advertised),
        bool_key_value(input.liveness.nvme_ecam_touched),
        bool_key_value(input.liveness.nvme_command_memory_enabled),
        bool_key_value(input.liveness.nvme_command_bus_master_enabled),
        bool_key_value(input.liveness.nvme_bar0_assigned),
        bool_key_value(input.liveness.nvme_mmio_reached),
        bool_key_value(input.liveness.nvme_cc_enabled),
        bool_key_value(input.liveness.nvme_admin_doorbell_rung),
        bool_key_value(input.summary.admin_create_io_cq_count != 0),
        bool_key_value(input.summary.admin_create_io_sq_count != 0),
        bool_key_value(input.summary.io_command_count != 0),
        bool_key_value(input.summary.io_write_success_count != 0),
        input.summary.io_write_success_count,
        input.summary.io_write_command_count,
        input.summary.io_flush_success_count,
        input.summary.io_command_count,
        input.summary.admin_create_io_cq_count,
        input.summary.admin_create_io_sq_count,
        input.summary.exact_target_storage_evidence(),
        input.summary.target_effect_class().as_str(),
        input.scratch_mutation.as_str()
    )
}

const fn bool_key_value(value: bool) -> &'static str {
    if value {
        "true"
    } else {
        "false"
    }
}

#[cfg(test)]
#[path = "storage_effect_matrix.rs"]
mod storage_effect_matrix;

#[cfg(test)]
#[path = "storage_effect_receipt_tests.rs"]
mod tests;
