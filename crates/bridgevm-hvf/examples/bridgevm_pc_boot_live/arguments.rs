//! Strict command-line boundary for the sealed proof and raw-disk diagnostic.

use std::path::PathBuf;

pub(super) struct Arguments {
    pub(super) firmware: PathBuf,
    pub(super) media: PathBuf,
    pub(super) vars: PathBuf,
    pub(super) windows_raw: bool,
}

pub(super) fn read() -> Result<Arguments, String> {
    let usage =
        "usage: bridgevm_pc_boot_live [--windows-raw-disk] FIRMWARE_FD BOOT_MEDIA VARS_FILE";
    let mut args = std::env::args_os().skip(1);
    let first = args.next().ok_or_else(|| usage.to_string())?;
    let (firmware, windows_raw) = if first == "--windows-raw-disk" {
        (
            args.next()
                .map(PathBuf::from)
                .ok_or_else(|| usage.to_string())?,
            true,
        )
    } else {
        (PathBuf::from(first), false)
    };
    let media = args
        .next()
        .map(PathBuf::from)
        .ok_or_else(|| usage.to_string())?;
    let vars = args
        .next()
        .map(PathBuf::from)
        .ok_or_else(|| usage.to_string())?;
    if args.next().is_some() {
        return Err("bridgevm_pc_boot_live received extra arguments".to_string());
    }
    Ok(Arguments {
        firmware,
        media,
        vars,
        windows_raw,
    })
}
