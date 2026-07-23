//! Split out of lib.rs by responsibility.

use crate::*;

impl BridgeVmResponse {
    pub fn into_result(self) -> Result<Self, String> {
        match self {
            BridgeVmResponse::Error { message } => Err(message),
            response => Ok(response),
        }
    }
}

pub fn handle_request(store: &VmStore, request: BridgeVmRequest) -> BridgeVmResponse {
    match handle_request_result(store, request) {
        Ok(response) => response,
        Err(message) => BridgeVmResponse::Error { message },
    }
}

pub(crate) fn handle_request_result(
    store: &VmStore,
    request: BridgeVmRequest,
) -> Result<BridgeVmResponse, String> {
    match request {
        BridgeVmRequest::Doctor => {
            store.ensure().map_err(|error| error.to_string())?;
            Ok(BridgeVmResponse::Doctor {
                store_root: store.root().to_path_buf(),
                vms_dir: store.vms_dir(),
                status: "OK".to_string(),
            })
        }
        BridgeVmRequest::ListTemplates => Ok(BridgeVmResponse::BootTemplates {
            templates: available_boot_templates(),
        }),
        BridgeVmRequest::ListVms => Ok(BridgeVmResponse::VmList {
            vms: records(store).map_err(|error| error.to_string())?,
        }),
        BridgeVmRequest::CreateVm { manifest } => {
            store
                .create_vm(&manifest)
                .map_err(|error| error.to_string())?;
            Ok(BridgeVmResponse::Vm {
                vm: record_for(store, &manifest.name).map_err(|error| error.to_string())?,
            })
        }
        BridgeVmRequest::CreateVmFromTemplate { name, template_id } => {
            let manifest = manifest_from_template(name, &template_id)?;
            store
                .create_vm(&manifest)
                .map_err(|error| error.to_string())?;
            Ok(BridgeVmResponse::Vm {
                vm: record_for(store, &manifest.name).map_err(|error| error.to_string())?,
            })
        }
        BridgeVmRequest::GetVm { name } => Ok(BridgeVmResponse::Vm {
            vm: record_for(store, &name).map_err(|error| error.to_string())?,
        }),
        BridgeVmRequest::DeleteVm {
            name,
            metadata_only,
        } => {
            let state = store.state(&name).map_err(|error| error.to_string())?;
            if state.state == VmRuntimeState::Running {
                return Err("refusing to delete a running VM; stop it first".to_string());
            }
            if metadata_only {
                let metadata = store
                    .delete_vm_metadata_only(&name)
                    .map_err(|error| error.to_string())?;
                return Ok(BridgeVmResponse::Deleted {
                    path: metadata.bundle.clone(),
                    metadata_only: true,
                    metadata: Some(metadata),
                });
            }
            Ok(BridgeVmResponse::Deleted {
                path: store.delete_vm(&name).map_err(|error| error.to_string())?,
                metadata_only: false,
                metadata: None,
            })
        }
        BridgeVmRequest::ExportVm { name, output } => Ok(BridgeVmResponse::Exported {
            export: store
                .export_vm(&name, output)
                .map_err(|error| error.to_string())?,
        }),
        BridgeVmRequest::ImportVm { input, name } => Ok(BridgeVmResponse::Imported {
            import: store
                .import_vm(input, name.as_deref())
                .map_err(|error| error.to_string())?,
        }),
        BridgeVmRequest::CloneVm {
            name,
            new_name,
            linked,
        } => Ok(BridgeVmResponse::Cloned {
            clone: store
                .clone_vm(&name, &new_name, linked)
                .map_err(|error| error.to_string())?,
        }),
        BridgeVmRequest::RepairMetadata { name } => Ok(BridgeVmResponse::MetadataRepaired {
            repair: store
                .repair_metadata(&name)
                .map_err(|error| error.to_string())?,
        }),
        BridgeVmRequest::MigrateManifest { name, dry_run } => {
            Ok(BridgeVmResponse::ManifestMigrated {
                migration: store
                    .migrate_manifest(&name, dry_run)
                    .map_err(|error| error.to_string())?,
            })
        }
        BridgeVmRequest::CreateDiagnosticBundle { name, output } => {
            Ok(BridgeVmResponse::DiagnosticBundle {
                bundle: create_diagnostic_bundle(store, &name, output)?,
            })
        }
        BridgeVmRequest::ViewLogs {
            name,
            kind,
            max_bytes,
        } => Ok(BridgeVmResponse::LogsViewed {
            log: view_vm_log(store, &name, kind, max_bytes)?,
        }),
        BridgeVmRequest::CreatePerformanceBaseline { name, output } => {
            Ok(BridgeVmResponse::PerformanceBaseline {
                baseline: create_performance_baseline(store, &name, output)?,
            })
        }
        BridgeVmRequest::CreatePerformanceSample {
            name,
            output,
            artifact_bytes,
            iterations,
            sync,
        } => Ok(BridgeVmResponse::PerformanceSample {
            sample: create_performance_sample(
                store,
                &name,
                output,
                artifact_bytes,
                iterations,
                sync,
            )?,
        }),
        BridgeVmRequest::ReadinessReport {
            name,
            live_evidence,
            record_live_evidence,
            clear_live_evidence,
        } => Ok(BridgeVmResponse::ReadinessReport {
            report: readiness_report_with_live_evidence_options(
                store,
                &name,
                live_evidence.as_deref(),
                record_live_evidence,
                clear_live_evidence,
            )?,
        }),
        BridgeVmRequest::TransitionVm { name, state } => Ok(BridgeVmResponse::State {
            name: name.clone(),
            metadata: store
                .transition_state(&name, state)
                .map_err(|error| error.to_string())?,
        }),
        BridgeVmRequest::RestartVm { name } => Ok(BridgeVmResponse::State {
            name: name.clone(),
            metadata: restart_vm(store, &name)?,
        }),
        BridgeVmRequest::CreateSnapshot { vm, name, kind } => {
            let snapshot = store
                .create_snapshot(&vm, &name, kind)
                .map_err(|error| error.to_string())?;
            let disk = store
                .snapshot_disk_metadata(&vm, &name)
                .map_err(|error| error.to_string())?;
            let application_consistent_preflight = store
                .application_consistent_snapshot_preflight_metadata(&vm, &name)
                .map_err(|error| error.to_string())?;
            Ok(BridgeVmResponse::Snapshot {
                snapshot,
                disk,
                application_consistent_preflight,
            })
        }
        BridgeVmRequest::ListSnapshots { vm } => Ok(BridgeVmResponse::SnapshotList {
            snapshots: store.snapshots(&vm).map_err(|error| error.to_string())?,
        }),
        BridgeVmRequest::SnapshotChain { vm } => Ok(BridgeVmResponse::SnapshotChain {
            chain: store
                .snapshot_chain(&vm)
                .map_err(|error| error.to_string())?,
        }),
        BridgeVmRequest::SnapshotPreflightStatus { name, consistency } => {
            Ok(BridgeVmResponse::SnapshotPreflightStatus {
                preflight: snapshot_preflight_status(store, &name, consistency)
                    .map_err(|error| error.to_string())?,
            })
        }
        BridgeVmRequest::ExecuteApplicationConsistentSnapshot { .. } => Err(
            "application-consistent snapshot execution requires a bridgevmd-owned running backend"
                .to_string(),
        ),
        BridgeVmRequest::RestoreSnapshot { vm, name } => Ok(BridgeVmResponse::SnapshotRestored {
            restore: store
                .restore_snapshot(&vm, &name)
                .map_err(|error| error.to_string())?,
        }),
        BridgeVmRequest::CreateSnapshotDisk { vm, name } => {
            Ok(BridgeVmResponse::SnapshotDiskCreated {
                metadata: store
                    .create_snapshot_disk(&vm, &name)
                    .map_err(|error| error.to_string())?,
            })
        }
        BridgeVmRequest::QemuArgs { name } => {
            let (bundle, manifest, _) = store
                .get_vm_with_active_disk(&name)
                .map_err(|error| error.to_string())?;
            Ok(BridgeVmResponse::QemuCommand {
                command: build_compatibility_command(&manifest, &bundle)
                    .map_err(compatibility_qemu_command_error)?,
            })
        }
        BridgeVmRequest::PrepareRun { name } => Ok(BridgeVmResponse::RunnerStatus {
            metadata: Some(run_backend(store, &name, false)?),
            qmp_supervisor: store
                .qmp_supervisor_metadata(&name)
                .map_err(|error| error.to_string())?,
        }),
        BridgeVmRequest::InspectBootMedia { name } => {
            let (bundle, manifest, _) = store
                .get_vm_with_active_disk(&name)
                .map_err(|error| error.to_string())?;
            let plan = build_fast_plan(&manifest, &bundle).map_err(|error| error.to_string())?;
            Ok(BridgeVmResponse::BootMedia {
                name,
                boot: plan.launch_spec().boot.clone(),
            })
        }
        BridgeVmRequest::ImportBootMedia { name, source, kind } => {
            Ok(BridgeVmResponse::BootMediaImported {
                import: import_boot_media(store, &name, source, kind)?,
            })
        }
        BridgeVmRequest::InspectBootMediaStatus { name } => Ok(BridgeVmResponse::BootMediaStatus {
            status: inspect_boot_media_status(store, &name)?,
        }),
        BridgeVmRequest::VerifyBootMedia {
            name,
            expected_sha256,
            kind,
        } => Ok(BridgeVmResponse::BootMediaVerified {
            verification: verify_boot_media(store, &name, &expected_sha256, kind)?,
        }),
        BridgeVmRequest::PlanBootMediaDownload {
            name,
            url,
            expected_sha256,
            kind,
        } => Ok(BridgeVmResponse::BootMediaDownloadPlanned {
            plan: plan_boot_media_download(store, &name, &url, expected_sha256.as_deref(), kind)?,
        }),
        BridgeVmRequest::DownloadBootMedia { name, kind } => {
            Ok(BridgeVmResponse::BootMediaDownloaded {
                download: download_boot_media(store, &name, kind)?,
            })
        }
        BridgeVmRequest::PrepareDisk { name } => Ok(BridgeVmResponse::DiskPrepared {
            metadata: store
                .prepare_primary_disk(&name)
                .map_err(|error| error.to_string())?,
        }),
        BridgeVmRequest::CreateDisk { name } => Ok(BridgeVmResponse::DiskCreated {
            metadata: store
                .create_primary_disk(&name)
                .map_err(|error| error.to_string())?,
        }),
        BridgeVmRequest::InspectDisk { name } => Ok(BridgeVmResponse::DiskInspected {
            metadata: store
                .inspect_primary_disk(&name)
                .map_err(|error| error.to_string())?,
        }),
        BridgeVmRequest::VerifyDisk { name } => Ok(BridgeVmResponse::DiskVerified {
            metadata: store
                .verify_active_disk(&name)
                .map_err(|error| error.to_string())?,
        }),
        BridgeVmRequest::CompactDisk { name } => Ok(BridgeVmResponse::DiskCompacted {
            metadata: store
                .compact_active_disk(&name)
                .map_err(|error| error.to_string())?,
        }),
        BridgeVmRequest::ListPorts { name } => Ok(BridgeVmResponse::PortForwards {
            ports: list_ports(store, &name)?,
        }),
        BridgeVmRequest::AddPort { name, host, guest } => Ok(BridgeVmResponse::PortForwards {
            ports: add_port(store, &name, host, guest)?,
        }),
        BridgeVmRequest::RemovePort { name, host, guest } => Ok(BridgeVmResponse::PortForwards {
            ports: remove_port(store, &name, host, guest)?,
        }),
        BridgeVmRequest::PlanNetwork { name } => Ok(BridgeVmResponse::NetworkPlanned {
            plan: network_plan(store, &name)?,
        }),
        BridgeVmRequest::ListShares { name } => Ok(BridgeVmResponse::SharedFolders {
            shares: list_shares(store, &name)?,
        }),
        BridgeVmRequest::AddShare {
            name,
            share,
            host_path,
            read_only,
            host_path_token,
        } => Ok(BridgeVmResponse::SharedFolders {
            shares: add_share(store, &name, share, host_path, read_only, host_path_token)?,
        }),
        BridgeVmRequest::RemoveShare { name, share } => Ok(BridgeVmResponse::SharedFolders {
            shares: remove_share(store, &name, &share)?,
        }),
        BridgeVmRequest::SshPlan { name, user } => Ok(BridgeVmResponse::SshPlan {
            plan: ssh_plan(store, &name, user.as_deref())?,
        }),
        BridgeVmRequest::OpenPort {
            name,
            guest,
            scheme,
        } => Ok(BridgeVmResponse::OpenPortPlan {
            plan: open_port_plan(store, &name, guest, scheme.as_deref())?,
        }),
        BridgeVmRequest::RunBackend { name, spawn } => Ok(BridgeVmResponse::RunnerStatus {
            metadata: Some(run_backend(store, &name, spawn)?),
            qmp_supervisor: store
                .qmp_supervisor_metadata(&name)
                .map_err(|error| error.to_string())?,
        }),
        BridgeVmRequest::SuspendBackend { name } => Ok(BridgeVmResponse::RunnerStatus {
            metadata: Some(suspend_backend(store, &name)?),
            qmp_supervisor: store
                .qmp_supervisor_metadata(&name)
                .map_err(|error| error.to_string())?,
        }),
        BridgeVmRequest::ResumeBackend { name } => Ok(BridgeVmResponse::RunnerStatus {
            metadata: Some(resume_backend(store, &name)?),
            qmp_supervisor: store
                .qmp_supervisor_metadata(&name)
                .map_err(|error| error.to_string())?,
        }),
        BridgeVmRequest::LifecyclePlan { name, action } => Ok(BridgeVmResponse::LifecyclePlan {
            plan: lifecycle_plan(store, &name, action)?,
        }),
        BridgeVmRequest::ReapplyRuntimeResources { name, visibility } => {
            Ok(BridgeVmResponse::RuntimeResourcePolicy {
                policy: reapply_runtime_resources(store, &name, visibility)?,
            })
        }
        BridgeVmRequest::StopBackend { name } => Ok(BridgeVmResponse::RunnerStatus {
            metadata: stop_backend(store, &name)?,
            qmp_supervisor: store
                .qmp_supervisor_metadata(&name)
                .map_err(|error| error.to_string())?,
        }),
        BridgeVmRequest::RunnerStatus { name } => Ok(BridgeVmResponse::RunnerStatus {
            metadata: store
                .runner_metadata(&name)
                .map_err(|error| error.to_string())?,
            qmp_supervisor: store
                .qmp_supervisor_metadata(&name)
                .map_err(|error| error.to_string())?,
        }),
        BridgeVmRequest::RuntimeControl { name, command } => Ok(BridgeVmResponse::RuntimeControl {
            control: runtime_control_command(store, &name, &command)?,
        }),
        BridgeVmRequest::QmpSocket { name } => {
            let (bundle, _) = store.get_vm(&name).map_err(|error| error.to_string())?;
            Ok(BridgeVmResponse::QmpSocket {
                path: qmp_socket_path(&bundle),
            })
        }
        BridgeVmRequest::QmpStatus { name } => {
            let (bundle, _) = store.get_vm(&name).map_err(|error| error.to_string())?;
            let socket_path = qmp_socket_path(&bundle);
            let supervisor = store
                .qmp_supervisor_metadata(&name)
                .map_err(|error| error.to_string())?;
            if !socket_path.exists() {
                return Ok(BridgeVmResponse::QmpStatus {
                    status: QmpStatusRecord {
                        socket_path,
                        available: false,
                        status: None,
                        running: None,
                        supervisor,
                    },
                });
            }
            let status = match query_status(&socket_path) {
                Ok(status) => status,
                Err(error) if is_qmp_status_unavailable(&error) => {
                    return Ok(BridgeVmResponse::QmpStatus {
                        status: QmpStatusRecord {
                            socket_path,
                            available: false,
                            status: None,
                            running: None,
                            supervisor,
                        },
                    });
                }
                Err(error) => return Err(error.to_string()),
            };
            Ok(BridgeVmResponse::QmpStatus {
                status: QmpStatusRecord {
                    socket_path,
                    available: true,
                    status: Some(status.status),
                    running: Some(status.running),
                    supervisor,
                },
            })
        }
        BridgeVmRequest::QmpStop { name } => {
            let command = execute_qmp_control(store, &name, "stop", qmp_stop)?;
            Ok(BridgeVmResponse::QmpCommandExecuted { command })
        }
        BridgeVmRequest::QmpCont { name } => {
            let command = execute_qmp_control(store, &name, "cont", qmp_cont)?;
            Ok(BridgeVmResponse::QmpCommandExecuted { command })
        }
        BridgeVmRequest::GuestToolsStatus { name } => Ok(BridgeVmResponse::GuestToolsStatus {
            status: inspect_guest_tools_status(store, &name)?,
        }),
        BridgeVmRequest::GuestToolsToken { name } => Ok(BridgeVmResponse::GuestToolsToken {
            token: guest_tools_token(store, &name)?,
        }),
        BridgeVmRequest::GuestToolsAcceptHello { name, envelope } => {
            Ok(BridgeVmResponse::GuestToolsSession {
                session: accept_guest_tools_hello(store, &name, &envelope)?,
            })
        }
        BridgeVmRequest::GuestToolsLinuxCommand {
            name,
            transport,
            token_file,
            device,
        } => Ok(BridgeVmResponse::GuestToolsLinuxCommand {
            command: guest_tools_linux_command(store, &name, transport, token_file, device)?,
        }),
        BridgeVmRequest::GuestToolsSendCommand { .. }
        | BridgeVmRequest::GuestToolsMountApprovedShare { .. } => Err(
            "guest-tools command dispatch requires a bridgevmd-owned running backend".to_string(),
        ),
        BridgeVmRequest::RecommendMode { choice } => Ok(BridgeVmResponse::ModeRecommendation {
            recommendation: recommend_mode(&choice),
        }),
    }
}

pub(crate) fn manifest_from_template(
    name: String,
    template_id: &str,
) -> Result<VmManifest, String> {
    let template = boot_template_by_id(template_id)
        .ok_or_else(|| format!("unknown template id: {template_id}"))?;
    let choice = GuestChoice {
        os: template.guest_os.clone(),
        version: template.guest_version.clone(),
        arch: template.guest_arch.clone(),
    };
    let recommendation = recommend_mode(&choice);
    let mut manifest = VmManifest::new(
        name,
        recommendation.mode,
        Guest {
            os: template.guest_os.clone(),
            version: template.guest_version.clone(),
            arch: template.guest_arch.clone(),
        },
        template.primary_disk_size().unwrap_or("80GiB"),
    );
    template.apply_storage_defaults(&mut manifest.storage.primary);
    manifest.boot = Some(template.as_boot());
    Ok(manifest)
}
