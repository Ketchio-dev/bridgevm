use super::{PASS_RESULT, RESULT_OFFSET};
use bridgevm_hvf::machine::bridgevm_pc as board;
use std::fmt;

const RESULT_SIZE: usize = 24;
const HOB_OFFSET: usize = 0x4000;
const HOB_SIZE: usize = 176;
const HOB_COUNT: u32 = 5;
const STACK_BASE: u64 = 0x1_0001_0000;
const STACK_SIZE: u64 = 0x1_0000;
const FREE_MEMORY_BOTTOM: u64 = 0x1_0003_0000;
const RAM_SIZE: u64 = 512 << 20;

#[derive(Debug, Eq, PartialEq)]
pub struct SecResult {
    stage: u32,
    hob_count: u32,
    hob_list_gpa: u64,
    hob_list_size: u32,
}

impl fmt::Display for SecResult {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "result={} hob_count={} hob_list_gpa={:#x} hob_list_size={}",
            self.stage, self.hob_count, self.hob_list_gpa, self.hob_list_size
        )
    }
}

fn bytes_at<const N: usize>(bytes: &[u8], offset: usize, label: &str) -> Result<[u8; N], String> {
    bytes
        .get(offset..offset + N)
        .ok_or_else(|| format!("{label} is outside probe RAM"))?
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

pub fn validate_sec_result(ram: &[u8]) -> Result<SecResult, String> {
    let result = ram
        .get(RESULT_OFFSET..RESULT_OFFSET + RESULT_SIZE)
        .ok_or_else(|| "SEC result is outside probe RAM".to_string())?;
    let parsed = SecResult {
        stage: u32_at(result, 0, "SEC stage")?,
        hob_count: u32_at(result, 4, "SEC HOB count")?,
        hob_list_gpa: u64_at(result, 8, "SEC HOB-list GPA")?,
        hob_list_size: u32_at(result, 16, "SEC HOB-list size")?,
    };
    expect("SEC stage", parsed.stage, PASS_RESULT)?;
    expect("SEC HOB count", parsed.hob_count, HOB_COUNT)?;
    expect(
        "SEC HOB-list GPA",
        parsed.hob_list_gpa,
        board::RAM_BASE + HOB_OFFSET as u64,
    )?;
    expect("SEC HOB-list size", parsed.hob_list_size, HOB_SIZE as u32)?;
    expect(
        "SEC result reserved",
        u32_at(result, 20, "SEC result reserved")?,
        0,
    )?;

    let hob = ram
        .get(HOB_OFFSET..HOB_OFFSET + HOB_SIZE)
        .ok_or_else(|| "PI HOB list is outside probe RAM".to_string())?;
    header(hob, 0, 1, 56, "PHIT header")?;
    expect("PHIT version", u32_at(hob, 8, "PHIT version")?, 9)?;
    expect("PHIT boot mode", u32_at(hob, 12, "PHIT boot mode")?, 0)?;
    expect(
        "PHIT memory top",
        u64_at(hob, 16, "PHIT memory top")?,
        board::RAM_BASE + RAM_SIZE,
    )?;
    expect(
        "PHIT memory bottom",
        u64_at(hob, 24, "PHIT memory bottom")?,
        board::RAM_BASE,
    )?;
    expect(
        "PHIT free top",
        u64_at(hob, 32, "PHIT free top")?,
        board::RAM_BASE + RAM_SIZE,
    )?;
    expect(
        "PHIT free bottom",
        u64_at(hob, 40, "PHIT free bottom")?,
        FREE_MEMORY_BOTTOM,
    )?;
    expect(
        "PHIT end pointer",
        u64_at(hob, 48, "PHIT end pointer")?,
        board::RAM_BASE + HOB_OFFSET as u64 + 168,
    )?;

    header(hob, 56, 3, 48, "resource HOB header")?;
    expect(
        "resource owner",
        bytes_at::<16>(hob, 64, "resource owner")?,
        [0; 16],
    )?;
    expect("resource type", u32_at(hob, 80, "resource type")?, 0)?;
    expect(
        "resource attributes",
        u32_at(hob, 84, "resource attributes")?,
        0x2007,
    )?;
    expect(
        "resource start",
        u64_at(hob, 88, "resource start")?,
        board::RAM_BASE,
    )?;
    expect(
        "resource length",
        u64_at(hob, 96, "resource length")?,
        RAM_SIZE,
    )?;

    header(hob, 104, 2, 48, "stack HOB header")?;
    expect(
        "stack GUID",
        bytes_at::<16>(hob, 112, "stack GUID")?,
        [
            0x27, 0xbf, 0xd4, 0x4e, 0x92, 0x40, 0xe9, 0x42, 0x80, 0x7d, 0x52, 0x7b, 0x1d, 0, 0xc9,
            0xbd,
        ],
    )?;
    expect("stack base", u64_at(hob, 128, "stack base")?, STACK_BASE)?;
    expect("stack size", u64_at(hob, 136, "stack size")?, STACK_SIZE)?;
    expect(
        "stack memory type",
        u32_at(hob, 144, "stack memory type")?,
        4,
    )?;
    expect("stack reserved", u32_at(hob, 148, "stack reserved")?, 0)?;

    header(hob, 152, 6, 16, "CPU HOB header")?;
    expect("CPU physical bits", hob[160], 40)?;
    expect("CPU I/O bits", hob[161], 0)?;
    expect(
        "CPU reserved",
        bytes_at::<6>(hob, 162, "CPU reserved")?,
        [0; 6],
    )?;
    header(hob, 168, 0xffff, 8, "end HOB header")?;
    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn put<const N: usize>(ram: &mut [u8], offset: usize, value: [u8; N]) {
        ram[offset..offset + N].copy_from_slice(&value);
    }

    fn valid_ram() -> Vec<u8> {
        let mut ram = vec![0; HOB_OFFSET + HOB_SIZE];
        put(&mut ram, RESULT_OFFSET, PASS_RESULT.to_le_bytes());
        put(&mut ram, RESULT_OFFSET + 4, HOB_COUNT.to_le_bytes());
        put(
            &mut ram,
            RESULT_OFFSET + 8,
            (board::RAM_BASE + HOB_OFFSET as u64).to_le_bytes(),
        );
        put(
            &mut ram,
            RESULT_OFFSET + 16,
            (HOB_SIZE as u32).to_le_bytes(),
        );
        for (offset, kind, size) in [
            (0, 1_u16, 56_u16),
            (56, 3, 48),
            (104, 2, 48),
            (152, 6, 16),
            (168, 0xffff, 8),
        ] {
            put(&mut ram, HOB_OFFSET + offset, kind.to_le_bytes());
            put(&mut ram, HOB_OFFSET + offset + 2, size.to_le_bytes());
        }
        put(&mut ram, HOB_OFFSET + 8, 9_u32.to_le_bytes());
        put(
            &mut ram,
            HOB_OFFSET + 16,
            (board::RAM_BASE + RAM_SIZE).to_le_bytes(),
        );
        put(&mut ram, HOB_OFFSET + 24, board::RAM_BASE.to_le_bytes());
        put(
            &mut ram,
            HOB_OFFSET + 32,
            (board::RAM_BASE + RAM_SIZE).to_le_bytes(),
        );
        put(&mut ram, HOB_OFFSET + 40, FREE_MEMORY_BOTTOM.to_le_bytes());
        put(
            &mut ram,
            HOB_OFFSET + 48,
            (board::RAM_BASE + HOB_OFFSET as u64 + 168).to_le_bytes(),
        );
        put(&mut ram, HOB_OFFSET + 84, 0x2007_u32.to_le_bytes());
        put(&mut ram, HOB_OFFSET + 88, board::RAM_BASE.to_le_bytes());
        put(&mut ram, HOB_OFFSET + 96, RAM_SIZE.to_le_bytes());
        put(
            &mut ram,
            HOB_OFFSET + 112,
            [
                0x27, 0xbf, 0xd4, 0x4e, 0x92, 0x40, 0xe9, 0x42, 0x80, 0x7d, 0x52, 0x7b, 0x1d, 0,
                0xc9, 0xbd,
            ],
        );
        put(&mut ram, HOB_OFFSET + 128, STACK_BASE.to_le_bytes());
        put(&mut ram, HOB_OFFSET + 136, STACK_SIZE.to_le_bytes());
        put(&mut ram, HOB_OFFSET + 144, 4_u32.to_le_bytes());
        ram[HOB_OFFSET + 160] = 40;
        ram
    }

    #[test]
    fn accepts_the_exact_bounded_hob_contract() {
        let parsed = validate_sec_result(&valid_ram()).unwrap();
        assert_eq!(parsed.hob_count, 5);
    }

    #[test]
    fn rejects_a_corrupt_hob_field() {
        let mut ram = valid_ram();
        ram[HOB_OFFSET + 160] = 39;
        assert!(validate_sec_result(&ram)
            .unwrap_err()
            .contains("physical bits"));
    }
}
