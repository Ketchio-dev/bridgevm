use anyhow::{bail, Context, Result};
use bridgevm_agent_protocol::{AgentEnvelope, AgentMessage, WindowInputEvent};
use bridgevm_api::{
    accept_guest_tools_hello, add_fast_spawn_runner_required_blocker, add_port, add_share,
    apple_vz_runner_configured, cold_start_fast_backend, compatibility_launch_dependency_blockers,
    compatibility_launch_readiness_metadata, create_diagnostic_bundle, create_performance_baseline,
    create_performance_sample, display_fast_backend_with_size, download_boot_media,
    fast_spawn_runner_required_error, guest_tools_linux_command, guest_tools_token,
    import_boot_media, inspect_boot_media_status, inspect_guest_tools_status,
    launch_readiness_metadata, list_ports, list_shares, open_port_plan, plan_boot_media_download,
    reapply_runtime_resources, remove_port, remove_share, resume_backend, runtime_control_command,
    stop_backend, suspend_backend, verify_boot_media, view_vm_log,
    ApplicationConsistentSnapshotExecutionRecord, BootMediaDownloadPlanMetadata,
    BootMediaDownloadResultMetadata, BootMediaImportMetadata, BootMediaKind, BootMediaStatus,
    BootMediaVerificationMetadata, BridgeVmRequest, BridgeVmResponse, DiagnosticBundleMetadata,
    GuestToolsLinuxCommandRecord, GuestToolsLinuxCommandTransport, GuestToolsSessionRecord,
    GuestToolsStatusRecord, GuestToolsTokenRecord, LifecycleAction, LifecyclePlanRecord,
    NetworkPlanRecord, OpenPortPlanRecord, PerformanceBaselineMetadata, PerformanceSampleMetadata,
    PortForwardListRecord, RuntimeControlCommandRecord, SharedFolderListRecord,
    SnapshotPreflightStatusRecord, SshPlanRecord, VmLogKind, VmLogViewRecord, VmReadinessReport,
    VmRecord,
};
use bridgevm_apple_vz::{
    build_fast_plan, write_launch_spec_artifact, AppleVzBootSpec, AppleVzPathSpec,
};
use bridgevm_config::{manifest_json_schema_v1, Boot, BootMode, Guest, VmManifest, VmMode};
use bridgevm_core::{
    available_boot_templates, available_engine_descriptors, boot_template_by_id,
    current_engine_descriptor_for_mode, recommend_mode, target_engine_descriptor_for_guest,
    BootTemplate, GuestChoice, ModeRecommendation, VmEngineDescriptor,
};
use bridgevm_hvf::{
    plan_windows_11_arm_hvf_machine, plan_windows_11_arm_no_qemu, probe_hvf_guest_entry,
    probe_hvf_guest_exit_loop, probe_hvf_interrupt_timer, probe_hvf_memory_map,
    probe_hvf_mmio_block_device, probe_hvf_mmio_block_queue, probe_hvf_mmio_read_emulation,
    probe_hvf_mmio_read_exit, probe_hvf_mmio_rtc_device, probe_hvf_mmio_serial_device,
    probe_hvf_mmio_write_emulation, probe_hvf_vcpu_create, probe_hvf_vcpu_run, probe_hvf_vm_create,
    probe_hvf_vtimer_exit, probe_virtio_block_file_backing, probe_virtio_block_iso_backing,
    probe_virtio_block_request_model, probe_virtio_block_writable_file_backing,
    probe_virtio_gpu_3d_host_preflight_for, probe_windows_11_arm_boot_disk_layout,
    probe_windows_11_arm_platform_description, probe_windows_11_arm_uefi_firmware_device_discovery,
    probe_windows_11_arm_uefi_firmware_handoff, probe_windows_11_arm_uefi_firmware_run_loop,
    probe_windows_11_arm_uefi_pflash_hvf_map, probe_windows_11_arm_uefi_pflash_map,
    probe_windows_11_arm_uefi_reset_vector_entry, probe_windows_11_arm_xhci_hid_boot_key_report,
    query_hvf_host_capabilities, HvfMachinePlanOptions, VirtioGpu3dHostPreflightProtocol,
    WindowsArmBootDiskLayoutOptions, WindowsArmPlatformDescriptionOptions,
    WindowsArmUefiFirmwareHandoffOptions, WindowsArmUefiFirmwareRunLoopExecutionOptions,
    WindowsArmUefiFirmwareRunLoopOptions, WindowsArmUefiPflashMapOptions,
    WINDOWS_ARM_BOOT_DISK_DEFAULT_SIZE_GIB,
};
use bridgevm_qemu::{
    build_compatibility_command, cont as qmp_cont, is_qmp_status_unavailable, qmp_socket_path,
    query_status, stop as qmp_stop, QemuError,
};
use bridgevm_storage::{
    ApplicationConsistentSnapshotPreflightMetadata, LaunchReadinessMetadata, QmpSupervisorMetadata,
    RuntimeResourcePolicyMetadata, RuntimeResourceVisibility, SnapshotKind,
    VmManifestMigrationMetadata, VmMetadataRepairMetadata, VmRuntimeState, VmStore,
};
use clap::{Parser, Subcommand, ValueEnum};
use sha2::{Digest, Sha256};
use std::{
    env,
    fs::{self, OpenOptions},
    io::{BufRead, BufReader, Read, Write},
    os::unix::fs::PermissionsExt,
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    process::{Command as ProcessCommand, Stdio},
    time::Duration,
};

const MAX_DAEMON_RESPONSE_BYTES: u64 = 16 * 1024 * 1024;
const DAEMON_IO_TIMEOUT: Duration = Duration::from_secs(30);

#[cfg(test)]
mod test_support;

mod args;
mod boot_media;
mod clone_migrate;
mod daemon_io;
mod disk;
mod doctor;
mod entry;
mod json_util;
mod report;
mod request;
mod runtime_print;
mod snapshot;
mod ssh_runtime;
mod title_gate;
mod vm_cmds;

use args::*;
use boot_media::*;
use clone_migrate::*;
use daemon_io::*;
use disk::*;
use doctor::*;
use json_util::*;
use report::*;
use request::*;
use runtime_print::*;
use snapshot::*;
use ssh_runtime::*;
use title_gate::*;
use vm_cmds::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_hvf_plan_render_is_blocked_and_qemu_free() {
        let plan =
            plan_windows_11_arm_no_qemu(Some(PathBuf::from("ISO/Win11_25H2_English_Arm64_v2.iso")));
        let output = plan.render_text();

        assert!(output.contains("Windows 11 Arm no-QEMU HVF plan"));
        assert!(output.contains("Engine: BridgeVM HVF"));
        assert!(output.contains("Substrate: Apple Hypervisor.framework"));
        assert!(output.contains("Installer: ISO/Win11_25H2_English_Arm64_v2.iso"));
        assert!(output.contains("QEMU: not used"));
        assert!(output.contains("Overall: blocked"));
        assert!(!output.contains("qemu-system"));
        assert!(!output.contains('%'));
    }
}

fn main() -> anyhow::Result<()> {
    entry::run()
}
