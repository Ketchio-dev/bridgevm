use bridgevm_hvf::machine::bridgevm_pc as board;
use std::fmt;
#[path = "contract/firmware.rs"]
mod firmware;
#[path = "contract/system_table.rs"]
mod system_table;
pub use firmware::validate as validate_firmware;

pub const RAM_PAGES: usize = 8192;
pub const RAM_EXECUTABLE: bool = true;
pub const PROBE_TITLE: &str = "BridgeVM Virtual ARM PC platform-table DXE probe: PASS";
pub const LIVE_PROOF: &str =
    "LIVE PROOF: BridgeVM published ACPI 2.0 and SMBIOS 3 through the EFI system table";
const RESULT_OFFSET: usize = 0x1000;
const DXE_RESULT_OFFSET: usize = 0x2000;
const HOB_OFFSET: usize = 0x4000;
const FV_OFFSET: usize = 0x10_0000;
const FV_SIZE: usize = 0x10_0000;
const DXE_CORE_LOAD_OFFSET: u64 = 0x40_0000;
const DXE_CORE_ENTRY_OFFSET: u64 = 0x40_6bec;

#[derive(Debug, Eq, PartialEq)]
pub struct DxeResult {
    system_table: u64,
    published: system_table::PublishedTables,
}

pub type SecResult = DxeResult;

pub fn result_gpa() -> Result<u64, String> {
    Ok(board::RAM_BASE + RESULT_OFFSET as u64)
}

impl fmt::Display for DxeResult {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "sec_result=1 hob_count=7 hob_list_gpa={:#x} hob_list_size=272 dxe_result=8 system_table={:#x} configuration_entries={} acpi={:#x} smbios={:#x}",
            board::RAM_BASE + HOB_OFFSET as u64,
            self.system_table,
            self.published.entry_count,
            self.published.acpi,
            self.published.smbios
        )
    }
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

fn header(bytes: &[u8], offset: usize, kind: u16, size: u16, label: &str) -> Result<(), String> {
    expect(label, u16_at(bytes, offset, label)?, kind)?;
    expect(label, u16_at(bytes, offset + 2, label)?, size)?;
    expect(label, u32_at(bytes, offset + 4, label)?, 0)
}

pub fn validate_dxe_result(ram: &[u8]) -> Result<DxeResult, String> {
    let raw_dxe_stage = u32_at(ram, DXE_RESULT_OFFSET, "raw DXE stage")?;
    let result = ram
        .get(RESULT_OFFSET..RESULT_OFFSET + 24)
        .ok_or_else(|| "SEC result is outside probe RAM".to_string())?;
    expect("SEC stage", u32_at(result, 0, "SEC stage")?, 1)?;
    expect(
        &format!("SEC HOB count with raw DXE stage {raw_dxe_stage}"),
        u32_at(result, 4, "SEC HOB count")?,
        7,
    )?;
    expect(
        "SEC HOB GPA",
        u64_at(result, 8, "SEC HOB GPA")?,
        board::RAM_BASE + HOB_OFFSET as u64,
    )?;
    expect("SEC HOB size", u32_at(result, 16, "SEC HOB size")?, 272)?;
    expect("SEC reserved", u32_at(result, 20, "SEC reserved")?, 0)?;

    let hob = ram
        .get(HOB_OFFSET..HOB_OFFSET + 272)
        .ok_or_else(|| "DXE HOB list is outside probe RAM".to_string())?;
    header(hob, 0, 1, 56, "PHIT header")?;
    expect(
        "PHIT end",
        u64_at(hob, 48, "PHIT end")?,
        board::RAM_BASE + HOB_OFFSET as u64 + 264,
    )?;
    header(hob, 56, 3, 48, "resource HOB")?;
    header(hob, 104, 2, 48, "stack HOB")?;
    header(hob, 152, 6, 16, "CPU HOB")?;
    header(hob, 168, 5, 24, "FV HOB")?;
    expect(
        "FV HOB base",
        u64_at(hob, 176, "FV HOB base")?,
        FV_OFFSET as u64,
    )?;
    expect(
        "FV HOB length",
        u64_at(hob, 184, "FV HOB length")?,
        FV_SIZE as u64,
    )?;
    header(hob, 192, 2, 72, "module HOB")?;
    expect(
        "module-allocation GUID",
        bytes_at::<16>(hob, 200, "module-allocation GUID")?,
        [
            0x75, 0x19, 0xe2, 0xf8, 0x99, 0x08, 0x58, 0x4f, 0xa4, 0xbe, 0x55, 0x25, 0xa9, 0xc6,
            0xd7, 0x7a,
        ],
    )?;
    expect(
        "DXE image base",
        u64_at(hob, 216, "DXE image base")?,
        board::RAM_BASE + DXE_CORE_LOAD_OFFSET,
    )?;
    expect(
        "DXE image size",
        u64_at(hob, 224, "DXE image size")?,
        0x17000,
    )?;
    expect("DXE memory type", u32_at(hob, 232, "DXE memory type")?, 3)?;
    expect(
        "DXE Core module GUID",
        bytes_at::<16>(hob, 240, "DXE Core module GUID")?,
        [
            0x7f, 0xcb, 0xa2, 0xd6, 0x18, 0x6a, 0x2f, 0x4e, 0xb4, 0x3b, 0x99, 0x20, 0xa7, 0x33,
            0x70, 0x0a,
        ],
    )?;
    expect(
        "DXE entry",
        u64_at(hob, 256, "DXE entry")?,
        board::RAM_BASE + DXE_CORE_ENTRY_OFFSET,
    )?;
    header(hob, 264, 0xffff, 8, "end HOB")?;

    let dxe = ram
        .get(DXE_RESULT_OFFSET..DXE_RESULT_OFFSET + 16)
        .ok_or_else(|| "DXE result is outside probe RAM".to_string())?;
    expect(
        "DXE dispatch stage",
        u32_at(dxe, 0, "DXE dispatch stage")?,
        8,
    )?;
    expect(
        "DXE result reserved",
        u32_at(dxe, 4, "DXE result reserved")?,
        0,
    )?;
    let system_table = u64_at(dxe, 8, "DXE system-table pointer")?;
    let published = system_table::validate(ram, system_table)?;
    Ok(DxeResult {
        system_table,
        published,
    })
}

pub fn validate_sec_result(ram: &[u8]) -> Result<SecResult, String> {
    validate_dxe_result(ram)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_contract_firmware_size() {
        assert!(validate_firmware(&[0; 64]).unwrap_err().contains("FD size"));
    }

    #[test]
    fn rejects_a_missing_dxe_dispatch_result() {
        let ram = vec![0; 0x80_0000];
        assert!(validate_dxe_result(&ram).unwrap_err().contains("SEC stage"));
    }
}
