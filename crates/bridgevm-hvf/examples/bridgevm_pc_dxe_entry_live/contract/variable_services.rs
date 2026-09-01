use super::{bytes_at, expect, u32_at, u64_at};
use bridgevm_hvf::machine::bridgevm_pc as board;

const GET_VARIABLE_OFFSET: usize = 72;
const SET_VARIABLE_OFFSET: usize = 88;
const QUERY_VARIABLE_INFO_OFFSET: usize = 128;
const RESULT_VARIABLE_OFFSET: usize = 56;
const REQUIRED_ATTRIBUTES: u32 = 7;
const STORE_SIZE: u32 = 0x1_0000;
const AUTHENTICATED_STORE_GUID: [u8; 16] = [
    0x78, 0x2c, 0xf3, 0xaa, 0x7b, 0x94, 0x9a, 0x43, 0xa1, 0x80, 0x2e, 0x14, 0x4e, 0xc3, 0x77, 0x92,
];

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum VariableState {
    Written,
    Restored,
}

impl VariableState {
    pub const fn raw(self) -> u32 {
        match self {
            Self::Written => 1,
            Self::Restored => 2,
        }
    }

    pub const fn stage(self) -> u32 {
        match self {
            Self::Written => 10,
            Self::Restored => 11,
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct VariableProof {
    pub state: VariableState,
    pub attributes: u32,
    pub maximum_storage: u64,
    pub remaining_storage: u64,
    pub maximum_variable_size: u64,
    pub get_variable: u64,
    pub set_variable: u64,
    pub query_variable_info: u64,
}

fn ram_offset(ram: &[u8], gpa: u64, label: &str) -> Result<usize, String> {
    let offset = gpa
        .checked_sub(board::RAM_BASE)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| format!("{label} {gpa:#x} is below RAM"))?;
    if offset >= ram.len() {
        return Err(format!("{label} {gpa:#x} is outside mapped RAM"));
    }
    Ok(offset)
}

pub fn validate(
    ram: &[u8],
    runtime_services: u64,
    result: &[u8],
    expected: VariableState,
) -> Result<VariableProof, String> {
    let offset = RESULT_VARIABLE_OFFSET;
    expect(
        "variable proof state",
        u32_at(result, offset, "variable proof state")?,
        expected.raw(),
    )?;
    let attributes = u32_at(result, offset + 4, "variable attributes")?;
    expect("variable attributes", attributes, REQUIRED_ATTRIBUTES)?;
    let get_variable = u64_at(result, offset + 8, "GetVariable proof pointer")?;
    let set_variable = u64_at(result, offset + 16, "SetVariable proof pointer")?;
    let query_variable_info = u64_at(result, offset + 24, "QueryVariableInfo proof pointer")?;
    let maximum_storage = u64_at(result, offset + 32, "maximum variable storage")?;
    let remaining_storage = u64_at(result, offset + 40, "remaining variable storage")?;
    let maximum_variable_size = u64_at(result, offset + 48, "maximum variable size")?;
    if maximum_storage < 0x1000 || remaining_storage > maximum_storage || maximum_variable_size < 16
    {
        return Err("variable quota result is outside the bounded contract".to_string());
    }
    let runtime_offset = ram_offset(ram, runtime_services, "EFI runtime services")?;
    for (pointer, table_offset, label) in [
        (get_variable, GET_VARIABLE_OFFSET, "GetVariable"),
        (set_variable, SET_VARIABLE_OFFSET, "SetVariable"),
        (
            query_variable_info,
            QUERY_VARIABLE_INFO_OFFSET,
            "QueryVariableInfo",
        ),
    ] {
        ram_offset(ram, pointer, label)?;
        expect(
            &format!("{label} table pointer"),
            u64_at(ram, runtime_offset + table_offset, label)?,
            pointer,
        )?;
    }
    Ok(VariableProof {
        state: expected,
        attributes,
        maximum_storage,
        remaining_storage,
        maximum_variable_size,
        get_variable,
        set_variable,
        query_variable_info,
    })
}

pub fn validate_store(store: &[u8]) -> Result<(), String> {
    expect("vars backing size", store.len(), STORE_SIZE as usize)?;
    expect(
        "authenticated variable-store GUID",
        bytes_at::<16>(store, 0, "variable-store GUID")?,
        AUTHENTICATED_STORE_GUID,
    )?;
    expect(
        "variable-store declared size",
        u32_at(store, 16, "variable-store size")?,
        STORE_SIZE,
    )?;
    expect("variable-store format", store[20], 0x5a)?;
    expect("variable-store health", store[21], 0xfe)?;
    if !store[28..].windows(2).any(|window| window == [0xaa, 0x55]) {
        return Err("vars backing contains no complete variable header".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_an_erased_vars_backing() {
        assert!(validate_store(&vec![0xff; STORE_SIZE as usize])
            .unwrap_err()
            .contains("GUID"));
    }
}
