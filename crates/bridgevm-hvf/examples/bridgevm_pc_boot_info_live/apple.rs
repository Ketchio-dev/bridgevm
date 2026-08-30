use super::guest::{
    result_gpa, BOOT_MAGIC, CODE, MEMORY_SIZE, PASS_RESULT, RESULT_OFFSET, RSDP_MAGIC, XSDT_MAGIC,
};
use bridgevm_hvf::bridgevm_pc_boot_info::BOOT_INFO_HEADER_SIZE;
use bridgevm_hvf::machine::bridgevm_pc as board;
use bridgevm_hvf::platform_pc::BridgeVmPcPlatform;
use std::alloc::{alloc_zeroed, dealloc, Layout};
use std::ffi::c_void;
use std::ptr::{null_mut, NonNull};
use std::sync::mpsc;
use std::time::Duration;

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
        let layout = Layout::from_size_align(size, MEMORY_SIZE)
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
        .map_err(|_| "boot-info probe watchdog panicked".to_string())?;
    status("run vCPU", run)?;
    if (*exit).reason != EXIT_EXCEPTION {
        return Err(format!("unexpected vCPU exit reason {}", (*exit).reason));
    }
    let esr = (*exit).exception.syndrome;
    let ec = (esr >> 26) & 0x3f;
    if ec != EC_HVC {
        return Err(format!(
            "unexpected guest exception EC={ec:#x} ESR={esr:#x}"
        ));
    }
    Ok(())
}

unsafe fn run_guest(ram: &mut AlignedMemory, boot_info: &mut AlignedMemory) -> Result<u32, String> {
    for (index, word) in CODE.iter().enumerate() {
        ram.bytes_mut()[index * 4..index * 4 + 4].copy_from_slice(&word.to_le_bytes());
    }
    status(
        "map probe RAM",
        hv_vm_map(
            ram.pointer.as_ptr().cast(),
            board::RAM_BASE,
            MEMORY_SIZE,
            HV_MEMORY_READ | HV_MEMORY_WRITE | HV_MEMORY_EXEC,
        ),
    )?;
    let ram_result = (|| {
        status(
            "map boot-info",
            hv_vm_map(
                boot_info.pointer.as_ptr().cast(),
                board::BOOT_INFO.base,
                MEMORY_SIZE,
                HV_MEMORY_READ,
            ),
        )?;
        let boot_result = (|| {
            let mut vcpu = 0;
            let mut exit = null_mut();
            status(
                "create vCPU",
                hv_vcpu_create(&mut vcpu, &mut exit, null_mut()),
            )?;
            let vcpu_result = (|| {
                status("set PC", hv_vcpu_set_reg(vcpu, HV_REG_PC, board::RAM_BASE))?;
                status("set CPSR", hv_vcpu_set_reg(vcpu, HV_REG_CPSR, 0x3c5))?;
                status(
                    "set boot-info GPA",
                    hv_vcpu_set_reg(vcpu, 0, board::BOOT_INFO.base),
                )?;
                status("set result GPA", hv_vcpu_set_reg(vcpu, 1, result_gpa()?))?;
                status("set boot magic", hv_vcpu_set_reg(vcpu, 8, BOOT_MAGIC))?;
                status("set RSDP magic", hv_vcpu_set_reg(vcpu, 9, RSDP_MAGIC))?;
                status("set XSDT magic", hv_vcpu_set_reg(vcpu, 10, XSDT_MAGIC))?;
                run_vcpu(vcpu, exit)
            })();
            let destroy = hv_vcpu_destroy(vcpu);
            vcpu_result?;
            status("destroy vCPU", destroy)
        })();
        let unmap = hv_vm_unmap(board::BOOT_INFO.base, MEMORY_SIZE);
        boot_result?;
        status("unmap boot-info", unmap)
    })();
    let unmap = hv_vm_unmap(board::RAM_BASE, MEMORY_SIZE);
    ram_result?;
    status("unmap probe RAM", unmap)?;
    Ok(std::ptr::read_volatile(
        ram.pointer.as_ptr().add(RESULT_OFFSET).cast::<u32>(),
    ))
}

unsafe fn run_unsafe() -> Result<(), String> {
    let bundle = BridgeVmPcPlatform::build_firmware_tables(1, 512 << 20)?;
    let mut ram = AlignedMemory::new(MEMORY_SIZE)?;
    let mut boot_info = AlignedMemory::new(MEMORY_SIZE)?;
    boot_info
        .bytes_mut()
        .copy_from_slice(&bundle.boot_info.bytes);

    let config = hv_vm_config_create();
    if config.is_null() {
        return Err("hv_vm_config_create returned null".to_string());
    }
    let mut max_ipa = 0;
    status("max IPA query", hv_vm_config_get_max_ipa_size(&mut max_ipa))?;
    status("set max IPA", hv_vm_config_set_ipa_size(config, max_ipa))?;
    status("create VM", hv_vm_create(config))?;
    let result = run_guest(&mut ram, &mut boot_info);
    let destroy = hv_vm_destroy();
    let guest_result = result?;
    status("destroy VM", destroy)?;
    if guest_result != PASS_RESULT {
        return Err(format!(
            "guest boot-info verification failed at stage {guest_result}"
        ));
    }
    let checksum = bundle.boot_info.bytes[..BOOT_INFO_HEADER_SIZE]
        .iter()
        .fold(0u8, |sum, byte| sum.wrapping_add(*byte));
    println!("BridgeVM Virtual ARM PC boot-info probe: PASS");
    println!("board={} abi={}", board::BOARD_ID, board::BOARD_ABI_VERSION);
    println!(
        "boot_info={:#x} size={:#x} ram={:#x}",
        board::BOOT_INFO.base,
        board::BOOT_INFO.size,
        board::RAM_BASE
    );
    println!(
        "guest_result={guest_result} header_checksum={checksum} rsdp={:#x} xsdt={:#x}",
        bundle.boot_info.rsdp_gpa, bundle.boot_info.acpi_tables_gpa
    );
    println!("LIVE PROOF: EL1 read BridgeVM boot-info v1 and followed its RSDP to XSDT");
    Ok(())
}

pub fn run() -> Result<(), String> {
    unsafe { run_unsafe() }
}
