use bridgevm_agent_protocol::{AgentCapability, AgentEnvelope, AgentMessage};
use bridgevm_agentd::{accept_guest_hello, AgentPolicy};
use bridgevm_apple_vz::{
    build_fast_plan, write_launch_spec_artifact, AppleVzBootSpec, AppleVzPathSpec,
    AppleVzReadinessSpec,
};
use bridgevm_config::{BootMode, Guest, PortForward, SharedFolder, VmManifest, VmMode};
use bridgevm_core::{
    available_boot_templates, boot_template_by_id, current_engine_descriptor_for_mode,
    recommend_mode, BootTemplate, EngineLane, GuestChoice, ModeRecommendation,
};
use bridgevm_network::{
    plan_network, NetworkBackend, NetworkCapabilities, NetworkMode, NetworkPlanError,
    PortForwardRule,
};
use bridgevm_qemu::{
    assign_free_vnc_display, build_compatibility_command, cont as qmp_cont,
    is_qmp_status_unavailable, qmp_socket_path, query_status, quit as qmp_quit,
    secure_boot_vars_path, stop as qmp_stop, suspend_to_snapshot, swtpm_socket_path, QemuCommand,
    QemuError, COMPAT_SUSPEND_SNAPSHOT_TAG,
};
use bridgevm_storage::{
    ApplicationConsistentSnapshotPreflightMetadata, DiskCompactMetadata, DiskCreateMetadata,
    DiskInspectMetadata, DiskPreparationMetadata, DiskVerifyMetadata, GuestToolsMetricsMetadata,
    GuestToolsRuntimeMetadata, LaunchReadinessBlockerMetadata, LaunchReadinessMetadata,
    QmpSupervisorMetadata, RunnerMetadata, RuntimeControlMetadata, RuntimeResourcePolicyBlocker,
    RuntimeResourcePolicyMetadata, RuntimeResourceVisibility, SnapshotChainMetadata,
    SnapshotDiskCreateMetadata, SnapshotDiskMetadata, SnapshotKind, SnapshotMetadata,
    SnapshotRestoreMetadata, VmCloneMetadata, VmDeletionMetadata, VmExportMetadata,
    VmImportMetadata, VmManifestMigrationMetadata, VmMetadataRepairMetadata, VmRuntimeMetadata,
    VmRuntimeState, VmStore,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sha2::{Digest, Sha256};
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::{
    fs::{self, OpenOptions},
    io::{BufRead, BufReader, Read, Seek, SeekFrom, Write},
    net::IpAddr,
    os::unix::net::UnixStream,
    path::{Component, Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const BOOT_MEDIA_CURL_CONNECT_TIMEOUT_SECS: u64 = 30;
const BOOT_MEDIA_CURL_MAX_TIME_SECS: u64 = 6 * 60 * 60;
const BOOT_MEDIA_CURL_SPEED_TIME_SECS: u64 = 5 * 60;
const BOOT_MEDIA_CURL_SPEED_LIMIT_BYTES: u64 = 1024;
const BOOT_MEDIA_CURL_OUTPUT_BYTES: usize = 64 * 1024;

const DEFAULT_GUEST_TOOLS_LINUX_DEVICE: &str = "/dev/virtio-ports/org.bridgevm.guest-tools.0";
const DEFAULT_PERFORMANCE_SAMPLE_ARTIFACT_BYTES: u64 = 1_048_576;
const MAX_PERFORMANCE_SAMPLE_ARTIFACT_BYTES: u64 = 64 * 1024 * 1024;
const DEFAULT_PERFORMANCE_SAMPLE_ITERATIONS: u16 = 1;
const MAX_PERFORMANCE_SAMPLE_ITERATIONS: u16 = 100;
const MAX_PERFORMANCE_SAMPLE_TOTAL_BYTES: u64 = 256 * 1024 * 1024;
const DEFAULT_LOG_VIEW_BYTES: u64 = 8 * 1024;
const MAX_LOG_VIEW_BYTES: u64 = 1024 * 1024;
const MAX_BOOT_MEDIA_METADATA_BYTES: u64 = 1024 * 1024;
const MAX_EVIDENCE_TEXT_BYTES: u64 = 16 * 1024 * 1024;
const MAX_DIAGNOSTIC_FILE_BYTES: u64 = 16 * 1024 * 1024;
const MAX_RUNTIME_CONTROL_RESPONSE_BYTES: u64 = 64 * 1024;

pub const BRIDGEVM_API_SCHEMA_ID: &str = "bridgevm.api/v1";
pub const BRIDGEVM_API_CONTRACT_VERSION: u16 = 1;
pub const BRIDGEVM_API_SERVICE_NAME: &str = "bridgevm.api.v1.BridgeVmService";
pub const BRIDGEVM_API_JSON_OVER_UDS_TRANSPORT: &str = "json-ndjson-over-uds";
pub const BRIDGEVM_API_GRPC_OVER_UDS_TRANSPORT: &str = "grpc-over-uds";

mod boot_media;
mod boot_media_meta;
#[cfg(test)]
mod test_support;

mod contract;
mod diagnostics;
mod dispatch;
mod evidence_read;
mod evidence_util;
mod evidence_verify;
mod guest_tools;
mod launch_readiness;
mod network;
mod performance;
mod process;
mod readiness;
mod records;
mod request;
mod response;
mod shares;
mod ssh_ports;

pub use boot_media::*;
pub(crate) use boot_media_meta::*;
pub use contract::*;
pub use diagnostics::*;
pub use dispatch::*;
pub(crate) use evidence_read::*;
pub(crate) use evidence_util::*;
pub(crate) use evidence_verify::*;
pub use guest_tools::*;
pub use launch_readiness::*;
pub use network::*;
pub use performance::*;
pub use process::*;
pub use readiness::*;
pub use records::*;
pub use request::*;
pub use response::*;
pub use shares::*;
pub use ssh_ports::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::*;

    #[test]
    fn service_contract_marks_json_and_grpc_uds_migration_boundary() {
        let current = BridgeVmServiceContract::json_over_uds();
        let target = BridgeVmServiceContract::grpc_over_uds();

        assert!(current.is_same_contract_as(&target));
        assert_eq!(current.schema_id, BRIDGEVM_API_SCHEMA_ID);
        assert_eq!(current.version, BRIDGEVM_API_CONTRACT_VERSION);
        assert_eq!(current.service, BRIDGEVM_API_SERVICE_NAME);
        assert_eq!(current.transport, BRIDGEVM_API_JSON_OVER_UDS_TRANSPORT);
        assert_eq!(target.transport, BRIDGEVM_API_GRPC_OVER_UDS_TRANSPORT);
    }

    #[test]
    fn evidence_text_reader_rejects_oversized_files() {
        let path = std::env::temp_dir().join(format!(
            "bridgevm-api-oversized-evidence-text-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::write(&path, vec![b'x'; MAX_EVIDENCE_TEXT_BYTES as usize + 1]).unwrap();

        let error = read_bounded_text_file(&path, "test evidence")
            .expect_err("oversized evidence must be rejected");
        assert!(error.contains("exceeds the 16777216-byte limit"));
        assert!(error.contains(&path.display().to_string()));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn handler_rejects_invalid_performance_sample_bounds_before_writing() {
        let mut root = std::env::temp_dir();
        root.push(format!(
            "bridgevm-api-performance-sample-bounds-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = VmStore::new(root);
        let manifest = VmManifest::new(
            "dev",
            VmMode::Fast,
            Guest {
                os: "ubuntu".to_string(),
                version: None,
                arch: "arm64".to_string(),
            },
            "80GiB",
        );
        handle_request(&store, BridgeVmRequest::create_vm(manifest))
            .into_result()
            .unwrap();

        let output = store.root().join("performance-sample-invalid-output");
        let cases = [
            (
                Some(4096),
                Some(0),
                "performance sample iterations must be greater than zero",
            ),
            (
                Some(4096),
                Some(MAX_PERFORMANCE_SAMPLE_ITERATIONS + 1),
                "performance sample iterations is too large",
            ),
            (
                Some(MAX_PERFORMANCE_SAMPLE_ARTIFACT_BYTES + 1),
                Some(1),
                "performance sample artifact is too large",
            ),
            (
                Some(MAX_PERFORMANCE_SAMPLE_ARTIFACT_BYTES),
                Some(5),
                "performance sample total artifact bytes is too large",
            ),
        ];

        for (artifact_bytes, iterations, expected) in cases {
            let response = handle_request(
                &store,
                BridgeVmRequest::CreatePerformanceSample {
                    name: "dev".to_string(),
                    output: output.clone(),
                    artifact_bytes,
                    iterations,
                    sync: false,
                },
            );
            let error = response.into_result().unwrap_err();
            assert!(
                error.contains(expected),
                "expected {expected:?} in {error:?}"
            );
        }

        assert!(
            !output.exists(),
            "invalid sample bounds should not create an output directory"
        );
    }

    #[test]
    fn runtime_control_reader_rejects_oversized_response() {
        let socket_path = unique_runtime_control_test_socket("oversized");
        let listener = std::os::unix::net::UnixListener::bind(&socket_path).unwrap();
        let server = std::thread::spawn({
            let socket_path = socket_path.clone();
            move || {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = String::new();
                BufReader::new(stream.try_clone().unwrap())
                    .read_line(&mut request)
                    .unwrap();
                let oversized = vec![b'x'; MAX_RUNTIME_CONTROL_RESPONSE_BYTES as usize + 1];
                let _ = stream.write_all(&oversized);
                drop(stream);
                let _ = fs::remove_file(socket_path);
            }
        });

        let error = send_runtime_control_command(&socket_path, "status").unwrap_err();
        assert!(error.contains("exceeded 65536 bytes"), "{error}");
        server.join().unwrap();
    }
}
