//! Decoder for the BridgeVM PC firmware-to-host boot observation record.

const OFFSET: usize = 0x3000;
const MAGIC: u64 = 0x544f_4f42_5043_4d42;
const VERSION: u32 = 2;
const LENGTH: usize = 144;

fn u32_at(bytes: &[u8], offset: usize) -> Result<u32, String> {
    let raw = bytes.get(offset..offset + 4);
    let raw = raw.ok_or("boot result u32 is outside RAM")?;
    Ok(u32::from_le_bytes(raw.try_into().expect("bounded u32")))
}

fn u64_at(bytes: &[u8], offset: usize) -> Result<u64, String> {
    let raw = bytes.get(offset..offset + 8);
    let raw = raw.ok_or("boot result u64 is outside RAM")?;
    Ok(u64::from_le_bytes(raw.try_into().expect("bounded u64")))
}

#[derive(Debug)]
pub(crate) struct BootResult {
    pub(crate) stage: u32,
    pub(crate) status: u64,
    pub(crate) arch: u64,
    pub(crate) file_systems: u64,
    pub(crate) file_system_handle: u64,
    pub(crate) image_handle: u64,
    pub(crate) image_base: u64,
    pub(crate) image_size: u64,
    pub(crate) memory_map_size: u64,
    pub(crate) map_key: u64,
    pub(crate) descriptor_size: u64,
    pub(crate) descriptor_version: u32,
    pub(crate) exit_attempts: u32,
    pub(crate) system_table: u64,
    pub(crate) boot_services: u64,
    pub(crate) gop_handles: u64,
    pub(crate) framebuffer_base: u64,
    pub(crate) framebuffer_size: u64,
}

pub(super) fn read(ram: &[u8]) -> Result<BootResult, String> {
    let result = ram
        .get(OFFSET..OFFSET + LENGTH)
        .ok_or_else(|| "boot result is outside RAM".to_string())?;
    let magic = u64_at(result, 0)?;
    let version = u32_at(result, 8)?;
    if magic != MAGIC || version != VERSION {
        return Err(format!(
            "boot result identity is invalid: magic={magic:#x} version={version}"
        ));
    }
    Ok(BootResult {
        stage: u32_at(result, 12)?,
        status: u64_at(result, 16)?,
        arch: u64_at(result, 24)?,
        file_systems: u64_at(result, 32)?,
        file_system_handle: u64_at(result, 40)?,
        image_handle: u64_at(result, 48)?,
        image_base: u64_at(result, 56)?,
        image_size: u64_at(result, 64)?,
        memory_map_size: u64_at(result, 72)?,
        map_key: u64_at(result, 80)?,
        descriptor_size: u64_at(result, 88)?,
        descriptor_version: u32_at(result, 96)?,
        exit_attempts: u32_at(result, 100)?,
        system_table: u64_at(result, 104)?,
        boot_services: u64_at(result, 112)?,
        gop_handles: u64_at(result, 120)?,
        framebuffer_base: u64_at(result, 128)?,
        framebuffer_size: u64_at(result, 136)?,
    })
}
