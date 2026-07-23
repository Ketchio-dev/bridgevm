//! Split out of main.rs by responsibility.

use crate::*;

pub(crate) fn print_boot_media_import(import: &BootMediaImportMetadata) {
    println!("Imported boot media for {}", import.vm);
    println!("Boot media kind: {}", import.kind);
    println!("Source: {}", import.source.display());
    println!("Destination: {}", import.destination.display());
    println!("Bytes: {}", import.bytes);
    println!("Replaced existing media: {}", import.replaced);
    println!("Imported: {}", import.imported_at_unix);
}

pub(crate) fn print_boot_media_status(status: &BootMediaStatus) {
    println!("VM: {}", status.vm);
    if status.entries.is_empty() {
        println!("No boot media entries");
        return;
    }
    for (index, entry) in status.entries.iter().enumerate() {
        if index > 0 {
            println!();
        }
        println!("Boot media kind: {}", entry.kind);
        println!("Path: {}", entry.path.display());
        println!("Exists: {}", entry.exists);
        println!(
            "Bytes: {}",
            entry
                .bytes
                .map_or("unknown".to_string(), |bytes| bytes.to_string())
        );
        if let Some(import) = &entry.last_import {
            println!("Last import source: {}", import.source.display());
            println!("Last import bytes: {}", import.bytes);
            println!("Last import time: {}", import.imported_at_unix);
        } else {
            println!("Last import: none");
        }
        if let Some(verification) = &entry.last_verification {
            println!(
                "Last verification expected: {}",
                verification.expected_sha256
            );
            println!("Last verification actual: {}", verification.actual_sha256);
            println!("Last verification passed: {}", verification.verified);
            println!("Last verification time: {}", verification.verified_at_unix);
        } else {
            println!("Last verification: none");
        }
        if let Some(plan) = &entry.last_download_plan {
            println!("Last download URL: {}", plan.url);
            println!(
                "Last download expected SHA-256: {}",
                plan.expected_sha256.as_deref().unwrap_or("unspecified")
            );
            println!("Last download planned: {}", plan.planned_at_unix);
        } else {
            println!("Last download plan: none");
        }
        if let Some(download) = &entry.last_download {
            println!("Last download completed: {}", download.downloaded);
            println!(
                "Last download bytes: {}",
                download
                    .bytes
                    .map_or("unknown".to_string(), |bytes| bytes.to_string())
            );
            println!("Last download time: {}", download.downloaded_at_unix);
        } else {
            println!("Last download result: none");
        }
    }
}

pub(crate) fn print_guest_tools_status(status: &GuestToolsStatusRecord) {
    println!("Guest tools status for {}", status.vm);
    println!("Tools requirement: {}", status.tools);
    println!("Tools token created: {}", status.token_created_at_unix);
    if status.capabilities.is_empty() {
        println!("No guest tools capabilities allowed");
        return;
    }
    for capability in &status.capabilities {
        println!("Capability: {}", capability.name);
        println!("Max version: {}", capability.max_version);
        println!("Enabled by: {}", capability.enabled_by);
    }
    if status.approved_shared_folders.is_empty() {
        println!("Approved shared folders: 0");
    } else {
        for folder in &status.approved_shared_folders {
            println!("Approved shared folder: {}", folder.name);
            println!("Approved shared folder host path: {}", folder.host_path);
            println!("Approved shared folder token: {}", folder.host_path_token);
            println!("Approved shared folder read-only: {}", folder.read_only);
            println!("Approved shared folder approval: {}", folder.approval);
        }
    }
    if let Some(runtime) = &status.runtime {
        println!("Runtime connected: {}", runtime.connected);
        println!(
            "Runtime guest OS: {}",
            runtime.guest_os.as_deref().unwrap_or("unknown")
        );
        println!(
            "Runtime agent version: {}",
            runtime.agent_version.as_deref().unwrap_or("unknown")
        );
        println!("Runtime updated: {}", runtime.updated_at_unix);
        println!(
            "Runtime last heartbeat: {}",
            runtime
                .last_heartbeat_at_unix
                .map_or("none".to_string(), |timestamp| timestamp.to_string())
        );
        for address in &runtime.guest_ip_addresses {
            println!("Guest IP: {}", address.address);
            println!(
                "Guest IP interface: {}",
                address.interface.as_deref().unwrap_or("unknown")
            );
        }
        if runtime.shared_folders.is_empty() {
            println!("Shared folders mounted: 0");
        } else {
            for folder in &runtime.shared_folders {
                println!("Shared folder: {}", folder.name);
                println!("Shared folder token: {}", folder.host_path_token);
                println!("Shared folder mounted: {}", folder.mounted_at_unix);
            }
        }
        if let Some(metrics) = &runtime.metrics {
            println!("Guest CPU percent: {}", metrics.cpu_percent);
            println!("Guest memory used MiB: {}", metrics.memory_used_mib);
            println!("Guest metrics updated: {}", metrics.updated_at_unix);
        }
        if let Some(clipboard) = &runtime.clipboard {
            println!("Guest clipboard text: {}", clipboard.text);
            println!("Guest clipboard updated: {}", clipboard.updated_at_unix);
        }
        if let Some(result) = &runtime.last_command_result {
            println!("Last command request ID: {}", result.request_id);
            println!(
                "Last command capability: {}",
                result.capability.as_deref().unwrap_or("none")
            );
            println!("Last command OK: {}", result.ok);
            println!(
                "Last command error code: {}",
                result.error_code.as_deref().unwrap_or("none")
            );
            println!(
                "Last command message: {}",
                result.message.as_deref().unwrap_or("none")
            );
            if let Some(payload) = &result.result {
                println!("Last command result JSON:");
                println!(
                    "{}",
                    serde_json::to_string_pretty(payload).unwrap_or_else(|_| payload.to_string())
                );
            }
            if let Some(metadata) = &result.metadata {
                println!("Last command metadata JSON:");
                println!(
                    "{}",
                    serde_json::to_string_pretty(metadata).unwrap_or_else(|_| metadata.to_string())
                );
            }
            println!("Last command completed: {}", result.completed_at_unix);
        }
        if let Some(update) = &runtime.agent_update {
            println!("Agent update current: {}", update.current_version);
            println!("Agent update available: {}", update.available_version);
            println!(
                "Agent update URL: {}",
                update.download_url.as_deref().unwrap_or("none")
            );
            println!(
                "Agent update signature: {}",
                if update.signature.is_some() {
                    "present"
                } else {
                    "none"
                }
            );
            println!("Agent update observed: {}", update.observed_at_unix);
        }
    } else {
        println!("Runtime connected: false");
    }
}

pub(crate) fn print_guest_tools_token(token: &GuestToolsTokenRecord) {
    println!("Guest tools token for {}", token.vm);
    println!("Token: {}", token.token);
    println!("Created: {}", token.created_at_unix);
}

pub(crate) fn print_guest_tools_linux_command(command: &GuestToolsLinuxCommandRecord) {
    for word in &command.command {
        println!("{word}");
    }
}

pub(crate) fn print_guest_tools_session(session: &GuestToolsSessionRecord) {
    println!("Accepted guest tools session for {}", session.vm);
    println!("Guest OS: {}", session.guest_os);
    println!(
        "Agent version: {}",
        session.agent_version.as_deref().unwrap_or("unknown")
    );
    if session.capabilities.is_empty() {
        println!("No advertised capabilities");
        return;
    }
    for capability in &session.capabilities {
        println!("Capability: {}", capability.name);
        println!("Version: {}", capability.version);
    }
}

pub(crate) fn print_boot_media_download(download: &BootMediaDownloadResultMetadata) {
    println!("Downloaded boot media for {}", download.vm);
    println!("Boot media kind: {}", download.kind);
    println!("URL: {}", download.url);
    println!("Destination: {}", download.destination.display());
    println!("Downloaded: {}", download.downloaded);
    println!("Replaced existing media: {}", download.replaced);
    println!(
        "Bytes: {}",
        download
            .bytes
            .map_or("unknown".to_string(), |bytes| bytes.to_string())
    );
    println!(
        "Expected SHA-256: {}",
        download.expected_sha256.as_deref().unwrap_or("unspecified")
    );
    println!(
        "Actual SHA-256: {}",
        download.actual_sha256.as_deref().unwrap_or("unknown")
    );
    println!(
        "Verified: {}",
        download
            .verified
            .map_or("not requested".to_string(), |verified| verified.to_string())
    );
    println!("Downloaded at: {}", download.downloaded_at_unix);
}

pub(crate) fn print_boot_media_download_plan(plan: &BootMediaDownloadPlanMetadata) {
    println!("Planned boot media download for {}", plan.vm);
    println!("Boot media kind: {}", plan.kind);
    println!("URL: {}", plan.url);
    println!("Destination: {}", plan.destination.display());
    println!("Destination exists: {}", plan.exists);
    println!(
        "Destination bytes: {}",
        plan.bytes
            .map_or("unknown".to_string(), |bytes| bytes.to_string())
    );
    println!(
        "Expected SHA-256: {}",
        plan.expected_sha256.as_deref().unwrap_or("unspecified")
    );
    if let Some(import) = &plan.last_import {
        println!("Last import source: {}", import.source.display());
        println!("Last import time: {}", import.imported_at_unix);
    } else {
        println!("Last import: none");
    }
    if let Some(verification) = &plan.last_verification {
        println!("Last verification passed: {}", verification.verified);
        println!("Last verification time: {}", verification.verified_at_unix);
    } else {
        println!("Last verification: none");
    }
    println!("Planned at: {}", plan.planned_at_unix);
}

pub(crate) fn print_boot_media_verification(verification: &BootMediaVerificationMetadata) {
    println!("Verified boot media for {}", verification.vm);
    println!("Boot media kind: {}", verification.kind);
    println!("Path: {}", verification.path.display());
    println!("Bytes: {}", verification.bytes);
    println!("Expected SHA-256: {}", verification.expected_sha256);
    println!("Actual SHA-256: {}", verification.actual_sha256);
    println!("Verified: {}", verification.verified);
    println!("Verified at: {}", verification.verified_at_unix);
}

pub(crate) fn print_disk_status(disk: &bridgevm_storage::DiskPreparationMetadata) {
    println!("Disk: {}", disk.path.display());
    println!("Disk format: {}", disk.format);
    println!("Disk size: {}", disk.size);
    println!("Disk ready: {}", disk.exists);
    println!("Disk created: {}", disk.created);
    if let Some(command) = &disk.create_command {
        println!("Disk create command: {}", command.join(" "));
    }
}

pub(crate) fn print_disk_create_status(metadata: &bridgevm_storage::DiskCreateMetadata) {
    println!("Disk create executed: {}", metadata.executed);
    if let Some(command) = &metadata.command {
        println!("Disk create command: {}", command.join(" "));
    }
    if let Some(status) = &metadata.exit_status {
        println!("Disk create status: {}", status);
    }
    if !metadata.stdout.is_empty() {
        println!("Disk create stdout: {}", metadata.stdout.trim_end());
    }
    if !metadata.stderr.is_empty() {
        println!("Disk create stderr: {}", metadata.stderr.trim_end());
    }
    print_disk_status(&metadata.preparation);
}

pub(crate) fn print_disk_inspect_status(metadata: &bridgevm_storage::DiskInspectMetadata) {
    println!("Disk inspect command: {}", metadata.command.join(" "));
    println!("Disk inspect status: {}", metadata.exit_status);
    println!(
        "Disk inspect duration: {} microseconds",
        metadata.inspect_duration_microseconds
    );
    if !metadata.stderr.is_empty() {
        println!("Disk inspect stderr: {}", metadata.stderr.trim_end());
    }
    print_disk_status(&metadata.preparation);
    println!(
        "Disk info: {}",
        serde_json::to_string_pretty(&metadata.info).unwrap_or_else(|_| metadata.info.to_string())
    );
}
