//! Drives snapshot_pair from a shell, so the live gate exercises the same code
//! the product uses rather than a reimplementation of it.

use bridgevm_hvf::media_lease_session::{self, pending_delete};
use bridgevm_hvf::snapshot_pair::{create_snapshot, restore_snapshot, verify_snapshot};
use std::path::Path;
use std::process::ExitCode;

#[path = "snapshot_pair_cli/usage.rs"]
mod usage;
use usage::usage;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(command) = args.first() else {
        return usage();
    };
    let result = match (command.as_str(), args.len()) {
        ("lease", 3) => return media_lease_session::command(&args[1..]),
        ("lease-pending-delete", 3) => return pending_delete::command(&args[1..]),
        ("digest", 3) => return bridgevm_hvf::snapshot_pair::selected::digest_command(&args[1..]),
        ("size", 3) => return bridgevm_hvf::snapshot_pair::selected::size_command(&args[1..]),
        ("grow", 4) => return bridgevm_hvf::snapshot_pair::selected::grow_command(&args[1..]),
        ("convert-to-raw", 3) => return bridgevm_hvf::disk_import::command(&args[1..]),
        ("import-disk", 3) => {
            return match bridgevm_hvf::snapshot_pair::import_disk(
                Path::new(&args[1]),
                Path::new(&args[2]),
            ) {
                Ok(paired) => {
                    println!("paired_vars {paired}");
                    ExitCode::SUCCESS
                }
                Err(error) => {
                    eprintln!("{error}");
                    ExitCode::FAILURE
                }
            };
        }
        // Older live-gate scripts still append a byte quota; it no longer limits anything.
        ("create", 5 | 6) => create_snapshot(
            Path::new(&args[1]),
            Path::new(&args[2]),
            Path::new(&args[3]),
            &args[4],
        ),
        ("verify", 2) => verify_snapshot(Path::new(&args[1])),
        ("restore", 4) => restore_snapshot(
            Path::new(&args[1]),
            Path::new(&args[2]),
            Path::new(&args[3]),
        ),
        _ => return usage(),
    };
    match result {
        Ok(manifest) => {
            println!("format_version {}", manifest.format_version);
            println!("vm_id {}", manifest.vm_id);
            println!("disk_bytes {}", manifest.disk_bytes);
            println!("disk_sha256 {}", manifest.disk_sha256);
            println!("vars_bytes {}", manifest.vars_bytes);
            println!("vars_sha256 {}", manifest.vars_sha256);
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}
