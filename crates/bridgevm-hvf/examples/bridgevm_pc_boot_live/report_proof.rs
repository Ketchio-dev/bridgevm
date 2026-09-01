//! Stable output for the existing sealed ExitBootServices proof.

use super::*;

pub(crate) fn write(
    hashes: (&str, &str),
    media: MediaIdentity,
    boot: BootResult,
    execution: Execution,
) -> Result<(), String> {
    let (firmware_hash, vars_hash) = hashes;
    let (mmio, vtimer) = execution.run?;
    let media_hash = media
        .sha256
        .ok_or_else(|| "sealed probe media hash is absent".to_string())?;
    println!("BridgeVM Virtual ARM PC BDS/ESP/PE/ExitBootServices probe: PASS");
    println!("board={} abi={}", board::BOARD_ID, board::BOARD_ABI_VERSION);
    println!(
        "firmware_sha256={firmware_hash} boot_media_sha256={media_hash} vars_sha256={vars_hash}"
    );
    println!("{}", super::stage::line(&boot));
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
