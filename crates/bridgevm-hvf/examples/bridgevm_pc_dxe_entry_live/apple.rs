use super::contract::{
    self, result_gpa, validate_firmware, validate_sec_result, validate_variable_store, SecResult,
    VariableState,
};
use bridgevm_hvf::machine::bridgevm_pc as board;
use bridgevm_hvf::platform_pc::BridgeVmPcPlatform;
use sha2::{Digest, Sha256};
use std::ffi::c_void;
use std::ptr::null_mut;

#[path = "apple/aligned_memory.rs"]
mod aligned_memory;
#[path = "apple/command.rs"]
mod command;
#[path = "apple/in_memory.rs"]
mod in_memory;
#[path = "apple/mmio.rs"]
mod mmio;
#[path = "apple/process.rs"]
mod process;
#[path = "apple/vars_file.rs"]
mod vars_file;
use aligned_memory::AlignedMemory;

#[path = "../bridgevm_pc_reset_vector_live/hvc_diagnostics.rs"]
mod hvc_diagnostics;
type HvReturn = i32;
type HvVcpu = u64;
const HV_SUCCESS: HvReturn = 0;
const HV_REG_PC: u32 = 31;
const HV_REG_CPSR: u32 = 34;
const HV_MEMORY_READ: u64 = 1;
const HV_MEMORY_WRITE: u64 = 2;
const HV_MEMORY_EXEC: u64 = 4;
const EXIT_EXCEPTION: u32 = 1;

#[repr(C)]
struct HvVcpuExitException {
    syndrome: u64,
    virtual_address: u64,
    physical_address: u64,
}

#[repr(C)]
struct HvVcpuExit {
    reason: u32,
    exception: HvVcpuExitException,
}

#[link(name = "Hypervisor", kind = "framework")]
unsafe extern "C" {
    fn hv_vm_config_create() -> *mut c_void;
    fn hv_vm_config_get_max_ipa_size(bits: *mut u32) -> HvReturn;
    fn hv_vm_config_set_ipa_size(config: *mut c_void, bits: u32) -> HvReturn;
    fn hv_vm_create(config: *mut c_void) -> HvReturn;
    fn hv_vm_destroy() -> HvReturn;
    fn hv_vm_map(addr: *mut c_void, ipa: u64, size: usize, flags: u64) -> HvReturn;
    fn hv_vm_unmap(ipa: u64, size: usize) -> HvReturn;
    fn hv_vcpu_create(
        vcpu: *mut HvVcpu,
        exit: *mut *mut HvVcpuExit,
        config: *mut c_void,
    ) -> HvReturn;
    fn hv_vcpu_destroy(vcpu: HvVcpu) -> HvReturn;
    fn hv_vcpu_run(vcpu: HvVcpu) -> HvReturn;
    fn hv_vcpus_exit(vcpus: *const HvVcpu, vcpu_count: u32) -> HvReturn;
    fn hv_vcpu_get_reg(vcpu: HvVcpu, reg: u32, value: *mut u64) -> HvReturn;
    fn hv_vcpu_set_reg(vcpu: HvVcpu, reg: u32, value: u64) -> HvReturn;
}
fn status(label: &str, value: HvReturn) -> Result<(), String> {
    (value == HV_SUCCESS)
        .then_some(())
        .ok_or_else(|| format!("{label} failed: {value:#x}"))
}

unsafe fn execute_reset_vector(
    firmware: &mut AlignedMemory,
    variables: &mut AlignedMemory,
    boot_info: &mut AlignedMemory,
    ram: &mut AlignedMemory,
    expected_variable_state: VariableState,
) -> Result<SecResult, String> {
    status(
        "map flash code",
        hv_vm_map(
            firmware.pointer.as_ptr().cast(),
            board::FLASH_CODE.base,
            firmware.layout.size(),
            HV_MEMORY_READ | HV_MEMORY_EXEC,
        ),
    )?;
    let firmware_result = (|| {
        status(
            "map vars backing",
            hv_vm_map(
                variables.pointer.as_ptr().cast(),
                board::FLASH_VARS.base,
                variables.layout.size(),
                HV_MEMORY_READ | HV_MEMORY_WRITE,
            ),
        )?;
        let variables_result = (|| {
            status(
                "map boot-info",
                hv_vm_map(
                    boot_info.pointer.as_ptr().cast(),
                    board::BOOT_INFO.base,
                    boot_info.layout.size(),
                    HV_MEMORY_READ,
                ),
            )?;
            let boot_info_result = (|| {
                status(
                    "map probe RAM",
                    hv_vm_map(
                        ram.pointer.as_ptr().cast(),
                        board::RAM_BASE,
                        ram.layout.size(),
                        HV_MEMORY_READ
                            | HV_MEMORY_WRITE
                            | (contract::RAM_EXECUTABLE as u64 * HV_MEMORY_EXEC),
                    ),
                )?;
                let ram_result = (|| {
                    let mut vcpu = 0;
                    let mut exit = null_mut();
                    status(
                        "create vCPU",
                        hv_vcpu_create(&mut vcpu, &mut exit, null_mut()),
                    )?;
                    let mut platform = BridgeVmPcPlatform::new();
                    let run_result = (|| {
                        status("set PC", hv_vcpu_set_reg(vcpu, HV_REG_PC, 0))?;
                        status("set CPSR", hv_vcpu_set_reg(vcpu, HV_REG_CPSR, 0x3c5))?;
                        mmio::run_vcpu(vcpu, exit, &mut platform)
                    })();
                    let destroy = hv_vcpu_destroy(vcpu);
                    run_result?;
                    status("destroy vCPU", destroy)
                })();
                let unmap = hv_vm_unmap(board::RAM_BASE, ram.layout.size());
                ram_result?;
                status("unmap probe RAM", unmap)
            })();
            let unmap = hv_vm_unmap(board::BOOT_INFO.base, boot_info.layout.size());
            boot_info_result?;
            status("unmap boot-info", unmap)
        })();
        let unmap = hv_vm_unmap(board::FLASH_VARS.base, variables.layout.size());
        variables_result?;
        status("unmap vars backing", unmap)
    })();
    let unmap = hv_vm_unmap(board::FLASH_CODE.base, firmware.layout.size());
    firmware_result?;
    status("unmap flash code", unmap)?;
    validate_sec_result(
        std::slice::from_raw_parts(ram.pointer.as_ptr(), ram.layout.size()),
        expected_variable_state,
    )
}

unsafe fn run_fresh_vm(
    firmware: &mut AlignedMemory,
    variables: &mut AlignedMemory,
    boot_info: &mut AlignedMemory,
    ram: &mut AlignedMemory,
    expected_state: VariableState,
) -> Result<SecResult, String> {
    let config = hv_vm_config_create();
    if config.is_null() {
        return Err("hv_vm_config_create returned null".to_string());
    }
    let mut max_ipa = 0;
    status("max IPA query", hv_vm_config_get_max_ipa_size(&mut max_ipa))?;
    status("set max IPA", hv_vm_config_set_ipa_size(config, max_ipa))?;
    status("create VM", hv_vm_create(config))?;
    let run_result = execute_reset_vector(firmware, variables, boot_info, ram, expected_state);
    let destroy = hv_vm_destroy();
    let result = run_result?;
    status("destroy VM", destroy)?;
    Ok(result)
}

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub fn run() -> Result<(), String> {
    let arguments = command::parse()?;
    let firmware = std::fs::read(&arguments.firmware).map_err(|error| {
        format!(
            "read reset-vector FD {}: {error}",
            arguments.firmware.display()
        )
    })?;
    let digest = validate_firmware(&firmware)?;
    match arguments.mode {
        command::RunMode::InMemory => unsafe { in_memory::run(&firmware, &digest) },
        command::RunMode::VarsFile { path, expectation } => unsafe {
            process::run(&firmware, &digest, &path, expectation)
        },
    }
}
