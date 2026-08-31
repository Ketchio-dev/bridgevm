//! Stable human-readable records for live proof and diagnostic modes.

use super::boot_media::MediaIdentity;
use super::result::BootResult;
use super::Execution;
use bridgevm_hvf::machine::bridgevm_pc as board;

pub(super) fn windows(
    firmware_hash: &str,
    media: MediaIdentity,
    vars_hash: &str,
    boot: BootResult,
    execution: Execution,
) {
    let termination = match execution.run {
        Ok((mmio, vtimer)) => format!("hvc mmio_exits={mmio} vtimer_exits={vtimer}"),
        Err(error) => error,
    };
    println!("BridgeVM Virtual ARM PC Windows Boot Manager diagnostic: COMPLETE");
    println!("board={} abi={}", board::BOARD_ID, board::BOARD_ABI_VERSION);
    println!(
        "firmware_sha256={firmware_hash} boot_media_bytes={} boot_media_mode=raw-cow ram_mib={} vars_sha256={vars_hash}",
        media.byte_len,
        media.ram_bytes >> 20
    );
    println!(
        "stage={} arch={:#x} filesystems={} image={:#x}+{:#x}",
        boot.stage, boot.arch, boot.file_systems, boot.image_base, boot.image_size
    );
    println!("termination={termination:?}");
    println!(
        "vcpu_final=pc:{:#x},cpsr:{:#x},exit:{},esr:{:#x},va:{:#x},pa:{:#x}",
        execution.state.pc,
        execution.state.cpsr,
        execution.state.exit_reason,
        execution.state.syndrome,
        execution.state.virtual_address,
        execution.state.physical_address
    );
    if !execution.serial.is_empty() {
        println!("serial={:?}", execution.serial);
    }
    println!("windows_boot_proven=false");
    println!("LIVE OBSERVATION: BDS loaded Windows BOOTAA64.EFI and StartImage did not return before the captured terminal boundary");
}

pub(super) fn proof(
    firmware_hash: &str,
    media: MediaIdentity,
    vars_hash: &str,
    boot: BootResult,
    execution: Execution,
) -> Result<(), String> {
    let (mmio, vtimer) = execution.run?;
    let media_hash = media
        .sha256
        .ok_or_else(|| "sealed probe media hash is absent".to_string())?;
    println!("BridgeVM Virtual ARM PC BDS/ESP/PE/ExitBootServices probe: PASS");
    println!("board={} abi={}", board::BOARD_ID, board::BOARD_ABI_VERSION);
    println!(
        "firmware_sha256={firmware_hash} boot_media_sha256={media_hash} vars_sha256={vars_hash}"
    );
    println!(
        "stage={} arch={:#x} filesystems={} image={:#x}+{:#x}",
        boot.stage, boot.arch, boot.file_systems, boot.image_base, boot.image_size
    );
    println!(
        "memory_map={} descriptor={}/{} exit_boot_services_attempts={} mmio_exits={} vtimer_exits={}",
        boot.memory_map_size,
        boot.descriptor_size,
        boot.descriptor_version,
        boot.exit_attempts,
        mmio,
        vtimer
    );
    if !execution.serial.is_empty() {
        println!("serial={:?}", execution.serial);
    }
    println!("LIVE PROOF: DXE Core invoked BDS, which loaded BOOTAA64.EFI from NVMe and reached code after ExitBootServices");
    Ok(())
}
