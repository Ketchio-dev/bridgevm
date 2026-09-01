use super::{expect, u32_at, u64_at};
use bridgevm_hvf::machine::bridgevm_pc as board;

pub const PROBE_CRC32: u32 = 0x3f6f_728d;
const RUNTIME_SERVICES_OFFSET: usize = 88;
const BOOT_SERVICES_OFFSET: usize = 96;
const SET_VIRTUAL_ADDRESS_MAP_OFFSET: usize = 56;
const CONVERT_POINTER_OFFSET: usize = 64;
const CALCULATE_CRC32_OFFSET: usize = 344;
const EFI_RUNTIME_SERVICES_SIGNATURE: u64 = 0x5652_4553_544e_5552;
const EFI_BOOT_SERVICES_SIGNATURE: u64 = 0x5652_4553_544f_4f42;

#[derive(Debug, Eq, PartialEq)]
pub struct RuntimeProof {
    pub services: u64,
    pub protocol: u64,
    pub set_virtual_address_map: u64,
    pub convert_pointer: u64,
    pub calculate_crc32: u64,
    pub crc32: u32,
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

pub fn validate(ram: &[u8], system_table: u64, result: &[u8]) -> Result<RuntimeProof, String> {
    expect(
        "RuntimeDxe CRC32 proof",
        u32_at(result, 4, "RuntimeDxe CRC32 proof")?,
        PROBE_CRC32,
    )?;
    let services = u64_at(result, 16, "runtime-services proof pointer")?;
    let protocol = u64_at(result, 24, "runtime protocol proof pointer")?;
    let set_virtual_address_map = u64_at(result, 32, "SetVirtualAddressMap proof pointer")?;
    let convert_pointer = u64_at(result, 40, "ConvertPointer proof pointer")?;
    let calculate_crc32 = u64_at(result, 48, "CalculateCrc32 proof pointer")?;
    let system_offset = ram_offset(ram, system_table, "EFI system table")?;
    expect(
        "EFI runtime-services pointer",
        u64_at(
            ram,
            system_offset + RUNTIME_SERVICES_OFFSET,
            "runtime-services pointer",
        )?,
        services,
    )?;
    let boot_services = u64_at(
        ram,
        system_offset + BOOT_SERVICES_OFFSET,
        "boot-services pointer",
    )?;
    let runtime_offset = ram_offset(ram, services, "EFI runtime services")?;
    let boot_offset = ram_offset(ram, boot_services, "EFI boot services")?;
    ram_offset(ram, protocol, "Runtime Architectural Protocol")?;
    for (pointer, label) in [
        (set_virtual_address_map, "SetVirtualAddressMap"),
        (convert_pointer, "ConvertPointer"),
        (calculate_crc32, "CalculateCrc32"),
    ] {
        ram_offset(ram, pointer, label)?;
    }
    expect(
        "EFI runtime-services signature",
        u64_at(ram, runtime_offset, "runtime-services signature")?,
        EFI_RUNTIME_SERVICES_SIGNATURE,
    )?;
    expect(
        "EFI boot-services signature",
        u64_at(ram, boot_offset, "boot-services signature")?,
        EFI_BOOT_SERVICES_SIGNATURE,
    )?;
    expect(
        "SetVirtualAddressMap table pointer",
        u64_at(
            ram,
            runtime_offset + SET_VIRTUAL_ADDRESS_MAP_OFFSET,
            "SetVirtualAddressMap",
        )?,
        set_virtual_address_map,
    )?;
    expect(
        "ConvertPointer table pointer",
        u64_at(
            ram,
            runtime_offset + CONVERT_POINTER_OFFSET,
            "ConvertPointer",
        )?,
        convert_pointer,
    )?;
    expect(
        "CalculateCrc32 table pointer",
        u64_at(ram, boot_offset + CALCULATE_CRC32_OFFSET, "CalculateCrc32")?,
        calculate_crc32,
    )?;
    Ok(RuntimeProof {
        services,
        protocol,
        set_virtual_address_map,
        convert_pointer,
        calculate_crc32,
        crc32: PROBE_CRC32,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn put_u32(bytes: &mut [u8], offset: usize, value: u32) {
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    fn put_u64(bytes: &mut [u8], offset: usize, value: u64) {
        bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
    }

    fn fixture() -> (Vec<u8>, Vec<u8>, u64) {
        let mut ram = vec![0; 0x1000];
        let mut result = vec![0; 56];
        let system = 0x100;
        let runtime = 0x200;
        let boot = 0x300;
        let protocol = 0x400;
        let functions = [0x500, 0x508, 0x510];
        put_u32(&mut result, 4, PROBE_CRC32);
        for (offset, value) in [
            (16, runtime),
            (24, protocol),
            (32, functions[0]),
            (40, functions[1]),
            (48, functions[2]),
        ] {
            put_u64(&mut result, offset, board::RAM_BASE + value as u64);
        }
        put_u64(
            &mut ram,
            system + RUNTIME_SERVICES_OFFSET,
            board::RAM_BASE + runtime as u64,
        );
        put_u64(
            &mut ram,
            system + BOOT_SERVICES_OFFSET,
            board::RAM_BASE + boot as u64,
        );
        put_u64(&mut ram, runtime, EFI_RUNTIME_SERVICES_SIGNATURE);
        put_u64(&mut ram, boot, EFI_BOOT_SERVICES_SIGNATURE);
        put_u64(
            &mut ram,
            runtime + SET_VIRTUAL_ADDRESS_MAP_OFFSET,
            board::RAM_BASE + functions[0] as u64,
        );
        put_u64(
            &mut ram,
            runtime + CONVERT_POINTER_OFFSET,
            board::RAM_BASE + functions[1] as u64,
        );
        put_u64(
            &mut ram,
            boot + CALCULATE_CRC32_OFFSET,
            board::RAM_BASE + functions[2] as u64,
        );
        (ram, result, board::RAM_BASE + system as u64)
    }

    #[test]
    fn accepts_matching_runtime_protocol_and_service_tables() {
        let (ram, result, system) = fixture();
        assert_eq!(validate(&ram, system, &result).unwrap().crc32, PROBE_CRC32);
    }

    #[test]
    fn rejects_a_runtime_table_without_the_standard_signature() {
        let (mut ram, result, system) = fixture();
        put_u64(&mut ram, 0x200, 0);
        assert!(validate(&ram, system, &result)
            .unwrap_err()
            .contains("signature"));
    }
}
