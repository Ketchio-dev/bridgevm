use super::contract::{
    self, result_gpa, validate_firmware, validate_sec_result, validate_variable_store, SecResult,
    VariableState,
};
use bridgevm_hvf::machine::bridgevm_pc as board;
use bridgevm_hvf::platform_pc::BridgeVmPcPlatform;
use sha2::{Digest, Sha256};
use std::alloc::{alloc_zeroed, dealloc, Layout};
use std::ffi::c_void;
use std::path::PathBuf;
use std::ptr::{null_mut, NonNull};
use std::sync::mpsc;
use std::time::Duration;

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
const EC_HVC: u64 = 0x16;
const PAGE_ALIGNMENT: usize = 0x1_0000;

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
    fn hv_vcpu_set_reg(vcpu: HvVcpu, reg: u32, value: u64) -> HvReturn;
}
struct AlignedMemory {
    pointer: NonNull<u8>,
    layout: Layout,
}

impl AlignedMemory {
    fn new(size: usize) -> Result<Self, String> {
        let layout = Layout::from_size_align(size, PAGE_ALIGNMENT)
            .map_err(|error| format!("guest allocation layout: {error}"))?;
        let pointer = NonNull::new(unsafe { alloc_zeroed(layout) })
            .ok_or_else(|| "guest allocation failed".to_string())?;
        Ok(Self { pointer, layout })
    }

    fn bytes_mut(&mut self) -> &mut [u8] {
        unsafe { std::slice::from_raw_parts_mut(self.pointer.as_ptr(), self.layout.size()) }
    }
}

impl Drop for AlignedMemory {
    fn drop(&mut self) {
        unsafe { dealloc(self.pointer.as_ptr(), self.layout) }
    }
}
fn status(label: &str, value: HvReturn) -> Result<(), String> {
    (value == HV_SUCCESS)
        .then_some(())
        .ok_or_else(|| format!("{label} failed: {value:#x}"))
}

unsafe fn run_vcpu(vcpu: HvVcpu, exit: *mut HvVcpuExit) -> Result<(), String> {
    let (stop_tx, stop_rx) = mpsc::channel();
    let watchdog = std::thread::spawn(move || {
        if stop_rx.recv_timeout(Duration::from_secs(2)).is_err() {
            let _ = hv_vcpus_exit(&vcpu, 1);
        }
    });
    let run = hv_vcpu_run(vcpu);
    let _ = stop_tx.send(());
    watchdog
        .join()
        .map_err(|_| "reset-vector probe watchdog panicked".to_string())?;
    status("run vCPU", run)?;
    if (*exit).reason != EXIT_EXCEPTION {
        return Err(format!("unexpected vCPU exit reason {}", (*exit).reason));
    }
    let esr = (*exit).exception.syndrome;
    let ec = (esr >> 26) & 0x3f;
    if ec != EC_HVC {
        return Err(format!(
            "unexpected reset-vector exception EC={ec:#x} ESR={esr:#x}"
        ));
    }
    hvc_diagnostics::print_hvc_arguments(vcpu, esr)?;
    Ok(())
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
                    let run_result = (|| {
                        status("set PC", hv_vcpu_set_reg(vcpu, HV_REG_PC, 0))?;
                        status("set CPSR", hv_vcpu_set_reg(vcpu, HV_REG_CPSR, 0x3c5))?;
                        run_vcpu(vcpu, exit)
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

unsafe fn run_unsafe(firmware_bytes: &[u8], firmware_sha256: &str) -> Result<(), String> {
    let bundle = BridgeVmPcPlatform::build_firmware_tables(1, 512 << 20)?;
    let mut firmware = AlignedMemory::new(firmware_bytes.len())?;
    let mut variables = AlignedMemory::new(PAGE_ALIGNMENT)?;
    let mut boot_info = AlignedMemory::new(board::BOOT_INFO.size as usize)?;
    firmware.bytes_mut().copy_from_slice(firmware_bytes);
    variables.bytes_mut().fill(0xff);
    boot_info
        .bytes_mut()
        .copy_from_slice(&bundle.boot_info.bytes);

    let initial_vars_sha256 = sha256(variables.bytes_mut());
    let mut results = Vec::new();
    let mut vars_hashes = Vec::new();
    let mut boot_rams = Vec::new();
    for _ in 0..2 {
        boot_rams.push(AlignedMemory::new(PAGE_ALIGNMENT * contract::RAM_PAGES)?);
    }
    for (expected_state, ram) in [VariableState::Written, VariableState::Restored]
        .into_iter()
        .zip(boot_rams.iter_mut())
    {
        let config = hv_vm_config_create();
        if config.is_null() {
            return Err("hv_vm_config_create returned null".to_string());
        }
        let mut max_ipa = 0;
        status("max IPA query", hv_vm_config_get_max_ipa_size(&mut max_ipa))?;
        status("set max IPA", hv_vm_config_set_ipa_size(config, max_ipa))?;
        status("create VM", hv_vm_create(config))?;
        let run_result = execute_reset_vector(
            &mut firmware,
            &mut variables,
            &mut boot_info,
            ram,
            expected_state,
        );
        let destroy = hv_vm_destroy();
        let result = run_result?;
        status("destroy VM", destroy)?;
        validate_variable_store(variables.bytes_mut())?;
        vars_hashes.push(sha256(variables.bytes_mut()));
        results.push(result);
    }
    if initial_vars_sha256 == vars_hashes[0] || vars_hashes[0] != vars_hashes[1] {
        return Err("vars backing did not change once and then remain stable".to_string());
    }
    let result = results
        .pop()
        .ok_or_else(|| "second variable-service boot is missing".to_string())?;
    println!("{}", contract::PROBE_TITLE);
    println!("board={} abi={}", board::BOARD_ID, board::BOARD_ABI_VERSION);
    println!(
        "reset_vector={:#x} firmware_size={:#x} ram={:#x}",
        board::FLASH_CODE.base,
        board::FLASH_CODE.size,
        board::RAM_BASE
    );
    println!(
        "{result} result_gpa={:#x} boot_info={:#x}",
        result_gpa()?,
        board::BOOT_INFO.base
    );
    println!("firmware_sha256={firmware_sha256}");
    println!("vars_initial_sha256={initial_vars_sha256}");
    println!("vars_written_sha256={}", vars_hashes[0]);
    println!("vars_restored_sha256={}", vars_hashes[1]);
    println!("{}", contract::LIVE_PROOF);
    Ok(())
}

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub fn run() -> Result<(), String> {
    let mut args = std::env::args_os();
    let _program = args.next();
    let path = PathBuf::from(
        args.next()
            .ok_or_else(|| "usage: bridgevm_pc_dxe_entry_live FIRMWARE_FD".to_string())?,
    );
    if args.next().is_some() {
        return Err("usage: bridgevm_pc_dxe_entry_live FIRMWARE_FD".to_string());
    }
    let firmware = std::fs::read(&path)
        .map_err(|error| format!("read reset-vector FD {}: {error}", path.display()))?;
    let digest = validate_firmware(&firmware)?;
    unsafe { run_unsafe(&firmware, &digest) }
}
