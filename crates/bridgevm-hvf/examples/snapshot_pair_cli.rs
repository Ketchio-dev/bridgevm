//! Drives snapshot_pair from a shell, so the live gate exercises the same code
//! the product uses rather than a reimplementation of it.

use bridgevm_hvf::snapshot_pair::{create_snapshot, restore_snapshot, verify_snapshot};
use std::path::Path;
use std::process::ExitCode;

fn usage() -> ExitCode {
    eprintln!(
        "usage:\n  \
         snapshot_pair_cli create <disk> <vars> <dest> <vm-id> <quota-bytes>\n  \
         snapshot_pair_cli verify <snapshot-dir>\n  \
         snapshot_pair_cli restore <snapshot-dir> <disk> <vars>"
    );
    ExitCode::from(2)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(command) = args.first() else {
        return usage();
    };

    // The VM is powered off by construction here: this binary is the only
    // thing touching the media, and the gate runs it against clones.
    let result = match (command.as_str(), args.len()) {
        ("create", 6) => {
            let Ok(quota) = args[5].parse::<u64>() else {
                eprintln!("quota must be a number of bytes");
                return ExitCode::from(2);
            };
            create_snapshot(
                Path::new(&args[1]),
                Path::new(&args[2]),
                Path::new(&args[3]),
                &args[4],
                false,
                quota,
            )
        }
        ("verify", 2) => verify_snapshot(Path::new(&args[1])),
        ("restore", 4) => restore_snapshot(
            Path::new(&args[1]),
            Path::new(&args[2]),
            Path::new(&args[3]),
            false,
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
