use super::aligned_memory::{AlignedMemory, PAGE_ALIGNMENT};
use super::command::Expectation;
use super::{board, contract, run_fresh_vm, sha256, vars_file, BridgeVmPcPlatform};
use super::{validate_variable_store, VariableState};
use std::path::Path;

pub(super) unsafe fn run(
    firmware_bytes: &[u8],
    firmware_sha256: &str,
    path: &Path,
    expectation: Expectation,
) -> Result<(), String> {
    let initial = vars_file::load(path, expectation)?;
    let initial_hash = sha256(&initial);
    let bundle = BridgeVmPcPlatform::build_firmware_tables(1, 512 << 20)?;
    let mut firmware = AlignedMemory::new(firmware_bytes.len())?;
    let mut variables = AlignedMemory::new(PAGE_ALIGNMENT)?;
    let mut boot_info = AlignedMemory::new(board::BOOT_INFO.size as usize)?;
    let mut ram = AlignedMemory::new(PAGE_ALIGNMENT * contract::RAM_PAGES)?;
    firmware.bytes_mut().copy_from_slice(firmware_bytes);
    variables.bytes_mut().copy_from_slice(&initial);
    boot_info
        .bytes_mut()
        .copy_from_slice(&bundle.boot_info.bytes);

    let expected_state = match expectation {
        Expectation::Written => VariableState::Written,
        Expectation::Restored => VariableState::Restored,
    };
    let result = run_fresh_vm(
        &mut firmware,
        &mut variables,
        &mut boot_info,
        &mut ram,
        expected_state,
    )?;
    validate_variable_store(variables.bytes_mut())?;
    let final_bytes = variables.bytes_mut();
    let final_hash = sha256(final_bytes);
    match expectation {
        Expectation::Written if initial_hash == final_hash => {
            return Err("written-mode vars backing did not change".to_string())
        }
        Expectation::Written => vars_file::persist_new(path, final_bytes)?,
        Expectation::Restored if initial_hash != final_hash => {
            return Err("restored-mode vars backing changed".to_string())
        }
        Expectation::Restored => {}
    }
    let file_bytes = vars_file::read_existing(path)?;
    let file_hash = sha256(&file_bytes);
    if file_hash != final_hash {
        return Err("vars file differs from the validated guest backing".to_string());
    }

    println!("BridgeVM Virtual ARM PC variable-file stage: PASS");
    println!("board={} abi={}", board::BOARD_ID, board::BOARD_ABI_VERSION);
    println!("{result}");
    println!("firmware_sha256={firmware_sha256}");
    println!("process_mode={}", expectation.label());
    println!("vars_loaded_sha256={initial_hash}");
    println!("vars_final_sha256={final_hash}");
    println!("vars_file_sha256={file_hash}");
    println!("LIVE PROOF: one HVF VM completed the requested variable-file stage");
    Ok(())
}
