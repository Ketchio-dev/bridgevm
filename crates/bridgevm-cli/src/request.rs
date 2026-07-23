//! Split out of main.rs by responsibility.

use crate::*;

pub(crate) fn request_for(command: Command) -> Result<BridgeVmRequest> {
    match command {
        Command::List => Ok(BridgeVmRequest::ListVms),
        Command::Templates => Ok(BridgeVmRequest::ListTemplates),
        Command::Create(args) => request_for_create(args),
        Command::Status(args) => Ok(BridgeVmRequest::GetVm { name: args.name }),
        Command::Start(args) => Ok(BridgeVmRequest::TransitionVm {
            name: args.name,
            state: VmRuntimeState::Running,
        }),
        Command::Stop(args) => Ok(BridgeVmRequest::StopBackend { name: args.name }),
        Command::Restart(args) => Ok(BridgeVmRequest::RestartVm { name: args.name }),
        Command::Suspend(args) => Ok(BridgeVmRequest::SuspendBackend { name: args.name }),
        Command::Resume(args) => Ok(BridgeVmRequest::ResumeBackend { name: args.name }),
        Command::Delete(args) => Ok(BridgeVmRequest::DeleteVm {
            name: args.name,
            metadata_only: args.metadata_only,
        }),
        Command::Export(args) => Ok(BridgeVmRequest::ExportVm {
            name: args.name,
            output: args.output,
        }),
        Command::Import(args) => Ok(BridgeVmRequest::ImportVm {
            input: args.input,
            name: args.name,
        }),
        Command::Clone(args) => Ok(BridgeVmRequest::CloneVm {
            name: args.name,
            new_name: args.new_name,
            linked: args.linked,
        }),
        Command::Diagnostics(args) => match args.command {
            DiagnosticsSubcommand::Bundle(args) => Ok(BridgeVmRequest::CreateDiagnosticBundle {
                name: args.vm,
                output: args.output,
            }),
        },
        Command::Logs(args) => match args.command {
            LogsSubcommand::Qemu(args) => Ok(BridgeVmRequest::ViewLogs {
                name: args.vm,
                kind: VmLogKind::Qemu,
                max_bytes: args.bytes,
            }),
            LogsSubcommand::Serial(args) => Ok(BridgeVmRequest::ViewLogs {
                name: args.vm,
                kind: VmLogKind::Serial,
                max_bytes: args.bytes,
            }),
        },
        Command::Performance(args) => match args.command {
            PerformanceSubcommand::Baseline(args) => {
                Ok(BridgeVmRequest::CreatePerformanceBaseline {
                    name: args.vm,
                    output: args.output,
                })
            }
            PerformanceSubcommand::Sample(args) => Ok(BridgeVmRequest::CreatePerformanceSample {
                name: args.vm,
                output: args.output,
                artifact_bytes: args.artifact_bytes,
                iterations: args.iterations,
                sync: args.sync,
            }),
        },
        Command::Metadata(args) => match args.command {
            MetadataSubcommand::Repair(args) => {
                Ok(BridgeVmRequest::RepairMetadata { name: args.name })
            }
            MetadataSubcommand::MigrateManifest(args) => Ok(BridgeVmRequest::MigrateManifest {
                name: args.name,
                dry_run: args.dry_run,
            }),
            MetadataSubcommand::ManifestSchema | MetadataSubcommand::ValidateManifest(_) => {
                bail!("metadata manifest-schema and validate-manifest are local-only commands")
            }
        },
        Command::Snapshot(args) => match args.command {
            SnapshotSubcommand::Create(args) => Ok(BridgeVmRequest::CreateSnapshot {
                vm: args.vm,
                name: args.name,
                kind: args.kind.into(),
            }),
            SnapshotSubcommand::ExecuteApplicationConsistent(args) => {
                Ok(BridgeVmRequest::ExecuteApplicationConsistentSnapshot {
                    vm: args.vm,
                    name: args.name,
                    freeze_timeout_millis: args.freeze_timeout_millis,
                })
            }
            SnapshotSubcommand::List(args) => Ok(BridgeVmRequest::ListSnapshots { vm: args.name }),
            SnapshotSubcommand::Chain(args) => Ok(BridgeVmRequest::SnapshotChain { vm: args.name }),
            SnapshotSubcommand::Restore(args) => Ok(BridgeVmRequest::RestoreSnapshot {
                vm: args.vm,
                name: args.name,
            }),
            SnapshotSubcommand::DiskCreate(args) => Ok(BridgeVmRequest::CreateSnapshotDisk {
                vm: args.vm,
                name: args.name,
            }),
        },
        Command::Disk(args) => match args.command {
            DiskSubcommand::Prepare(args) => Ok(BridgeVmRequest::PrepareDisk { name: args.name }),
            DiskSubcommand::Create(args) => Ok(BridgeVmRequest::CreateDisk { name: args.name }),
            DiskSubcommand::Inspect(args) => Ok(BridgeVmRequest::InspectDisk { name: args.name }),
            DiskSubcommand::Verify(args) => Ok(BridgeVmRequest::VerifyDisk { name: args.name }),
            DiskSubcommand::Compact(args) => Ok(BridgeVmRequest::CompactDisk { name: args.name }),
        },
        Command::Port(args) => match args.command {
            PortSubcommand::List(args) => Ok(BridgeVmRequest::ListPorts { name: args.name }),
            PortSubcommand::Add(args) => {
                let (host, guest) = parse_port_mapping(&args.mapping)?;
                Ok(BridgeVmRequest::AddPort {
                    name: args.vm,
                    host,
                    guest,
                })
            }
            PortSubcommand::Remove(args) => {
                let (host, guest) = parse_port_mapping(&args.mapping)?;
                Ok(BridgeVmRequest::RemovePort {
                    name: args.vm,
                    host,
                    guest,
                })
            }
        },
        Command::NetworkPlan(args) => Ok(BridgeVmRequest::PlanNetwork { name: args.name }),
        Command::Share(args) => match args.command {
            ShareSubcommand::List(args) => Ok(BridgeVmRequest::ListShares { name: args.name }),
            ShareSubcommand::Add(args) => Ok(BridgeVmRequest::AddShare {
                name: args.vm,
                share: args.name,
                host_path: args.host_path,
                read_only: args.read_only,
                host_path_token: args.host_path_token,
            }),
            ShareSubcommand::Remove(args) => Ok(BridgeVmRequest::RemoveShare {
                name: args.vm,
                share: args.name,
            }),
        },
        Command::Media(args) => match args.command {
            MediaSubcommand::Download(args) => Ok(BridgeVmRequest::DownloadBootMedia {
                name: args.vm,
                kind: args.kind.map(Into::into),
            }),
            MediaSubcommand::DownloadPlan(args) => Ok(BridgeVmRequest::PlanBootMediaDownload {
                name: args.vm,
                url: args.url,
                expected_sha256: args.sha256,
                kind: args.kind.map(Into::into),
            }),
            MediaSubcommand::Import(args) => Ok(BridgeVmRequest::ImportBootMedia {
                name: args.vm,
                source: args.source,
                kind: args.kind.map(Into::into),
            }),
            MediaSubcommand::Status(args) => {
                Ok(BridgeVmRequest::InspectBootMediaStatus { name: args.name })
            }
            MediaSubcommand::Verify(args) => Ok(BridgeVmRequest::VerifyBootMedia {
                name: args.vm,
                expected_sha256: args.sha256,
                kind: args.kind.map(Into::into),
            }),
        },
        Command::GuestTools(args) => match args.command {
            GuestToolsSubcommand::Status(args) => {
                Ok(BridgeVmRequest::GuestToolsStatus { name: args.name })
            }
            GuestToolsSubcommand::Token(args) => {
                Ok(BridgeVmRequest::GuestToolsToken { name: args.name })
            }
            GuestToolsSubcommand::LinuxCommand(args) => {
                Ok(BridgeVmRequest::GuestToolsLinuxCommand {
                    name: args.vm,
                    transport: args.transport.into(),
                    token_file: args.token_file,
                    device: args.device,
                })
            }
            GuestToolsSubcommand::AcceptHello(args) => Ok(BridgeVmRequest::GuestToolsAcceptHello {
                name: args.vm,
                envelope: parse_agent_envelope(&args.hello_json)?,
            }),
            GuestToolsSubcommand::SendCommand(args) => Ok(BridgeVmRequest::GuestToolsSendCommand {
                name: args.vm,
                envelope: parse_agent_envelope(&args.envelope_json)?,
            }),
            GuestToolsSubcommand::FreezeFilesystem(args) => {
                Ok(BridgeVmRequest::GuestToolsSendCommand {
                    name: args.vm,
                    envelope: agent_command_envelope(
                        AgentMessage::FreezeFilesystem {
                            timeout_millis: args.timeout_millis,
                        },
                        args.request_id,
                    ),
                })
            }
            GuestToolsSubcommand::ThawFilesystem(args) => {
                Ok(BridgeVmRequest::GuestToolsSendCommand {
                    name: args.vm,
                    envelope: agent_command_envelope(AgentMessage::ThawFilesystem, args.request_id),
                })
            }
            GuestToolsSubcommand::SetClipboard(args) => {
                Ok(BridgeVmRequest::GuestToolsSendCommand {
                    name: args.vm,
                    envelope: agent_command_envelope(
                        AgentMessage::SetClipboard { text: args.text },
                        args.request_id,
                    ),
                })
            }
            GuestToolsSubcommand::ResizeDisplay(args) => {
                Ok(BridgeVmRequest::GuestToolsSendCommand {
                    name: args.vm,
                    envelope: agent_command_envelope(
                        AgentMessage::ResizeDisplay {
                            width: args.width,
                            height: args.height,
                            scale: args.scale,
                        },
                        args.request_id,
                    ),
                })
            }
            GuestToolsSubcommand::MountShare(args) => Ok(BridgeVmRequest::GuestToolsSendCommand {
                name: args.vm,
                envelope: agent_command_envelope(
                    AgentMessage::MountShare {
                        name: args.name,
                        host_path_token: args.host_path_token,
                    },
                    args.request_id,
                ),
            }),
            GuestToolsSubcommand::MountApprovedShare(args) => {
                Ok(BridgeVmRequest::GuestToolsMountApprovedShare {
                    name: args.vm,
                    share: args.share,
                    request_id: args.request_id,
                })
            }
            GuestToolsSubcommand::UnmountShare(args) => {
                Ok(BridgeVmRequest::GuestToolsSendCommand {
                    name: args.vm,
                    envelope: agent_command_envelope(
                        AgentMessage::UnmountShare { name: args.name },
                        args.request_id,
                    ),
                })
            }
            GuestToolsSubcommand::FileDropStart(args) => {
                Ok(BridgeVmRequest::GuestToolsSendCommand {
                    name: args.vm,
                    envelope: agent_command_envelope(
                        AgentMessage::FileDropStart {
                            transfer_id: args.transfer_id,
                            file_name: args.file_name,
                            size_bytes: args.size_bytes,
                        },
                        args.request_id,
                    ),
                })
            }
            GuestToolsSubcommand::FileDropChunk(args) => {
                Ok(BridgeVmRequest::GuestToolsSendCommand {
                    name: args.vm,
                    envelope: agent_command_envelope(
                        AgentMessage::FileDropChunk {
                            transfer_id: args.transfer_id,
                            chunk_index: args.chunk_index,
                            data_base64: args.data_base64,
                        },
                        args.request_id,
                    ),
                })
            }
            GuestToolsSubcommand::FileDropComplete(args) => {
                Ok(BridgeVmRequest::GuestToolsSendCommand {
                    name: args.vm,
                    envelope: agent_command_envelope(
                        AgentMessage::FileDropComplete {
                            transfer_id: args.transfer_id,
                        },
                        args.request_id,
                    ),
                })
            }
            GuestToolsSubcommand::ListApplications(args) => {
                Ok(BridgeVmRequest::GuestToolsSendCommand {
                    name: args.vm,
                    envelope: agent_command_envelope(
                        AgentMessage::ListApplications,
                        args.request_id,
                    ),
                })
            }
            GuestToolsSubcommand::LaunchApplication(args) => {
                Ok(BridgeVmRequest::GuestToolsSendCommand {
                    name: args.vm,
                    envelope: agent_command_envelope(
                        AgentMessage::LaunchApplication { id: args.id },
                        args.request_id,
                    ),
                })
            }
            GuestToolsSubcommand::ListWindows(args) => Ok(BridgeVmRequest::GuestToolsSendCommand {
                name: args.vm,
                envelope: agent_command_envelope(AgentMessage::ListWindows, args.request_id),
            }),
            GuestToolsSubcommand::FocusWindow(args) => Ok(BridgeVmRequest::GuestToolsSendCommand {
                name: args.vm,
                envelope: agent_command_envelope(
                    AgentMessage::FocusWindow { id: args.id },
                    args.request_id,
                ),
            }),
            GuestToolsSubcommand::CloseWindow(args) => Ok(BridgeVmRequest::GuestToolsSendCommand {
                name: args.vm,
                envelope: agent_command_envelope(
                    AgentMessage::CloseWindow { id: args.id },
                    args.request_id,
                ),
            }),
            GuestToolsSubcommand::SetWindowBounds(args) => {
                Ok(BridgeVmRequest::GuestToolsSendCommand {
                    name: args.vm,
                    envelope: agent_command_envelope(
                        AgentMessage::SetWindowBounds {
                            id: args.id,
                            x: args.x,
                            y: args.y,
                            width: args.width,
                            height: args.height,
                        },
                        args.request_id,
                    ),
                })
            }
            GuestToolsSubcommand::WindowPointer(args) => {
                Ok(BridgeVmRequest::GuestToolsSendCommand {
                    name: args.vm,
                    envelope: agent_command_envelope(
                        AgentMessage::WindowInput {
                            id: args.id,
                            event: WindowInputEvent::Pointer {
                                x: args.x,
                                y: args.y,
                                action: args.action.as_protocol().to_string(),
                                button: args
                                    .button
                                    .map(|button| button.as_protocol().to_string()),
                            },
                        },
                        args.request_id,
                    ),
                })
            }
            GuestToolsSubcommand::WindowKey(args) => Ok(BridgeVmRequest::GuestToolsSendCommand {
                name: args.vm,
                envelope: agent_command_envelope(
                    AgentMessage::WindowInput {
                        id: args.id,
                        event: WindowInputEvent::Key {
                            key: args.key,
                            action: args.action.as_protocol().to_string(),
                        },
                    },
                    args.request_id,
                ),
            }),
            GuestToolsSubcommand::TimeSync(args) => Ok(BridgeVmRequest::GuestToolsSendCommand {
                name: args.vm,
                envelope: agent_command_envelope(
                    AgentMessage::TimeSync {
                        unix_epoch_millis: args
                            .unix_epoch_millis
                            .unwrap_or_else(current_unix_epoch_millis),
                    },
                    args.request_id,
                ),
            }),
        },
        Command::Resources(args) => match args {
            ResourcesCommand::Reapply(args) => Ok(BridgeVmRequest::ReapplyRuntimeResources {
                name: args.name,
                visibility: args.visibility.into(),
            }),
        },
        Command::RuntimeControl(args) => match args {
            RuntimeControlCommand::Status(args) => Ok(BridgeVmRequest::RuntimeControl {
                name: args.name,
                command: "status".to_string(),
            }),
            RuntimeControlCommand::Stop(args) => Ok(BridgeVmRequest::RuntimeControl {
                name: args.name,
                command: "stop".to_string(),
            }),
            RuntimeControlCommand::Policy(args) => Ok(BridgeVmRequest::RuntimeControl {
                name: args.name,
                command: "policy".to_string(),
            }),
            RuntimeControlCommand::Pacing(args) => Ok(BridgeVmRequest::RuntimeControl {
                name: args.name,
                command: "pacing".to_string(),
            }),
            RuntimeControlCommand::Reapply(args) => Ok(BridgeVmRequest::ReapplyRuntimeResources {
                name: args.name,
                visibility: args.visibility.into(),
            }),
        },
        Command::QemuArgs(args) => Ok(BridgeVmRequest::QemuArgs { name: args.name }),
        Command::PrepareRun(args) => Ok(BridgeVmRequest::PrepareRun { name: args.name }),
        Command::BootMedia(args) => Ok(BridgeVmRequest::InspectBootMedia { name: args.name }),
        Command::Ssh(args) => Ok(BridgeVmRequest::SshPlan {
            name: args.vm,
            user: Some(args.user),
        }),
        Command::Open(args) => Ok(BridgeVmRequest::OpenPort {
            name: args.vm,
            guest: args.guest,
            scheme: Some(args.scheme),
        }),
        Command::Run(args) => Ok(BridgeVmRequest::RunBackend {
            name: args.name,
            spawn: args.spawn,
        }),
        Command::Display(_) => Err(anyhow::anyhow!(
            "the embedded display window must run on the local GUI session; run `bridgevm display <vm>` with --store (not --socket)"
        )),
        Command::Readiness(args) => Ok(BridgeVmRequest::ReadinessReport {
            name: args.name,
            live_evidence: args.live_evidence,
            record_live_evidence: args.record_live_evidence,
            clear_live_evidence: args.clear_live_evidence,
        }),
        Command::LifecyclePlan(args) => Ok(BridgeVmRequest::LifecyclePlan {
            name: args.name,
            action: args.action.into(),
        }),
        Command::QmpSocket(args) => Ok(BridgeVmRequest::QmpSocket { name: args.name }),
        Command::QmpStatus(args) => Ok(BridgeVmRequest::QmpStatus { name: args.name }),
        Command::QmpStop(args) => Ok(BridgeVmRequest::QmpStop { name: args.name }),
        Command::QmpCont(args) => Ok(BridgeVmRequest::QmpCont { name: args.name }),
        Command::RunnerStatus(args) => Ok(BridgeVmRequest::RunnerStatus { name: args.name }),
        Command::Recommend(args) => Ok(BridgeVmRequest::RecommendMode {
            choice: GuestChoice {
                os: args.os,
                version: args.version,
                arch: args.arch,
            },
        }),
        Command::Hvf(_) => {
            bail!("hvf commands are local metadata-only commands; omit --socket")
        }
        Command::Store(StoreCommand::Doctor) => Ok(BridgeVmRequest::Doctor),
        Command::Doctor => Ok(BridgeVmRequest::Doctor),
    }
}

pub(crate) fn request_for_create(args: CreateArgs) -> Result<BridgeVmRequest> {
    if create_args_are_plain_template_request(&args) {
        return Ok(BridgeVmRequest::CreateVmFromTemplate {
            name: args.name,
            template_id: args.template.expect("plain template request has template"),
        });
    }

    let manifest = manifest_for_create(args)?;
    Ok(BridgeVmRequest::create_vm(manifest))
}

pub(crate) fn create_args_are_plain_template_request(args: &CreateArgs) -> bool {
    args.template.is_some()
        && args.os.is_none()
        && args.version.is_none()
        && args.arch.is_none()
        && args.mode == ModeChoice::Auto
        && args.disk.is_none()
        && args.disk_format.is_none()
        && args.boot_mode.is_none()
        && args.installer_image.is_none()
        && args.kernel_path.is_none()
        && args.initrd_path.is_none()
        && args.kernel_command_line.is_none()
        && args.macos_restore_image.is_none()
}

#[cfg(test)]
#[path = "request_tests/mod.rs"]
mod tests;
