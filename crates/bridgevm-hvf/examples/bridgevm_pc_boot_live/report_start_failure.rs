//! Bounded decoder for a boot image that returned from UEFI StartImage.

const OFFSET: usize = 0x3100;
const MAGIC: u64 = 0x3146_4953_5043_4d42;
const VERSION: u32 = 1;
const CAPACITY: usize = 96;
const HEADER: usize = 40;

fn u32_at(bytes: &[u8], offset: usize) -> Result<u32, String> {
    Ok(u32::from_le_bytes(
        bytes
            .get(offset..offset + 4)
            .ok_or_else(|| "StartImage diagnostic u32 is outside RAM".to_string())?
            .try_into()
            .map_err(|_| "StartImage diagnostic u32 has the wrong size".to_string())?,
    ))
}

fn u64_at(bytes: &[u8], offset: usize) -> Result<u64, String> {
    Ok(u64::from_le_bytes(
        bytes
            .get(offset..offset + 8)
            .ok_or_else(|| "StartImage diagnostic u64 is outside RAM".to_string())?
            .try_into()
            .map_err(|_| "StartImage diagnostic u64 has the wrong size".to_string())?,
    ))
}

#[derive(Debug, PartialEq, Eq)]
struct Failure {
    status: u64,
    exit_data_size: u64,
    exit_data_address: u64,
    units: Vec<u16>,
}

fn decode(ram: &[u8]) -> Result<Option<Failure>, String> {
    let bytes = ram
        .get(OFFSET..OFFSET + HEADER + CAPACITY * 2)
        .ok_or_else(|| "StartImage diagnostic is outside RAM".to_string())?;
    let magic = u64_at(bytes, 0)?;
    if magic == 0 {
        return Ok(None);
    }
    let version = u32_at(bytes, 8)?;
    let count = u32_at(bytes, 12)? as usize;
    if magic != MAGIC || version != VERSION || count > CAPACITY {
        return Err(format!(
            "StartImage diagnostic identity is invalid: magic={magic:#x} version={version} units={count}"
        ));
    }
    let units = (0..count)
        .map(|index| {
            let offset = HEADER + index * 2;
            Ok(u16::from_le_bytes(
                bytes[offset..offset + 2]
                    .try_into()
                    .map_err(|_| "StartImage UTF-16 unit has the wrong size".to_string())?,
            ))
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(Some(Failure {
        status: u64_at(bytes, 16)?,
        exit_data_size: u64_at(bytes, 24)?,
        exit_data_address: u64_at(bytes, 32)?,
        units,
    }))
}

pub(super) fn write(ram: &[u8]) {
    match decode(ram) {
        Ok(Some(failure)) => println!(
            "start_image_return=status:{:#x},exit_data_size:{},exit_data_address:{:#x},utf16_units:{},utf16_valid:{},text:{:?}",
            failure.status,
            failure.exit_data_size,
            failure.exit_data_address,
            failure.units.len(),
            String::from_utf16(&failure.units).is_ok(),
            String::from_utf16_lossy(&failure.units)
        ),
        Ok(None) => {}
        Err(error) => println!("start_image_return_error={error:?}"),
    }
}

#[cfg(test)]
#[path = "report_start_failure_tests.rs"]
mod tests;
