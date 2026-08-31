use bridgevm_hvf::machine::bridgevm_pc as board;
use bridgevm_hvf::platform_pc::BridgeVmPcPlatform;
use sha2::{Digest, Sha256};
use std::ffi::c_void;
use std::ptr::null_mut;

#[path = "arguments.rs"]
mod arguments;
#[path = "boot_media.rs"]
mod boot_media;
#[path = "gic.rs"]
mod gic;
#[path = "hvf.rs"]
mod hvf;
#[path = "memory.rs"]
mod memory;
#[path = "report.rs"]
mod report;
#[path = "result.rs"]
mod result;
#[path = "run_loop.rs"]
mod run_loop;
#[path = "vars_file.rs"]
mod vars_file;
#[path = "vcpu_state.rs"]
mod vcpu_state;
use hvf::*;
use memory::{AlignedMemory, GuestRam};

const EXPECTED_FIRMWARE: &str = "82a6fa3bb8417d49db43d4a2089a48c8c215f8f6fafafc9a3a903ba661b73e64";
const VARS_SIZE: usize = 0x1_0000;

struct Execution {
    run: Result<(usize, usize), String>,
    state: vcpu_state::VcpuState,
    serial: String,
}

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
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
    media: boot_media::BootMedia,
) -> Result<Execution, String> {
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
    media.attach(&mut platform)?;
    let run = (|| {
        status(
            "set MPIDR_EL1",
            hv_vcpu_set_sys_reg(vcpu, HV_SYS_REG_MPIDR_EL1, 0x8000_0000),
        )?;
        status("set PC", hv_vcpu_set_reg(vcpu, HV_REG_PC, 0))?;
        status("set CPSR", hv_vcpu_set_reg(vcpu, HV_REG_CPSR, 0x3c5))?;
        status("unmask VTimer", hv_vcpu_set_vtimer_mask(vcpu, false))?;
        let mut guest_ram = GuestRam::new(ram);
        let run = run_loop::run(vcpu, exit, &mut platform, &mut guest_ram);
        let state = vcpu_state::capture(vcpu, exit)?;
        let serial = String::from_utf8_lossy(platform.uart_output()).into_owned();
        Ok(Execution { run, state, serial })
    })();
    let destroy = hv_vcpu_destroy(vcpu);
    run.and_then(|value| {
        status("destroy vCPU", destroy)?;
        println!("gic_geometry={geometry:?}");
        Ok(value)
    })
}

unsafe fn run_unsafe() -> Result<(), String> {
    let arguments::Arguments {
        firmware: firmware_path,
        media: media_path,
        vars: vars_path,
        windows_raw,
    } = arguments::read()?;
    let firmware_bytes = std::fs::read(&firmware_path)
        .map_err(|error| format!("read firmware {}: {error}", firmware_path.display()))?;
    let media = boot_media::BootMedia::open(&media_path, windows_raw)?;
    let media_identity = media.identity();
    let windows_diagnostic = media.is_windows_diagnostic();
    let ram_size = media_identity.ram_bytes as usize;
    let firmware_hash = sha256(&firmware_bytes);
    if firmware_bytes.len() != board::FLASH_CODE.size as usize || firmware_hash != EXPECTED_FIRMWARE
    {
        return Err(format!(
            "unexpected boot firmware size/hash: {} {firmware_hash}",
            firmware_bytes.len()
        ));
    }
    let (mut vars_file, vars_bytes) = vars_file::open(&vars_path)?;
    if vars_bytes.len() != VARS_SIZE {
        return Err(format!("unexpected vars size: {}", vars_bytes.len()));
    }
    let bundle = BridgeVmPcPlatform::build_firmware_tables(1, ram_size as u64)?;
    let mut firmware = AlignedMemory::new(firmware_bytes.len())?;
    let mut variables = AlignedMemory::new(VARS_SIZE)?;
    let mut boot_info = AlignedMemory::new(board::BOOT_INFO.size as usize)?;
    let mut ram = AlignedMemory::new(ram_size)?;
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
        media,
    );
    let destroy = hv_vm_destroy();
    let execution = run?;
    status("destroy VM", destroy)?;
    vars_file::persist(&mut vars_file, variables.bytes())?;
    let vars_hash = sha256(variables.bytes());
    let hashes = (firmware_hash.as_str(), vars_hash.as_str());
    if windows_diagnostic {
        return report::windows(hashes, media_identity, ram.bytes(), execution);
    }
    let boot = result::validate(ram.bytes())?;
    report::proof::write(hashes, media_identity, boot, execution)
}

pub(super) fn run() -> Result<(), String> {
    unsafe { run_unsafe() }
}
