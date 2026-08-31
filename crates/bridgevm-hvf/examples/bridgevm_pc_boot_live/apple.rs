use bridgevm_hvf::machine::bridgevm_pc as board;
use bridgevm_hvf::platform_pc::BridgeVmPcPlatform;
use sha2::{Digest, Sha256};
use std::ffi::c_void;
use std::path::PathBuf;
use std::ptr::null_mut;

#[path = "gic.rs"]
mod gic;
#[path = "hvf.rs"]
mod hvf;
#[path = "memory.rs"]
mod memory;
#[path = "result.rs"]
mod result;
#[path = "run_loop.rs"]
mod run_loop;
#[path = "vars_file.rs"]
mod vars_file;
use hvf::*;
use memory::{AlignedMemory, GuestRam};

const EXPECTED_FIRMWARE: &str = "9bf4152f31bf304a384341ee8f9fce7f9d2fc890b9302a19935e107596575849";
const EXPECTED_MEDIA: &str = "a49be97db44c0d68b3382f3b1e46eba2fc7a3b12bcba14c1ec720f0511b71979";
const RAM_SIZE: usize = 512 << 20;
const VARS_SIZE: usize = 0x1_0000;

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn arguments() -> Result<(PathBuf, PathBuf, PathBuf), String> {
    let mut args = std::env::args_os().skip(1).map(PathBuf::from);
    let firmware = args.next().ok_or_else(|| {
        "usage: bridgevm_pc_boot_live FIRMWARE_FD BOOT_MEDIA VARS_FILE".to_string()
    })?;
    let media = args.next().ok_or_else(|| {
        "usage: bridgevm_pc_boot_live FIRMWARE_FD BOOT_MEDIA VARS_FILE".to_string()
    })?;
    let vars = args.next().ok_or_else(|| {
        "usage: bridgevm_pc_boot_live FIRMWARE_FD BOOT_MEDIA VARS_FILE".to_string()
    })?;
    if args.next().is_some() {
        return Err("bridgevm_pc_boot_live received extra arguments".to_string());
    }
    Ok((firmware, media, vars))
}

unsafe fn map(memory: &AlignedMemory, address: u64, flags: u64) -> Result<(), String> {
    status(
        "map guest region",
        hv_vm_map(
            memory.pointer.as_ptr().cast::<c_void>(),
            address,
            memory.layout.size(),
            flags,
        ),
    )
}

unsafe fn execute(
    firmware: &mut AlignedMemory,
    variables: &mut AlignedMemory,
    boot_info: &mut AlignedMemory,
    ram: &mut AlignedMemory,
    media: Vec<u8>,
) -> Result<(result::BootResult, usize, usize, String), String> {
    map(
        firmware,
        board::FLASH_CODE.base,
        HV_MEMORY_READ | HV_MEMORY_EXEC,
    )?;
    map(
        variables,
        board::FLASH_VARS.base,
        HV_MEMORY_READ | HV_MEMORY_WRITE,
    )?;
    map(boot_info, board::BOOT_INFO.base, HV_MEMORY_READ)?;
    map(
        ram,
        board::RAM_BASE,
        HV_MEMORY_READ | HV_MEMORY_WRITE | HV_MEMORY_EXEC,
    )?;
    let geometry = gic::create()?;
    let mut vcpu = 0;
    let mut exit = null_mut();
    status(
        "create vCPU",
        hv_vcpu_create(&mut vcpu, &mut exit, null_mut()),
    )?;
    let mut platform = BridgeVmPcPlatform::new();
    platform.load_nvme_disk_image(media);
    let run = (|| {
        status(
            "set MPIDR_EL1",
            hv_vcpu_set_sys_reg(vcpu, HV_SYS_REG_MPIDR_EL1, 0x8000_0000),
        )?;
        status("set PC", hv_vcpu_set_reg(vcpu, HV_REG_PC, 0))?;
        status("set CPSR", hv_vcpu_set_reg(vcpu, HV_REG_CPSR, 0x3c5))?;
        status("unmask VTimer", hv_vcpu_set_vtimer_mask(vcpu, false))?;
        let mut guest_ram = GuestRam::new(ram);
        let (mmio, vtimer) = run_loop::run(vcpu, exit, &mut platform, &mut guest_ram)?;
        let boot_result = result::validate(guest_ram.bytes())?;
        let serial = String::from_utf8_lossy(platform.uart_output()).into_owned();
        Ok((boot_result, mmio, vtimer, serial))
    })();
    let destroy = hv_vcpu_destroy(vcpu);
    run.and_then(|value| {
        status("destroy vCPU", destroy)?;
        println!("gic_geometry={geometry:?}");
        Ok(value)
    })
}

unsafe fn run_unsafe() -> Result<(), String> {
    let (firmware_path, media_path, vars_path) = arguments()?;
    let firmware_bytes = std::fs::read(&firmware_path)
        .map_err(|error| format!("read firmware {}: {error}", firmware_path.display()))?;
    let media_bytes = std::fs::read(&media_path)
        .map_err(|error| format!("read boot media {}: {error}", media_path.display()))?;
    let firmware_hash = sha256(&firmware_bytes);
    let media_hash = sha256(&media_bytes);
    if firmware_bytes.len() != board::FLASH_CODE.size as usize || firmware_hash != EXPECTED_FIRMWARE
    {
        return Err(format!(
            "unexpected boot firmware size/hash: {} {firmware_hash}",
            firmware_bytes.len()
        ));
    }
    if media_hash != EXPECTED_MEDIA {
        return Err(format!("unexpected boot-media hash: {media_hash}"));
    }
    let (mut vars_file, vars_bytes) = vars_file::open(&vars_path)?;
    if vars_bytes.len() != VARS_SIZE {
        return Err(format!("unexpected vars size: {}", vars_bytes.len()));
    }
    let bundle = BridgeVmPcPlatform::build_firmware_tables(1, RAM_SIZE as u64)?;
    let mut firmware = AlignedMemory::new(firmware_bytes.len())?;
    let mut variables = AlignedMemory::new(VARS_SIZE)?;
    let mut boot_info = AlignedMemory::new(board::BOOT_INFO.size as usize)?;
    let mut ram = AlignedMemory::new(RAM_SIZE)?;
    firmware.bytes_mut().copy_from_slice(&firmware_bytes);
    variables.bytes_mut().copy_from_slice(&vars_bytes);
    boot_info
        .bytes_mut()
        .copy_from_slice(&bundle.boot_info.bytes);
    let config = hv_vm_config_create();
    if config.is_null() {
        return Err("hv_vm_config_create returned null".to_string());
    }
    let mut max_ipa = 0;
    status("query max IPA", hv_vm_config_get_max_ipa_size(&mut max_ipa))?;
    status("set max IPA", hv_vm_config_set_ipa_size(config, max_ipa))?;
    status("create VM", hv_vm_create(config))?;
    let run = execute(
        &mut firmware,
        &mut variables,
        &mut boot_info,
        &mut ram,
        media_bytes,
    );
    let destroy = hv_vm_destroy();
    let (boot, mmio, vtimer, serial) = run?;
    status("destroy VM", destroy)?;
    vars_file::persist(&mut vars_file, variables.bytes())?;
    let vars_hash = sha256(variables.bytes());
    println!("BridgeVM Virtual ARM PC BDS/ESP/PE/ExitBootServices probe: PASS");
    println!("board={} abi={}", board::BOARD_ID, board::BOARD_ABI_VERSION);
    println!(
        "firmware_sha256={firmware_hash} boot_media_sha256={media_hash} vars_sha256={vars_hash}"
    );
    println!(
        "stage={} arch={:#x} filesystems={} image={:#x}+{:#x}",
        boot.stage, boot.arch, boot.file_systems, boot.image_base, boot.image_size
    );
    println!(
        "memory_map={} descriptor={}/{} exit_boot_services_attempts={} mmio_exits={} vtimer_exits={}",
        boot.memory_map_size,
        boot.descriptor_size,
        boot.descriptor_version,
        boot.exit_attempts,
        mmio,
        vtimer
    );
    if !serial.is_empty() {
        println!("serial={serial:?}");
    }
    println!("LIVE PROOF: DXE Core invoked BDS, which loaded BOOTAA64.EFI from NVMe and reached code after ExitBootServices");
    Ok(())
}

pub(super) fn run() -> Result<(), String> {
    unsafe { run_unsafe() }
}
