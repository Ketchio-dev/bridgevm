use super::aligned_memory::{AlignedMemory, PAGE_ALIGNMENT};
use super::{board, contract, result_gpa, run_fresh_vm, sha256, BridgeVmPcPlatform};
use super::{validate_variable_store, VariableState};

pub(super) unsafe fn run(firmware_bytes: &[u8], firmware_sha256: &str) -> Result<(), String> {
    let bundle = BridgeVmPcPlatform::build_firmware_tables(1, 512 << 20)?;
    let mut firmware = AlignedMemory::new(firmware_bytes.len())?;
    let mut variables = AlignedMemory::new(PAGE_ALIGNMENT)?;
    let mut boot_info = AlignedMemory::new(board::BOOT_INFO.size as usize)?;
    firmware.bytes_mut().copy_from_slice(firmware_bytes);
    variables.bytes_mut().fill(0xff);
    boot_info
        .bytes_mut()
        .copy_from_slice(&bundle.boot_info.bytes);

    let initial_vars_sha256 = sha256(variables.bytes_mut());
    let mut results = Vec::new();
    let mut vars_hashes = Vec::new();
    let mut boot_rams = Vec::new();
    for _ in 0..2 {
        boot_rams.push(AlignedMemory::new(PAGE_ALIGNMENT * contract::RAM_PAGES)?);
    }
    for (expected_state, ram) in [VariableState::Written, VariableState::Restored]
        .into_iter()
        .zip(boot_rams.iter_mut())
    {
        let result = run_fresh_vm(
            &mut firmware,
            &mut variables,
            &mut boot_info,
            ram,
            expected_state,
        )?;
        validate_variable_store(variables.bytes_mut())?;
        vars_hashes.push(sha256(variables.bytes_mut()));
        results.push(result);
    }
    if initial_vars_sha256 == vars_hashes[0] || vars_hashes[0] != vars_hashes[1] {
        return Err("vars backing did not change once and then remain stable".to_string());
    }
    let result = results
        .pop()
        .ok_or_else(|| "second variable-service boot is missing".to_string())?;
    println!("{}", contract::PROBE_TITLE);
    println!("board={} abi={}", board::BOARD_ID, board::BOARD_ABI_VERSION);
    println!(
        "reset_vector={:#x} firmware_size={:#x} ram={:#x}",
        board::FLASH_CODE.base,
        board::FLASH_CODE.size,
        board::RAM_BASE
    );
    println!(
        "{result} result_gpa={:#x} boot_info={:#x}",
        result_gpa()?,
        board::BOOT_INFO.base
    );
    println!("firmware_sha256={firmware_sha256}");
    println!("vars_initial_sha256={initial_vars_sha256}");
    println!("vars_written_sha256={}", vars_hashes[0]);
    println!("vars_restored_sha256={}", vars_hashes[1]);
    println!("{}", contract::LIVE_PROOF);
    Ok(())
}
