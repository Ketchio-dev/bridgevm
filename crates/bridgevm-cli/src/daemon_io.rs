//! Split out of main.rs by responsibility.

use crate::*;

pub(crate) fn send_request(socket: &Path, request: BridgeVmRequest) -> Result<BridgeVmResponse> {
    let mut stream = UnixStream::connect(socket)
        .with_context(|| format!("failed to connect to daemon socket {}", socket.display()))?;
    stream
        .set_read_timeout(Some(DAEMON_IO_TIMEOUT))
        .context("failed to configure daemon response timeout")?;
    stream
        .set_write_timeout(Some(DAEMON_IO_TIMEOUT))
        .context("failed to configure daemon request timeout")?;
    serde_json::to_writer(&mut stream, &request).context("failed to write daemon request")?;
    stream.write_all(b"\n")?;

    let mut response_frame = Vec::new();
    BufReader::new(stream)
        .take(MAX_DAEMON_RESPONSE_BYTES + 1)
        .read_until(b'\n', &mut response_frame)
        .context("failed to read daemon response")?;
    if response_frame.is_empty() {
        bail!("daemon returned an empty response")
    }
    if response_frame.len() as u64 > MAX_DAEMON_RESPONSE_BYTES {
        bail!(
            "daemon response exceeded {} bytes",
            MAX_DAEMON_RESPONSE_BYTES
        )
    }
    if response_frame.last() != Some(&b'\n') {
        bail!("daemon returned an incomplete response frame")
    }
    let response = serde_json::from_slice::<BridgeVmResponse>(&response_frame)
        .context("invalid daemon response JSON")?;
    response.into_result().map_err(anyhow::Error::msg)
}

pub(crate) fn print_daemon_response(response: BridgeVmResponse) -> Result<()> {
    match response {
        BridgeVmResponse::Doctor {
            store_root,
            vms_dir,
            status,
        } => {
            println!("BridgeVM store: {}", store_root.display());
            println!("VM bundles: {}", vms_dir.display());
            print_doctor_audit(&doctor_audit_for_paths(&store_root, &vms_dir));
            print_engine_catalog(available_engine_descriptors());
            print_parallels_class_progress(&parallels_class_progress());
            println!("Status: {}", status);
        }
        BridgeVmResponse::VmList { vms } => {
            if vms.is_empty() {
                println!("No VMs found");
            } else {
                for vm in vms {
                    print_vm_record(&vm);
                }
            }
        }
        BridgeVmResponse::BootTemplates { templates } => print_boot_templates(&templates),
        BridgeVmResponse::Vm { vm } => print_vm_record(&vm),
        BridgeVmResponse::Deleted {
            path,
            metadata_only,
            metadata,
        } => {
            if metadata_only {
                if let Some(metadata) = metadata {
                    println!(
                        "Deleted VM metadata for {} at {} (bundle preserved: {})",
                        metadata.vm,
                        metadata.metadata_path.display(),
                        path.display()
                    );
                } else {
                    println!("Deleted VM metadata at {}", path.display());
                }
            } else {
                println!("Deleted VM bundle {}", path.display());
            }
        }
        BridgeVmResponse::Exported { export } => println!(
            "Exported {} from {} to {}",
            export.vm,
            export.source.display(),
            export.output.display()
        ),
        BridgeVmResponse::Imported { import } => println!(
            "Imported {} from {} to {}",
            import.vm,
            import.source.display(),
            import.output.display()
        ),
        BridgeVmResponse::Cloned { clone } => print_clone(&clone),
        BridgeVmResponse::DiagnosticBundle { bundle } => print_diagnostic_bundle(&bundle),
        BridgeVmResponse::LogsViewed { log } => print_vm_log(&log),
        BridgeVmResponse::PerformanceBaseline { baseline } => print_performance_baseline(&baseline),
        BridgeVmResponse::PerformanceSample { sample } => print_performance_sample(&sample),
        BridgeVmResponse::MetadataRepaired { repair } => print_metadata_repair(&repair),
        BridgeVmResponse::ManifestMigrated { migration } => print_manifest_migration(&migration),
        BridgeVmResponse::State { name, metadata } => {
            println!("Metadata state recorded for {} ({})", name, metadata.state);
        }
        BridgeVmResponse::Snapshot {
            snapshot,
            disk,
            application_consistent_preflight,
        } => {
            println!(
                "Created {} snapshot '{}' ({})",
                snapshot.kind, snapshot.name, snapshot.vm_state
            );
            if let Some(disk) = disk {
                print_snapshot_disk_status(&disk);
            }
            if let Some(preflight) = application_consistent_preflight {
                print_application_consistent_snapshot_preflight(&preflight);
            }
        }
        BridgeVmResponse::SnapshotList { snapshots } => {
            if snapshots.is_empty() {
                println!("No snapshots found");
            } else {
                for snapshot in snapshots {
                    println!(
                        "{}\t{}\t{}\t{}",
                        snapshot.name, snapshot.kind, snapshot.vm_state, snapshot.created_at_unix
                    );
                }
            }
        }
        BridgeVmResponse::SnapshotChain { chain } => print_snapshot_chain(&chain),
        BridgeVmResponse::SnapshotPreflightStatus { preflight } => {
            print_snapshot_preflight_status(&preflight)
        }
        BridgeVmResponse::ApplicationConsistentSnapshotExecution { execution } => {
            print_application_consistent_snapshot_execution(&execution)
        }
        BridgeVmResponse::SnapshotRestored { restore } => {
            println!(
                "Restored snapshot '{}' metadata; recorded state: {}",
                restore.snapshot, restore.restored_state
            );
            if let Some(active_disk) = restore.active_disk {
                print_active_disk(&active_disk);
            }
            if let Some(suspend_image) = restore.suspend_image {
                print_snapshot_suspend_image_status(&suspend_image);
            }
        }
        BridgeVmResponse::SnapshotDiskCreated { metadata } => {
            print_snapshot_disk_create_status(&metadata)
        }
        BridgeVmResponse::QemuCommand { command } => {
            for word in command.render_shell_words() {
                println!("{word}");
            }
        }
        BridgeVmResponse::DiskPrepared { metadata } => print_disk_status(&metadata),
        BridgeVmResponse::DiskCreated { metadata } => print_disk_create_status(&metadata),
        BridgeVmResponse::DiskInspected { metadata } => print_disk_inspect_status(&metadata),
        BridgeVmResponse::DiskVerified { metadata } => print_disk_verify_status(&metadata),
        BridgeVmResponse::DiskCompacted { metadata } => print_disk_compact_status(&metadata),
        BridgeVmResponse::PortForwards { ports } => print_port_forwards(&ports),
        BridgeVmResponse::NetworkPlanned { plan } => print_network_plan(&plan),
        BridgeVmResponse::SharedFolders { shares } => print_shared_folders(&shares),
        BridgeVmResponse::SshPlan { plan } => print_ssh_plan(&plan),
        BridgeVmResponse::OpenPortPlan { plan } => print_open_port_plan(&plan),
        BridgeVmResponse::RunnerStatus {
            metadata,
            qmp_supervisor,
        } => print_runner_status(metadata, qmp_supervisor.as_ref(), None),
        BridgeVmResponse::RuntimeControl { control } => print_runtime_control_command(&control)?,
        BridgeVmResponse::ReadinessReport { report } => print_readiness_report(&report),
        BridgeVmResponse::LifecyclePlan { plan } => print_lifecycle_plan(&plan),
        BridgeVmResponse::RuntimeResourcePolicy { policy } => {
            print_runtime_resource_policy(&policy)
        }
        BridgeVmResponse::BootMedia { name, boot } => print_boot_media(&name, &boot),
        BridgeVmResponse::BootMediaImported { import } => print_boot_media_import(&import),
        BridgeVmResponse::BootMediaStatus { status } => print_boot_media_status(&status),
        BridgeVmResponse::BootMediaVerified { verification } => {
            print_boot_media_verification(&verification)
        }
        BridgeVmResponse::BootMediaDownloadPlanned { plan } => {
            print_boot_media_download_plan(&plan)
        }
        BridgeVmResponse::BootMediaDownloaded { download } => print_boot_media_download(&download),
        BridgeVmResponse::QmpSocket { path } => println!("{}", path.display()),
        BridgeVmResponse::QmpStatus { status } => {
            if !status.available {
                println!("QMP socket unavailable: {}", status.socket_path.display());
            } else {
                println!(
                    "QMP status: {}",
                    status.status.unwrap_or_else(|| "unknown".to_string())
                );
                println!("Running: {}", status.running.unwrap_or(false));
            }
            if let Some(supervisor) = &status.supervisor {
                print_qmp_supervisor(supervisor);
            }
        }
        BridgeVmResponse::QmpCommandExecuted { command } => {
            println!("QMP command sent: {}", command.command);
            println!("VM: {}", command.vm);
            println!("QMP socket: {}", command.socket_path.display());
        }
        BridgeVmResponse::GuestToolsStatus { status } => print_guest_tools_status(&status),
        BridgeVmResponse::GuestToolsToken { token } => print_guest_tools_token(&token),
        BridgeVmResponse::GuestToolsSession { session } => print_guest_tools_session(&session),
        BridgeVmResponse::GuestToolsLinuxCommand { command } => {
            print_guest_tools_linux_command(&command)
        }
        BridgeVmResponse::GuestToolsCommand { command } => {
            println!("Guest tools command sent for {}", command.vm);
            println!(
                "Request ID: {}",
                command.request_id.as_deref().unwrap_or("none")
            );
            println!("Pending commands: {}", command.pending_commands);
        }
        BridgeVmResponse::ModeRecommendation { recommendation } => {
            print_mode_recommendation(&recommendation, None);
        }
        BridgeVmResponse::Error { message } => bail!(message),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_socket_path(prefix: &str) -> PathBuf {
        let mut path = PathBuf::from("/tmp");
        path.push(format!(
            "{prefix}-{}-{}.sock",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        path
    }

    #[test]
    fn daemon_client_rejects_oversized_response() {
        let socket_path = unique_socket_path("bridgevm-cli-oversized-response");
        let listener = std::os::unix::net::UnixListener::bind(&socket_path).unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = String::new();
            BufReader::new(stream.try_clone().unwrap())
                .read_line(&mut request)
                .unwrap();
            let oversized = vec![b'x'; MAX_DAEMON_RESPONSE_BYTES as usize + 1];
            let _ = stream.write_all(&oversized);
        });

        let error = send_request(&socket_path, BridgeVmRequest::Doctor).unwrap_err();
        assert!(error.to_string().contains("exceeded 16777216 bytes"));
        server.join().unwrap();
        let _ = fs::remove_file(socket_path);
    }

    #[test]
    fn daemon_client_rejects_incomplete_response_frame() {
        let socket_path = unique_socket_path("bridgevm-cli-incomplete-response");
        let listener = std::os::unix::net::UnixListener::bind(&socket_path).unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = String::new();
            BufReader::new(stream.try_clone().unwrap())
                .read_line(&mut request)
                .unwrap();
            stream.write_all(b"{}").unwrap();
        });

        let error = send_request(&socket_path, BridgeVmRequest::Doctor).unwrap_err();
        assert!(error.to_string().contains("incomplete response frame"));
        server.join().unwrap();
        let _ = fs::remove_file(socket_path);
    }

    #[test]
    fn daemon_error_output_preserves_qemu_network_blocker_requirement() {
        let error = print_daemon_response(BridgeVmResponse::Error {
            message: "failed to build Compatibility Mode QEMU command: QEMU launch blocker qemu-advanced-network-requires-schema: advanced networking requires an advanced Compatibility Mode QEMU schema before args can be generated; requirement: Compatibility Mode QEMU requires an advanced network schema and launcher wiring before launch".to_string(),
        })
        .unwrap_err();
        let message = format!("{error:#}");

        assert!(
            message.contains("QEMU launch blocker qemu-advanced-network-requires-schema"),
            "missing QEMU blocker: {message}"
        );
        assert!(
            message.contains("requirement: Compatibility Mode QEMU requires an advanced network schema and launcher wiring before launch"),
            "missing QEMU requirement: {message}"
        );
    }
}
