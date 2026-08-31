//! Stable human-readable records for live proof and diagnostic modes.

use super::boot_media::MediaIdentity;
use super::result::BootResult;
use super::Execution;
use bridgevm_hvf::machine::bridgevm_pc as board;

#[path = "report_proof.rs"]
pub(super) mod proof;
#[path = "report_stage.rs"]
mod stage;
#[path = "report_state.rs"]
mod state;

pub(super) fn windows(
    hashes: (&str, &str),
    media: MediaIdentity,
    ram: &[u8],
    execution: Execution,
) -> Result<(), String> {
    let (firmware_hash, vars_hash) = hashes;
    // Prefer the live RAM; if the Boot Manager has run far enough to overwrite
    // the low-memory result record, fall back to the snapshot captured the
    // moment the StartImage handoff first became valid.
    let boot = match super::result::validate_windows_start(ram) {
        Ok(boot) => Ok(boot),
        Err(error) => execution.result_snapshot.clone().ok_or(error),
    };
    let advanced = boot.is_ok() && super::result::validate_windows_start(ram).is_err();
    let complete = boot.is_ok();
    let termination = match execution.run {
        Ok((mmio, vtimer)) => format!("hvc mmio_exits={mmio} vtimer_exits={vtimer}"),
        Err(error) => error,
    };
    println!(
        "BridgeVM Virtual ARM PC Windows Boot Manager diagnostic: {}",
        if complete { "COMPLETE" } else { "INCOMPLETE" }
    );
    println!("board={} abi={}", board::BOARD_ID, board::BOARD_ABI_VERSION);
    println!(
        "firmware_sha256={firmware_hash} boot_media_bytes={} boot_media_mode=raw-cow ram_mib={} vars_sha256={vars_hash}",
        media.byte_len,
        media.ram_bytes >> 20
    );
    match boot {
        Ok(boot) => println!("{}", stage::line(&boot)),
        Err(error) => println!("boot_observation_error={error:?}"),
    }
    println!("termination={termination:?}");
    state::write(&execution.state, &execution.serial, ram);
    println!("windows_boot_proven=false");
    if complete {
        if advanced {
            println!("boot_manager_advanced=true (overwrote the low-memory result record after StartImage)");
        }
        println!("LIVE OBSERVATION: BDS loaded Windows BOOTAA64.EFI and StartImage did not return before the captured terminal boundary");
        Ok(())
    } else {
        Err("Windows Boot Manager StartImage handoff was not observed".to_string())
    }
}
