//! Split out of lib.rs by responsibility.

use crate::*;

pub fn readiness_report(store: &VmStore, name: &str) -> Result<VmReadinessReport, String> {
    readiness_report_with_live_evidence(store, name, None)
}

pub fn readiness_report_with_live_evidence(
    store: &VmStore,
    name: &str,
    live_evidence_path: Option<&Path>,
) -> Result<VmReadinessReport, String> {
    readiness_report_with_live_evidence_and_record(store, name, live_evidence_path, false)
}

pub fn readiness_report_with_live_evidence_and_record(
    store: &VmStore,
    name: &str,
    live_evidence_path: Option<&Path>,
    record_live_evidence: bool,
) -> Result<VmReadinessReport, String> {
    readiness_report_with_live_evidence_options(
        store,
        name,
        live_evidence_path,
        record_live_evidence,
        false,
    )
}

pub fn readiness_report_with_live_evidence_options(
    store: &VmStore,
    name: &str,
    live_evidence_path: Option<&Path>,
    record_live_evidence: bool,
    clear_live_evidence: bool,
) -> Result<VmReadinessReport, String> {
    let (_, manifest) = store.get_vm(name).map_err(|error| error.to_string())?;
    let state = store.state(name).map_err(|error| error.to_string())?.state;

    let (boot_media, boot_media_error) = if manifest.mode == VmMode::Fast {
        match inspect_boot_media_status(store, name) {
            Ok(status) => (Some(status), None),
            Err(error) => (None, Some(error)),
        }
    } else {
        (None, None)
    };

    let (snapshot_chain, snapshot_chain_error) = match store.snapshot_chain(name) {
        Ok(chain) => (Some(chain), None),
        Err(error) => (None, Some(error.to_string())),
    };
    let (runner, runner_error) = match store.runner_metadata(name) {
        Ok(metadata) => (metadata, None),
        Err(error) => (None, Some(error.to_string())),
    };
    let qmp_supervisor = store
        .qmp_supervisor_metadata(name)
        .map_err(|error| error.to_string())?;
    let mut pre_run_launch_readiness = None;

    let mut blockers = Vec::new();
    let mut notes = vec![
        "metadata-only preflight report; no VM, QEMU, Apple VZ, console, or guest workload was started".to_string(),
        "live E2E boot, console, and guest-tools effects still require the explicit opt-in live smoke evidence path".to_string(),
    ];
    let mut live_evidence = None;
    if clear_live_evidence {
        if live_evidence_path.is_some() || record_live_evidence {
            blockers.push(
                "live-evidence-clear-error:--clear-live-evidence cannot be combined with --live-evidence or --record-live-evidence"
                    .to_string(),
            );
        } else {
            match store.clear_live_evidence_metadata(name) {
                Ok(()) => notes.push("cleared preserved live evidence metadata".to_string()),
                Err(error) => blockers.push(format!("live-evidence-clear-error:{error}")),
            }
        }
    } else if let Some(path) = live_evidence_path {
        let live_context = LiveEvidenceVerificationContext::from_readiness(
            name,
            manifest.mode,
            &store.bundle_path(name),
            snapshot_chain.as_ref(),
        );
        match verify_live_evidence_bundle_with_context(path, Some(&live_context)) {
            Ok(mut verification) => {
                if record_live_evidence {
                    match store.import_live_evidence_bundle(name, path) {
                        Ok(metadata) => {
                            notes.push(format!(
                                "recorded preserved live evidence bundle: {}",
                                metadata.preserved_path.display()
                            ));
                            verification = verify_live_evidence_bundle_with_context(
                                &metadata.preserved_path,
                                Some(&live_context),
                            )?;
                        }
                        Err(error) => blockers.push(format!("live-evidence-record-error:{error}")),
                    }
                }
                if !blockers
                    .iter()
                    .any(|blocker| blocker.starts_with("live-evidence-record-error:"))
                {
                    notes.push(format!(
                        "verified {} live evidence bundle: {}",
                        live_evidence_backend_label(&verification.backend),
                        verification.path.display()
                    ));
                    live_evidence = Some(verification);
                }
            }
            Err(error) => blockers.push(format!("live-evidence-invalid:{error}")),
        }
    } else if record_live_evidence {
        blockers.push(
            "live-evidence-record-error:--record-live-evidence requires --live-evidence"
                .to_string(),
        );
    } else {
        match store.live_evidence_metadata(name) {
            Ok(Some(metadata)) => {
                let live_context = LiveEvidenceVerificationContext::from_readiness(
                    name,
                    manifest.mode,
                    &store.bundle_path(name),
                    snapshot_chain.as_ref(),
                );
                match verify_live_evidence_bundle_with_context(
                    &metadata.preserved_path,
                    Some(&live_context),
                ) {
                    Ok(verification) => {
                        notes.push(format!(
                            "verified {} live evidence bundle: {}",
                            live_evidence_backend_label(&verification.backend),
                            verification.path.display()
                        ));
                        live_evidence = Some(verification);
                    }
                    Err(error) => blockers.push(format!("live-evidence-invalid:{error}")),
                }
            }
            Ok(None) => {}
            Err(error) => blockers.push(format!("live-evidence-metadata-error:{error}")),
        }
    }

    if let Some(error) = &boot_media_error {
        blockers.push(format!("boot-media-status-error:{error}"));
    }
    if let Some(status) = &boot_media {
        for entry in status.entries.iter().filter(|entry| !entry.exists) {
            blockers.push(format!(
                "boot-media-missing:{}:{}",
                entry.kind,
                entry.path.display()
            ));
        }
    }

    if let Some(error) = &snapshot_chain_error {
        blockers.push(format!("snapshot-chain-error:{error}"));
    }
    if let Some(chain) = &snapshot_chain {
        if !chain.active_disk.exists {
            blockers.push(format!(
                "active-disk-missing:{}",
                chain.active_disk.path.display()
            ));
        }
    }

    match &runner {
        Some(metadata) => {
            if let Some(readiness) = &metadata.launch_readiness {
                if !readiness.ready {
                    for blocker in &readiness.blockers {
                        blockers.push(format!("launch-readiness-blocker:{}", blocker.code));
                    }
                }
            } else {
                notes.push(
                    "runner metadata has no launch-readiness field for this backend".to_string(),
                );
            }
        }
        None => {
            if let Some(error) = &runner_error {
                blockers.push(format!("runner-metadata-error:{error}"));
            } else if manifest.mode == VmMode::Fast {
                match build_fast_plan(&manifest, &store.bundle_path(name)) {
                    Ok(plan) => {
                        let readiness = launch_readiness_metadata(&plan.launch_spec().readiness);
                        if readiness.ready {
                            notes.push("Fast Mode launch readiness was evaluated from the manifest and bundle without writing runner metadata".to_string());
                        } else {
                            for blocker in &readiness.blockers {
                                blockers.push(format!("launch-readiness-blocker:{}", blocker.code));
                            }
                        }
                        pre_run_launch_readiness = Some(readiness);
                    }
                    Err(error) => {
                        blockers.push(format!("launch-readiness-error:{error}"));
                    }
                }
            } else if manifest.mode == VmMode::Compatibility {
                if let Some(chain) = &snapshot_chain {
                    let disk = bridgevm_storage::DiskPreparationMetadata {
                        path: chain.active_disk.path.clone(),
                        format: chain.active_disk.format.clone(),
                        size: manifest.storage.primary.size.clone(),
                        size_bytes: None,
                        exists: chain.active_disk.exists,
                        created: false,
                        create_command: None,
                        prepared_at_unix: now_unix(),
                    };
                    let bundle = store.bundle_path(name);
                    let mut readiness_blockers =
                        compatibility_launch_dependency_blockers(&manifest, &bundle);
                    if let Some(blocker) = build_compatibility_command(&manifest, &bundle)
                        .err()
                        .map(compatibility_launch_readiness_blocker_from_qemu_error)
                    {
                        readiness_blockers.push(blocker);
                    }
                    let readiness =
                        compatibility_launch_readiness_metadata(&disk, readiness_blockers);
                    if readiness.ready {
                        notes.push("Compatibility Mode launch readiness was evaluated from the manifest and active disk without writing runner metadata".to_string());
                    } else {
                        for blocker in &readiness.blockers {
                            blockers.push(format!("launch-readiness-blocker:{}", blocker.code));
                        }
                    }
                    pre_run_launch_readiness = Some(readiness);
                } else if let Some(error) = &snapshot_chain_error {
                    blockers.push(format!("launch-readiness-error:{error}"));
                }
            } else {
                blockers.push("runner-metadata-missing".to_string());
            }
        }
    }

    if state == VmRuntimeState::Running {
        notes.push(
            "running VM should use QMP status and bounded log tails for console diagnostics"
                .to_string(),
        );
    }
    if manifest.mode == VmMode::Compatibility {
        notes.push("Compatibility Mode readiness is driven by disk, runner metadata, QMP, and logs rather than Fast boot media status".to_string());
    }

    Ok(VmReadinessReport {
        vm: name.to_string(),
        mode: manifest.mode,
        state,
        metadata_only: true,
        live_e2e_required: true,
        live_evidence: live_evidence.clone(),
        evidence_requirements: metadata_safe_live_evidence_requirements(live_evidence.as_ref()),
        boot_media,
        boot_media_error,
        snapshot_chain,
        snapshot_chain_error,
        runner,
        pre_run_launch_readiness,
        qmp_supervisor,
        runner_error,
        blockers,
        notes,
    })
}

pub(crate) fn metadata_safe_live_evidence_requirements(
    live_evidence: Option<&VmLiveEvidenceVerification>,
) -> Vec<VmEvidenceRequirement> {
    let live_boot_proven = live_evidence.is_some_and(live_boot_progress_proven);
    let console_proven = live_evidence.is_some_and(|evidence| {
        evidence.serial_sentinel_proven
            || evidence.viewer_evidence_proven
            || evidence.qmp_evidence_proven
    });
    let guest_tools_effects_proven =
        live_evidence.is_some_and(|evidence| evidence.guest_tools_effects_proven);
    vec![
        VmEvidenceRequirement {
            kind: "live-boot".to_string(),
            required: true,
            proven: live_boot_proven,
            note: if live_boot_proven {
                let evidence = live_evidence.expect("live boot proven requires evidence");
                let progress_label =
                    if evidence.serial_sentinel_proven && evidence.graphical_boot_progress_proven {
                        "serial and graphical boot progress"
                    } else if evidence.graphical_boot_progress_proven {
                        "graphical boot progress"
                    } else {
                        "serial boot progress"
                    };
                format!(
                    "verified preserved opt-in {} {progress_label} evidence bundle",
                    live_evidence_backend_label(&evidence.backend)
                )
            } else if let Some(evidence) = live_evidence {
                format!(
                    "verified preserved opt-in {} launch evidence; guest boot progress evidence is still required",
                    live_evidence_backend_label(&evidence.backend)
                )
            } else {
                "requires preserved opt-in serial or graphical boot progress evidence from Apple VZ or QEMU"
                    .to_string()
            },
        },
        VmEvidenceRequirement {
            kind: "console".to_string(),
            required: true,
            proven: console_proven,
            note: if console_proven {
                "verified serial, graphical viewer, or QMP evidence from the preserved live bundle"
                    .to_string()
            } else {
                "requires graphical console, QMP, or serial evidence from a live backend"
                    .to_string()
            },
        },
        VmEvidenceRequirement {
            kind: "guest-tools-effects".to_string(),
            required: true,
            proven: guest_tools_effects_proven,
            note: if guest_tools_effects_proven {
                "verified guest-tools command/effect evidence from the preserved live bundle"
                    .to_string()
            } else {
                "requires guest-tools command and effect evidence from a live guest".to_string()
            },
        },
    ]
}

pub(crate) fn live_boot_progress_proven(evidence: &VmLiveEvidenceVerification) -> bool {
    evidence.serial_sentinel_proven || evidence.graphical_boot_progress_proven
}

pub(crate) struct LiveEvidenceVerificationContext {
    pub(crate) vm_name: String,
    pub(crate) mode: VmMode,
    pub(crate) bundle_path: PathBuf,
    pub(crate) disk_format: Option<String>,
    pub(crate) network: String,
    pub(crate) qmp_socket: PathBuf,
}

impl LiveEvidenceVerificationContext {
    pub(crate) fn from_readiness(
        vm_name: &str,
        mode: VmMode,
        bundle: &Path,
        snapshot_chain: Option<&SnapshotChainMetadata>,
    ) -> Self {
        Self {
            vm_name: vm_name.to_string(),
            mode,
            bundle_path: bundle.to_path_buf(),
            disk_format: snapshot_chain.map(|chain| chain.active_disk.format.clone()),
            network: "nat".to_string(),
            qmp_socket: qmp_socket_path(bundle),
        }
    }
}
