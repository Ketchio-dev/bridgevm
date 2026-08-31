use bridgevm_hvf::machine::bridgevm_pc as board;

#[path = "result_wire.rs"]
mod wire;
pub(super) use wire::BootResult;

#[cfg(test)]
const OFFSET: usize = 0x3000;
#[cfg(test)]
const MAGIC: u64 = 0x544f_4f42_5043_4d42;
const REQUIRED_ARCH: u64 = 0x0fff;
const READY_TO_BOOT: u32 = 7;
const POST_EXIT: u32 = 11;

pub(super) fn inspect(ram: &[u8]) -> Result<BootResult, String> {
    wire::read(ram)
}

fn valid_loaded_image(result: &BootResult, ram_len: usize) -> bool {
    let ram_end = board::RAM_BASE + ram_len as u64;
    [result.image_base, result.system_table, result.boot_services]
        .into_iter()
        .all(|value| (board::RAM_BASE..ram_end).contains(&value))
        && result.image_size != 0
        && result.image_base + result.image_size <= ram_end
}

pub(super) fn validate(ram: &[u8]) -> Result<BootResult, String> {
    let result = inspect(ram)?;
    if result.stage != POST_EXIT || result.status != 0 {
        return Err(format!(
            "boot result header is invalid: stage={:#x} status={:#x} arch={:#x}",
            result.stage, result.status, result.arch
        ));
    }
    if result.arch != REQUIRED_ARCH
        || result.file_systems == 0
        || result.file_system_handle == 0
        || result.image_handle == 0
    {
        return Err(format!(
            "BDS evidence is incomplete: arch={:#x} fs={} fs_handle={:#x} image_handle={:#x}",
            result.arch, result.file_systems, result.file_system_handle, result.image_handle
        ));
    }
    if !valid_loaded_image(&result, ram.len()) {
        return Err("loaded-image or UEFI table pointers are outside RAM".to_string());
    }
    if result.memory_map_size == 0
        || result.map_key == 0
        || result.descriptor_size < 40
        || result.descriptor_version != 1
    {
        return Err(format!(
            "memory-map evidence is invalid: size={} key={} descriptor={}/{}",
            result.memory_map_size,
            result.map_key,
            result.descriptor_size,
            result.descriptor_version
        ));
    }
    if !(1..=3).contains(&result.exit_attempts) {
        return Err(format!(
            "ExitBootServices attempts is {}",
            result.exit_attempts
        ));
    }
    Ok(result)
}

pub(super) fn validate_windows_start(ram: &[u8]) -> Result<BootResult, String> {
    let result = inspect(ram)?;
    if result.stage != READY_TO_BOOT
        || result.status != 0
        || result.arch != REQUIRED_ARCH
        || result.file_systems == 0
        || result.file_system_handle == 0
        || result.image_handle == 0
        || !valid_loaded_image(&result, ram.len())
    {
        return Err(format!(
            "Windows Boot Manager handoff is incomplete: stage={:#x} status={:#x} arch={:#x} fs={} image={:#x}+{:#x}",
            result.stage,
            result.status,
            result.arch,
            result.file_systems,
            result.image_base,
            result.image_size
        ));
    }
    Ok(result)
}

#[cfg(test)]
#[path = "result_tests.rs"]
mod tests;
