//! Split out of main.rs by responsibility.

use crate::*;

pub(crate) fn print_runtime_control(control: &bridgevm_storage::RuntimeControlMetadata) {
    println!("Runtime control kind: {}", control.kind);
    println!("Runtime control socket: {}", control.socket_path.display());
    println!("Runtime control commands: {}", control.commands.join(", "));
}

pub(crate) fn print_runner_runtime_policy(policy: &RuntimeResourcePolicyMetadata) {
    println!("Runtime policy visibility: {}", policy.visibility);
    println!("Runtime policy display FPS cap: {}", policy.display_fps_cap);
    println!("Runtime policy live applied: {}", policy.live_applied);
    println!(
        "Runtime policy control acknowledged: {}",
        policy.runtime_control_acknowledged
    );
    if !policy.live_apply_blockers.is_empty() {
        let blockers = policy
            .live_apply_blockers
            .iter()
            .map(|blocker| blocker.code.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        println!("Runtime policy blockers: {blockers}");
    }
}

pub(crate) fn print_runtime_resource_policy(policy: &RuntimeResourcePolicyMetadata) {
    println!("Runtime resources for {}", policy.vm);
    println!("Mode: {}", policy.mode);
    println!("Profile: {}", policy.profile);
    println!("Visibility: {}", policy.visibility);
    println!("State: {}", policy.state);
    println!("On battery: {}", policy.on_battery);
    println!("Memory: {}", policy.memory);
    println!("CPU: {}", policy.cpu);
    println!("Display FPS cap: {}", policy.display_fps_cap);
    println!("Rationale: {}", policy.rationale);
    println!("Live applied: {}", policy.live_applied);
    println!(
        "Runtime control acknowledged: {}",
        policy.runtime_control_acknowledged
    );
    if policy.live_apply_blockers.is_empty() {
        println!("Live apply blockers: none");
    } else {
        for blocker in &policy.live_apply_blockers {
            println!("Live apply blocker: {} - {}", blocker.code, blocker.message);
        }
    }
    println!("Metadata recorded: {}", policy.updated_at_unix);
}

pub(crate) fn compatibility_qemu_command_error(error: QemuError) -> String {
    format!("failed to build Compatibility Mode QEMU command: {error}")
}

pub(crate) fn compatibility_launch_readiness_summary(
    readiness: &LaunchReadinessMetadata,
) -> String {
    let summary = readiness
        .blockers
        .iter()
        .map(|blocker| format!("{}: {}", blocker.code, blocker.message))
        .collect::<Vec<_>>()
        .join("; ");
    if summary.is_empty() {
        "Compatibility Mode launch readiness failed".to_string()
    } else {
        format!("Compatibility Mode launch readiness failed: {summary}")
    }
}

pub(crate) fn print_launch_readiness(readiness: &LaunchReadinessMetadata) {
    println!("Launch ready: {}", readiness.ready);
    if readiness.blockers.is_empty() {
        return;
    }
    println!("Launch blockers:");
    for blocker in &readiness.blockers {
        match &blocker.path {
            Some(path) => println!(
                "- {}: {} ({})",
                blocker.code,
                blocker.message,
                path.display()
            ),
            None => println!("- {}: {}", blocker.code, blocker.message),
        }
    }
}

pub(crate) fn print_lifecycle_plan(plan: &LifecyclePlanRecord) {
    println!("Lifecycle plan for {}", plan.vm);
    println!("Action: {}", plan.action);
    println!("Current state: {}", plan.current_state);
    println!("Target state: {}", plan.target_state);
    println!("Backend: {}", plan.backend);
    println!("Metadata only: {}", plan.metadata_only);
    println!("Executable: {}", plan.executable);
    if let Some(command) = &plan.qmp_command {
        println!("QMP command: {}", command);
    }
    if let Some(path) = &plan.socket_path {
        println!("QMP socket: {}", path.display());
    }
    println!("QMP socket available: {}", plan.socket_available);
    if let Some(supervisor) = &plan.qmp_supervisor {
        print_qmp_supervisor(supervisor);
    }
    if plan.blockers.is_empty() {
        println!("Blockers: none");
    } else {
        for blocker in &plan.blockers {
            println!("Blocker: {}", blocker);
        }
    }
    for note in &plan.notes {
        println!("Note: {}", note);
    }
}

pub(crate) fn print_qmp_supervisor(supervisor: &QmpSupervisorMetadata) {
    println!("QMP supervisor events: {}", supervisor.events.len());
    println!(
        "QMP supervisor envelopes read: {}",
        supervisor.envelopes_read
    );
    println!("QMP supervisor limit reached: {}", supervisor.limit_reached);
    println!("QMP supervisor updated at: {}", supervisor.updated_at_unix);
    if let Some(event) = &supervisor.terminal_event {
        println!("QMP supervisor terminal event: {}", event.name);
    }
    for event in &supervisor.events {
        println!("QMP supervisor event: {}", event.name);
    }
}

pub(crate) fn print_readiness_report(report: &VmReadinessReport) {
    println!("Readiness report for {}", report.vm);
    println!("Mode: {}", report.mode);
    println!("State: {}", report.state);
    println!("Metadata only: {}", report.metadata_only);
    println!("Live E2E required: {}", report.live_e2e_required);
    if let Some(evidence) = &report.live_evidence {
        println!("Live evidence: verified ({})", evidence.path.display());
        println!("Live evidence backend: {}", evidence.backend);
        println!("Live evidence VM: {}", evidence.vm_name);
        println!("Live evidence boot mode: {}", evidence.boot_mode);
        println!("Live evidence disk: {}", evidence.disk_format);
        println!("Live evidence network: {}", evidence.network);
        println!(
            "Live evidence serial sentinel: required={} proven={}",
            evidence.serial_sentinel_required, evidence.serial_sentinel_proven
        );
        println!(
            "Live evidence graphical boot progress: proven={}",
            evidence.graphical_boot_progress_proven
        );
        println!(
            "Live evidence viewer/console: proven={}",
            evidence.viewer_evidence_proven
        );
        println!("Live evidence QMP: proven={}", evidence.qmp_evidence_proven);
        println!(
            "Live evidence guest-tools effects: proven={}",
            evidence.guest_tools_effects_proven
        );
    }
    if !report.evidence_requirements.is_empty() {
        println!("Evidence requirements:");
        for requirement in &report.evidence_requirements {
            println!(
                "- {}: required={} proven={} ({})",
                requirement.kind, requirement.required, requirement.proven, requirement.note
            );
        }
    }
    match &report.boot_media {
        Some(status) => {
            println!("Boot media entries: {}", status.entries.len());
            for entry in &status.entries {
                println!(
                    "Boot media {}: {} ({})",
                    entry.kind,
                    entry.path.display(),
                    if entry.exists { "exists" } else { "missing" }
                );
            }
        }
        None => {
            if let Some(error) = &report.boot_media_error {
                println!("Boot media status error: {}", error);
            } else {
                println!("Boot media: not applicable");
            }
        }
    }
    match &report.snapshot_chain {
        Some(chain) => {
            println!("Active disk: {}", chain.active_disk.path.display());
            println!("Active disk exists: {}", chain.active_disk.exists);
            println!("Snapshot disk entries: {}", chain.disks.len());
        }
        None => {
            if let Some(error) = &report.snapshot_chain_error {
                println!("Snapshot chain error: {}", error);
            }
        }
    }
    match &report.runner {
        Some(runner) => {
            println!("Runner: {}", runner.engine);
            println!("Runner dry run: {}", runner.dry_run);
            if let Some(readiness) = &runner.launch_readiness {
                print_launch_readiness(readiness);
            }
        }
        None => {
            if let Some(error) = &report.runner_error {
                println!("Runner metadata error: {}", error);
            } else {
                println!("Runner: missing metadata");
            }
            if let Some(readiness) = &report.pre_run_launch_readiness {
                println!("Pre-run launch readiness:");
                print_launch_readiness(readiness);
            }
        }
    }
    if let Some(supervisor) = &report.qmp_supervisor {
        print_qmp_supervisor(supervisor);
    }
    if report.blockers.is_empty() {
        println!("Blockers: none");
    } else {
        println!("Blockers:");
        for blocker in &report.blockers {
            println!("- {}", blocker);
        }
    }
    for note in &report.notes {
        println!("Note: {}", note);
    }
}

pub(crate) fn print_network_plan(plan: &NetworkPlanRecord) {
    println!("Network plan for {}", plan.vm);
    println!("Backend: {}", plan.backend);
    println!("Mode: {}", plan.mode);
    println!("Hostname: {}", plan.hostname);
    println!("Dry run: {}", plan.dry_run);
    println!("Executable: {}", plan.executable);
    if let Some(capabilities) = &plan.capabilities {
        println!("Guest outbound: {}", capabilities.guest_outbound);
        println!("Host to guest: {}", capabilities.host_to_guest);
        println!("Guest to host: {}", capabilities.guest_to_host);
        println!(
            "Host visible hostname: {}",
            capabilities.host_visible_hostname
        );
        println!(
            "Supports port forwarding: {}",
            capabilities.supports_port_forwarding
        );
        println!(
            "Requires privileged helper: {}",
            capabilities.requires_privileged_helper
        );
    }
    if plan.port_forwards.is_empty() {
        println!("Port forwards: none");
    } else {
        for forward in &plan.port_forwards {
            println!("Port forward: {}:{}", forward.host, forward.guest);
        }
    }
    if plan.blockers.is_empty() {
        println!("Blockers: none");
    } else {
        for blocker in &plan.blockers {
            println!("Blocker: {} - {}", blocker.code, blocker.message);
        }
    }
    for note in &plan.notes {
        println!("Note: {}", note);
    }
}

pub(crate) fn print_boot_media(name: &str, boot: &AppleVzBootSpec) {
    println!("VM: {name}");
    println!("Boot mode: {}", boot.mode);
    if let Some(path) = &boot.installer_image {
        print_boot_path("Installer image", path);
    }
    if let Some(path) = &boot.kernel {
        print_boot_path("Kernel", path);
    }
    if let Some(path) = &boot.initrd {
        print_boot_path("Initrd", path);
    }
    if let Some(command_line) = &boot.kernel_command_line {
        println!("Kernel command line: {command_line}");
    }
    if let Some(path) = &boot.macos_restore_image {
        print_boot_path("macOS restore image", path);
    }
}

pub(crate) fn print_boot_path(label: &str, path: &AppleVzPathSpec) {
    println!("{label}: {}", path.path);
    println!("{label} exists: {}", path.exists);
}
