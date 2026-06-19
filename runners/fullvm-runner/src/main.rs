use anyhow::{bail, Context, Result};
use bridgevm_core::VmEngine;
use bridgevm_fullvm::FullVmEngine;
use bridgevm_qemu::{
    build_compatibility_command, is_qmp_status_unavailable, qmp_socket_path, query_status,
};
use bridgevm_storage::{
    ActiveDiskMetadata, DiskPreparationMetadata, GuestToolsRunnerMetadata, RunnerMetadata,
    VmRuntimeState, VmStore,
};
use clap::Parser;
use std::{
    fs::{self, OpenOptions},
    path::PathBuf,
    process::{Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Parser)]
struct Args {
    #[arg(value_name = "VM")]
    vm: Option<String>,
    #[arg(long, value_name = "PATH")]
    store: Option<PathBuf>,
    #[arg(long)]
    print_qemu_args: bool,
    #[arg(long)]
    spawn: bool,
    #[arg(long)]
    runner_status: bool,
    #[arg(long)]
    qmp_socket: bool,
    #[arg(long)]
    qmp_status: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let engine = FullVmEngine;
    if let Some(vm) = args.vm {
        let store = args
            .store
            .map(VmStore::new)
            .unwrap_or_else(VmStore::default);
        if args.runner_status {
            match store.runner_metadata(&vm)? {
                Some(metadata) => {
                    println!("engine: {}", metadata.engine);
                    println!(
                        "pid: {}",
                        metadata
                            .pid
                            .map_or("none".to_string(), |pid| pid.to_string())
                    );
                    println!("dry_run: {}", metadata.dry_run);
                    println!("log: {}", metadata.log_path.display());
                    println!("command: {}", command_line(&metadata.command));
                }
                None => println!("no runner metadata for {vm}"),
            }
            return Ok(());
        }

        let (bundle, manifest, _) = store
            .get_vm_with_active_disk(&vm)
            .context("failed to read VM")?;
        if args.qmp_socket {
            println!("{}", qmp_socket_path(&bundle).display());
            return Ok(());
        }
        if args.qmp_status {
            let path = qmp_socket_path(&bundle);
            if !path.exists() {
                println!("QMP socket unavailable: {}", path.display());
            } else {
                let status = match query_status(&path) {
                    Ok(status) => status,
                    Err(error) if is_qmp_status_unavailable(&error) => {
                        println!("QMP socket unavailable: {}", path.display());
                        return Ok(());
                    }
                    Err(error) => return Err(error).context("failed to query QMP status"),
                };
                println!("qmp_status: {}", status.status);
                println!("running: {}", status.running);
            }
            return Ok(());
        }

        let (disk, active_disk) = store
            .prepare_active_disk(&vm)
            .context("failed to prepare active disk")?;
        if args.spawn && !disk.exists {
            if let Some(command) = &disk.create_command {
                bail!(
                    "active disk is not ready: {}; create it with: {}",
                    disk.path.display(),
                    command.join(" ")
                );
            }
            bail!("active disk is not ready: {}", disk.path.display());
        }

        ensure_compatibility_mode(manifest.mode)?;
        let command = build_compatibility_command(&manifest, &bundle)?;
        if args.print_qemu_args {
            for word in command.render_shell_words() {
                println!("{word}");
            }
        } else if args.spawn {
            let guest_tools = store
                .guest_tools_runner_metadata(&vm)
                .context("failed to prepare guest tools runner metadata")?;
            fs::create_dir_all(bundle.join("logs"))?;
            let log_path = bundle.join("logs").join("qemu.log");
            let stdout = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&log_path)
                .context("failed to open QEMU log file")?;
            let stderr = stdout
                .try_clone()
                .context("failed to clone QEMU log file")?;
            let mut child = Command::new(&command.program)
                .args(&command.args)
                .stdout(Stdio::from(stdout))
                .stderr(Stdio::from(stderr))
                .spawn()
                .with_context(|| format!("failed to spawn {}", command.program))?;
            let metadata = runner_metadata(
                engine.name(),
                Some(child.id()),
                command.render_shell_words(),
                log_path,
                now_unix(),
                false,
                Some(guest_tools),
                Some(disk),
                Some(active_disk),
            );
            store.write_runner_metadata(&vm, &metadata)?;
            store.transition_state(&vm, VmRuntimeState::Running)?;
            println!("started {} with pid {}", vm, child.id());
            let _ = child.try_wait();
        } else {
            let guest_tools = store
                .guest_tools_runner_metadata(&vm)
                .context("failed to prepare guest tools runner metadata")?;
            let command_words = command.render_shell_words();
            let metadata = runner_metadata(
                engine.name(),
                None,
                command_words,
                bundle.join("logs").join("qemu.log"),
                now_unix(),
                true,
                Some(guest_tools),
                Some(disk),
                Some(active_disk),
            );
            store.write_runner_metadata(&vm, &metadata)?;
            println!("{} dry-run for {}", engine.name(), vm);
            println!("{}", command_line(&metadata.command));
        }
    } else {
        println!("{}", runner_ready_line(engine.name()));
    }
    Ok(())
}

fn runner_ready_line(engine_name: &str) -> String {
    format!("{engine_name} runner ready")
}

fn ensure_compatibility_mode(mode: impl std::fmt::Display) -> Result<()> {
    let mode = mode.to_string();
    if mode != "compatibility" {
        anyhow::bail!(
            "QEMU command builder only supports Compatibility Mode manifests, got {}",
            mode
        );
    }
    Ok(())
}

fn command_line(words: &[String]) -> String {
    words.join(" ")
}

fn runner_metadata(
    engine_name: &str,
    pid: Option<u32>,
    command: Vec<String>,
    log_path: PathBuf,
    started_at_unix: u64,
    dry_run: bool,
    guest_tools: Option<GuestToolsRunnerMetadata>,
    disk: Option<DiskPreparationMetadata>,
    active_disk: Option<ActiveDiskMetadata>,
) -> RunnerMetadata {
    RunnerMetadata {
        engine: engine_name.to_string(),
        pid,
        command,
        log_path,
        started_at_unix,
        dry_run,
        launch_spec_path: None,
        guest_tools,
        disk,
        active_disk,
        launch_readiness: None,
        runtime_control: None,
    }
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use bridgevm_storage::ActiveDiskSource;

    #[test]
    fn ready_line_matches_no_vm_output() {
        assert_eq!(runner_ready_line("fullvm"), "fullvm runner ready");
    }

    #[test]
    fn compatibility_mode_is_accepted() {
        ensure_compatibility_mode("compatibility").expect("compatibility mode should be accepted");
    }

    #[test]
    fn fast_mode_is_rejected() {
        let error = ensure_compatibility_mode("fast").expect_err("fast mode must fail");

        assert!(
            error.to_string().contains(
                "QEMU command builder only supports Compatibility Mode manifests, got fast"
            ),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn command_line_joins_rendered_words() {
        let words = vec![
            "qemu-system-aarch64".to_string(),
            "-name".to_string(),
            "demo".to_string(),
        ];

        assert_eq!(command_line(&words), "qemu-system-aarch64 -name demo");
    }

    #[test]
    fn runner_metadata_preserves_command_and_injected_timestamp() {
        let command = vec![
            "qemu-system-x86_64".to_string(),
            "-m".to_string(),
            "2048M".to_string(),
        ];
        let guest_tools = GuestToolsRunnerMetadata {
            transport: "unix".to_string(),
            channel_name: "bridgevm-tools".to_string(),
            socket_path: PathBuf::from("run/tools.sock"),
            token_path: PathBuf::from("run/tools.token"),
            token_created_at_unix: 41,
        };
        let disk = DiskPreparationMetadata {
            path: PathBuf::from("disk.qcow2"),
            format: "qcow2".to_string(),
            size: "20G".to_string(),
            size_bytes: Some(20 * 1024 * 1024 * 1024),
            exists: true,
            created: false,
            create_command: None,
            prepared_at_unix: 42,
        };
        let active_disk = ActiveDiskMetadata {
            source: ActiveDiskSource::Primary,
            snapshot: None,
            path: PathBuf::from("disk.qcow2"),
            format: "qcow2".to_string(),
            exists: true,
            activated_at_unix: 43,
        };

        let metadata = runner_metadata(
            "fullvm",
            None,
            command.clone(),
            PathBuf::from("logs/qemu.log"),
            1234,
            true,
            Some(guest_tools),
            Some(disk),
            Some(active_disk),
        );

        assert_eq!(metadata.engine, "fullvm");
        assert_eq!(metadata.pid, None);
        assert_eq!(metadata.command, command);
        assert_eq!(metadata.log_path, PathBuf::from("logs/qemu.log"));
        assert_eq!(metadata.started_at_unix, 1234);
        assert!(metadata.dry_run);
        assert!(metadata.guest_tools.is_some());
        assert!(metadata.disk.is_some());
        assert!(metadata.active_disk.is_some());
        assert_eq!(metadata.launch_readiness, None);
    }

    #[test]
    fn now_unix_returns_a_plausible_timestamp() {
        let before = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let actual = now_unix();
        let after = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        assert!(actual >= before, "{actual} should be >= {before}");
        assert!(actual <= after, "{actual} should be <= {after}");
    }
}
