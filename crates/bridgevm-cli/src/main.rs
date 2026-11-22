//! The `bridgevm` command-line front-end.
//!
//! Routes legacy store/daemon commands through `bridgevm-api`, runs local HVF
//! tools, and forwards native app inventory queries to the packaged app CLI.

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
use bridgevm_hvf::{probe_virtio_gpu_3d_host_preflight_for, VirtioGpu3dHostPreflightProtocol};
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

mod app_cli;
mod app_cli_resolver;
mod args;
mod boot_media;
mod clone_migrate;
mod daemon_io;
mod disk;
mod doctor;
mod entry;
mod json_util;
mod local_dispatch;
mod report;
mod request;
mod request_create;
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
use request_create::*;
use runtime_print::*;
use snapshot::*;
use ssh_runtime::*;
use title_gate::*;
use vm_cmds::*;

fn main() -> anyhow::Result<()> {
    entry::run()
}
