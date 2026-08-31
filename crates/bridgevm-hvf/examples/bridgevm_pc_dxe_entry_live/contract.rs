use bridgevm_hvf::machine::bridgevm_pc as board;
use std::fmt;
#[path = "contract/firmware.rs"]
mod firmware;
#[path = "contract/hob.rs"]
mod hob;
#[path = "contract/pcie.rs"]
mod pcie;
#[path = "contract/pcie_display.rs"]
mod pcie_display;
#[path = "contract/probe_failure.rs"]
mod probe_failure;
#[path = "contract/result.rs"]
mod result;
#[path = "contract/runtime_services.rs"]
mod runtime_services;
#[path = "contract/system_table.rs"]
mod system_table;
#[cfg(test)]
#[path = "contract/tests.rs"]
mod tests;
#[path = "contract/variable_services.rs"]
mod variable_services;
pub use firmware::validate as validate_firmware;
pub use result::DxeResult;

pub const RAM_PAGES: usize = 8192;
pub const PROBE_TITLE: &str = "BridgeVM Virtual ARM PC variable restore probe: PASS";
pub const LIVE_PROOF: &str = "LIVE PROOF: a recreated HVF VM restored the non-volatile UEFI variable from the preserved vars backing";
const RESULT_OFFSET: usize = 0x1000;
const DXE_RESULT_OFFSET: usize = 0x2000;
const HOB_OFFSET: usize = 0x4000;
const FV_OFFSET: usize = 0x10_0000;
const FV_SIZE: usize = 0x10_0000;
const DXE_CORE_LOAD_OFFSET: u64 = 0x40_0000;
const DXE_CORE_ENTRY_OFFSET: u64 = 0x40_6bf4;

pub type SecResult = DxeResult;

pub fn result_gpa() -> Result<u64, String> {
    Ok(board::RAM_BASE + RESULT_OFFSET as u64)
}

pub const fn pcie_function_count() -> usize {
    pcie::FUNCTION_COUNT
}

fn bytes_at<const N: usize>(bytes: &[u8], offset: usize, label: &str) -> Result<[u8; N], String> {
    bytes
        .get(offset..offset + N)
        .ok_or_else(|| format!("{label} is outside mapped memory"))?
        .try_into()
        .map_err(|_| format!("{label} has the wrong size"))
}

fn u16_at(bytes: &[u8], offset: usize, label: &str) -> Result<u16, String> {
    Ok(u16::from_le_bytes(bytes_at(bytes, offset, label)?))
}

fn u32_at(bytes: &[u8], offset: usize, label: &str) -> Result<u32, String> {
    Ok(u32::from_le_bytes(bytes_at(bytes, offset, label)?))
}

fn u64_at(bytes: &[u8], offset: usize, label: &str) -> Result<u64, String> {
    Ok(u64::from_le_bytes(bytes_at(bytes, offset, label)?))
}

fn expect<T>(label: &str, actual: T, expected: T) -> Result<(), String>
where
    T: fmt::Debug + PartialEq,
{
    if actual == expected {
        Ok(())
    } else {
        Err(format!("{label} is {actual:?}; expected {expected:?}"))
    }
}

pub fn validate_dxe_result(
    ram: &[u8],
    expected_variable_state: variable_services::VariableState,
) -> Result<DxeResult, String> {
    let raw_dxe_stage = u32_at(ram, DXE_RESULT_OFFSET, "raw DXE stage")?;
    let result = ram
        .get(RESULT_OFFSET..RESULT_OFFSET + 24)
        .ok_or_else(|| "SEC result is outside probe RAM".to_string())?;
    hob::validate(ram, result, raw_dxe_stage)?;

    let dxe = ram
        .get(DXE_RESULT_OFFSET..DXE_RESULT_OFFSET + 208)
        .ok_or_else(|| "DXE result is outside probe RAM".to_string())?;
    probe_failure::check(dxe, raw_dxe_stage)?;
    expect(
        "DXE dispatch stage",
        u32_at(dxe, 0, "DXE dispatch stage")?,
        expected_variable_state.stage(),
    )?;
    let system_table = u64_at(dxe, 8, "DXE system-table pointer")?;
    let published = system_table::validate(ram, system_table)?;
    let runtime = runtime_services::validate(ram, system_table, dxe)?;
    let variable =
        variable_services::validate(ram, runtime.services, dxe, expected_variable_state)?;
    let pcie = pcie::validate(dxe)?;
    Ok(DxeResult {
        system_table,
        published,
        runtime,
        variable,
        pcie,
    })
}

pub fn validate_sec_result(
    ram: &[u8],
    expected_variable_state: variable_services::VariableState,
) -> Result<SecResult, String> {
    validate_dxe_result(ram, expected_variable_state)
}

pub use variable_services::{validate_store as validate_variable_store, VariableState};
