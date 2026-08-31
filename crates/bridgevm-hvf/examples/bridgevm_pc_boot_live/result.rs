use bridgevm_hvf::machine::bridgevm_pc as board;

const OFFSET: usize = 0x3000;
const MAGIC: u64 = 0x544f_4f42_5043_4d42;
const REQUIRED_ARCH: u64 = 0x0fff;
const POST_EXIT: u32 = 11;

fn u32_at(bytes: &[u8], offset: usize) -> Result<u32, String> {
    Ok(u32::from_le_bytes(
        bytes
            .get(offset..offset + 4)
            .ok_or_else(|| "boot result u32 is outside RAM".to_string())?
            .try_into()
            .map_err(|_| "boot result u32 has the wrong size".to_string())?,
    ))
}

fn u64_at(bytes: &[u8], offset: usize) -> Result<u64, String> {
    Ok(u64::from_le_bytes(
        bytes
            .get(offset..offset + 8)
            .ok_or_else(|| "boot result u64 is outside RAM".to_string())?
            .try_into()
            .map_err(|_| "boot result u64 has the wrong size".to_string())?,
    ))
}

#[derive(Debug)]
pub(super) struct BootResult {
    pub(super) stage: u32,
    pub(super) arch: u64,
    pub(super) file_systems: u64,
    pub(super) image_base: u64,
    pub(super) image_size: u64,
    pub(super) memory_map_size: u64,
    pub(super) descriptor_size: u64,
    pub(super) descriptor_version: u32,
    pub(super) exit_attempts: u32,
}

pub(super) fn validate(ram: &[u8]) -> Result<BootResult, String> {
    let result = ram
        .get(OFFSET..OFFSET + 128)
        .ok_or_else(|| "boot result is outside RAM".to_string())?;
    let magic = u64_at(result, 0)?;
    let version = u32_at(result, 8)?;
    let stage = u32_at(result, 12)?;
    let status = u64_at(result, 16)?;
    let arch = u64_at(result, 24)?;
    let file_systems = u64_at(result, 32)?;
    let file_system_handle = u64_at(result, 40)?;
    let image_handle = u64_at(result, 48)?;
    let image_base = u64_at(result, 56)?;
    let image_size = u64_at(result, 64)?;
    let memory_map_size = u64_at(result, 72)?;
    let map_key = u64_at(result, 80)?;
    let descriptor_size = u64_at(result, 88)?;
    let descriptor_version = u32_at(result, 96)?;
    let exit_attempts = u32_at(result, 100)?;
    let system_table = u64_at(result, 104)?;
    let boot_services = u64_at(result, 112)?;
    let ram_end = board::RAM_BASE + ram.len() as u64;
    let pointers_in_ram = [image_base, system_table, boot_services]
        .into_iter()
        .all(|value| (board::RAM_BASE..ram_end).contains(&value));
    if magic != MAGIC || version != 1 || stage != POST_EXIT || status != 0 {
        return Err(format!(
            "boot result header is invalid: magic={magic:#x} version={version} stage={stage:#x} status={status:#x} arch={arch:#x}"
        ));
    }
    if arch != REQUIRED_ARCH || file_systems == 0 || file_system_handle == 0 || image_handle == 0 {
        return Err(format!(
            "BDS evidence is incomplete: arch={arch:#x} fs={file_systems} fs_handle={file_system_handle:#x} image_handle={image_handle:#x}"
        ));
    }
    if !pointers_in_ram || image_size == 0 || image_base + image_size > ram_end {
        return Err("loaded-image or UEFI table pointers are outside RAM".to_string());
    }
    if memory_map_size == 0 || map_key == 0 || descriptor_size < 40 || descriptor_version != 1 {
        return Err(format!(
            "memory-map evidence is invalid: size={memory_map_size} key={map_key} descriptor={descriptor_size}/{descriptor_version}"
        ));
    }
    if !(1..=3).contains(&exit_attempts) {
        return Err(format!("ExitBootServices attempts is {exit_attempts}"));
    }
    Ok(BootResult {
        stage,
        arch,
        file_systems,
        image_base,
        image_size,
        memory_map_size,
        descriptor_size,
        descriptor_version,
        exit_attempts,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_u32(bytes: &mut [u8], offset: usize, value: u32) {
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    fn write_u64(bytes: &mut [u8], offset: usize, value: u64) {
        bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
    }

    fn valid_ram() -> Vec<u8> {
        let mut ram = vec![0; 0x10000];
        let result = &mut ram[OFFSET..OFFSET + 128];
        write_u64(result, 0, MAGIC);
        write_u32(result, 8, 1);
        write_u32(result, 12, POST_EXIT);
        write_u64(result, 24, REQUIRED_ARCH);
        write_u64(result, 32, 1);
        write_u64(result, 40, board::RAM_BASE + 0x1000);
        write_u64(result, 48, board::RAM_BASE + 0x2000);
        write_u64(result, 56, board::RAM_BASE + 0x4000);
        write_u64(result, 64, 0x1000);
        write_u64(result, 72, 48);
        write_u64(result, 80, 7);
        write_u64(result, 88, 48);
        write_u32(result, 96, 1);
        write_u32(result, 100, 1);
        write_u64(result, 104, board::RAM_BASE + 0x6000);
        write_u64(result, 112, board::RAM_BASE + 0x7000);
        ram
    }

    #[test]
    fn accepts_complete_post_exit_evidence() {
        let result = validate(&valid_ram()).expect("valid result");
        assert_eq!(result.stage, POST_EXIT);
        assert_eq!(result.arch, REQUIRED_ARCH);
        assert_eq!(result.file_systems, 1);
        assert_eq!(result.image_base, board::RAM_BASE + 0x4000);
        assert_eq!(result.image_size, 0x1000);
        assert_eq!(result.memory_map_size, 48);
        assert_eq!(result.descriptor_size, 48);
        assert_eq!(result.descriptor_version, 1);
        assert_eq!(result.exit_attempts, 1);
    }

    #[test]
    fn rejects_a_pre_exit_stage() {
        let mut ram = valid_ram();
        write_u32(&mut ram[OFFSET..], 12, POST_EXIT - 1);
        assert!(validate(&ram).unwrap_err().contains("header is invalid"));
    }
}
