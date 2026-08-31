use super::{
    bytes_at, expect, u16_at, u32_at, u64_at, DXE_CORE_ENTRY_OFFSET, DXE_CORE_LOAD_OFFSET,
    FV_OFFSET, FV_SIZE, HOB_OFFSET,
};
use bridgevm_hvf::machine::bridgevm_pc as board;

fn header(bytes: &[u8], offset: usize, kind: u16, size: u16, label: &str) -> Result<(), String> {
    expect(label, u16_at(bytes, offset, label)?, kind)?;
    expect(label, u16_at(bytes, offset + 2, label)?, size)?;
    expect(label, u32_at(bytes, offset + 4, label)?, 0)
}

pub fn validate(ram: &[u8], result: &[u8], raw_dxe_stage: u32) -> Result<(), String> {
    expect("SEC stage", u32_at(result, 0, "SEC stage")?, 1)?;
    expect(
        &format!("SEC HOB count with raw DXE stage {raw_dxe_stage}"),
        u32_at(result, 4, "SEC HOB count")?,
        8,
    )?;
    expect(
        "SEC HOB GPA",
        u64_at(result, 8, "SEC HOB GPA")?,
        board::RAM_BASE + HOB_OFFSET as u64,
    )?;
    expect("SEC HOB size", u32_at(result, 16, "SEC HOB size")?, 320)?;
    expect("SEC reserved", u32_at(result, 20, "SEC reserved")?, 0)?;
    let hob = ram
        .get(HOB_OFFSET..HOB_OFFSET + 320)
        .ok_or_else(|| "DXE HOB list is outside probe RAM".to_string())?;
    header(hob, 0, 1, 56, "PHIT header")?;
    expect(
        "PHIT end",
        u64_at(hob, 48, "PHIT end")?,
        board::RAM_BASE + HOB_OFFSET as u64 + 312,
    )?;
    header(hob, 56, 3, 48, "resource HOB")?;
    header(hob, 104, 2, 48, "stack HOB")?;
    header(hob, 152, 2, 48, "page-table allocation HOB")?;
    expect(
        "page-table allocation base",
        u64_at(hob, 176, "page-table allocation base")?,
        board::RAM_BASE + 0x30_000,
    )?;
    expect(
        "page-table allocation size",
        u64_at(hob, 184, "page-table allocation size")?,
        0x3000,
    )?;
    expect(
        "page-table memory type",
        u32_at(hob, 192, "page-table memory type")?,
        0,
    )?;
    header(hob, 200, 6, 16, "CPU HOB")?;
    header(hob, 216, 5, 24, "FV HOB")?;
    expect(
        "FV HOB base",
        u64_at(hob, 224, "FV HOB base")?,
        FV_OFFSET as u64,
    )?;
    expect(
        "FV HOB length",
        u64_at(hob, 232, "FV HOB length")?,
        FV_SIZE as u64,
    )?;
    header(hob, 240, 2, 72, "module HOB")?;
    expect(
        "module-allocation GUID",
        bytes_at::<16>(hob, 248, "module-allocation GUID")?,
        [
            0x75, 0x19, 0xe2, 0xf8, 0x99, 0x08, 0x58, 0x4f, 0xa4, 0xbe, 0x55, 0x25, 0xa9, 0xc6,
            0xd7, 0x7a,
        ],
    )?;
    expect(
        "DXE image base",
        u64_at(hob, 264, "DXE image base")?,
        board::RAM_BASE + DXE_CORE_LOAD_OFFSET,
    )?;
    expect(
        "DXE image size",
        u64_at(hob, 272, "DXE image size")?,
        0x17000,
    )?;
    expect("DXE memory type", u32_at(hob, 280, "DXE memory type")?, 3)?;
    expect(
        "DXE Core module GUID",
        bytes_at::<16>(hob, 288, "DXE Core module GUID")?,
        [
            0x7f, 0xcb, 0xa2, 0xd6, 0x18, 0x6a, 0x2f, 0x4e, 0xb4, 0x3b, 0x99, 0x20, 0xa7, 0x33,
            0x70, 0x0a,
        ],
    )?;
    expect(
        "DXE entry",
        u64_at(hob, 304, "DXE entry")?,
        board::RAM_BASE + DXE_CORE_ENTRY_OFFSET,
    )?;
    header(hob, 312, 0xffff, 8, "end HOB")
}
