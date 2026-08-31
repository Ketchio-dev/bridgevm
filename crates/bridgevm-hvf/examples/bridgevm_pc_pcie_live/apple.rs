use super::contract::{self, ENDPOINTS};
use bridgevm_hvf::machine::bridgevm_pc as board;
use bridgevm_hvf::platform_pc::BridgeVmPcPlatform;
use bridgevm_hvf::platform_virt::{MmioOp, MmioOutcome};
use std::alloc::{alloc_zeroed, dealloc, Layout};
use std::ffi::c_void;
use std::ptr::{null_mut, NonNull};
use std::sync::mpsc;
use std::time::Duration;

type HvReturn = i32;
type HvVcpu = u64;

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

const HV_SUCCESS: HvReturn = 0;
const HV_REG_PC: u32 = 31;
const HV_REG_CPSR: u32 = 34;
const HV_MEMORY_READ: u64 = 1;
const HV_MEMORY_WRITE: u64 = 2;
const HV_MEMORY_EXEC: u64 = 4;
const EXIT_EXCEPTION: u32 = 1;
const EC_DATA_ABORT: u64 = 0x24;
const EC_HVC: u64 = 0x16;
const PAGE_SIZE: usize = 0x1_0000;
const RESULT_REGS: [u32; ENDPOINTS.len()] = [0, 9, 10, 11, 12, 13, 14, 15];

const fn ldr_w(result: u32, address: u32) -> u32 {
    0xb940_0000 | (address << 5) | result
}

const GUEST_CODE: [u32; ENDPOINTS.len() + 1] = [
    ldr_w(0, 1),
    ldr_w(9, 2),
    ldr_w(10, 3),
    ldr_w(11, 4),
    ldr_w(12, 5),
    ldr_w(13, 6),
    ldr_w(14, 7),
    ldr_w(15, 8),
    0xd400_0002,
];

struct GuestPage {
    pointer: NonNull<u8>,
    layout: Layout,
}

impl GuestPage {
    fn new() -> Result<Self, String> {
        let layout = Layout::from_size_align(PAGE_SIZE, PAGE_SIZE)
            .map_err(|error| format!("guest page layout: {error}"))?;
        let pointer = NonNull::new(unsafe { alloc_zeroed(layout) })
            .ok_or_else(|| "guest page allocation failed".to_string())?;
        for (index, instruction) in GUEST_CODE.iter().enumerate() {
            unsafe {
                std::ptr::copy_nonoverlapping(
                    instruction.to_le_bytes().as_ptr(),
                    pointer.as_ptr().add(index * 4),
                    4,
                )
            };
        }
        Ok(Self { pointer, layout })
    }
}

impl Drop for GuestPage {
    fn drop(&mut self) {
        unsafe { dealloc(self.pointer.as_ptr(), self.layout) }
    }
}

fn status(label: &str, value: HvReturn) -> Result<(), String> {
    (value == HV_SUCCESS)
        .then_some(())
        .ok_or_else(|| format!("{label} failed: {value:#x}"))
}

unsafe fn execute(vcpu: HvVcpu, exit: *mut HvVcpuExit) -> Result<[u64; 8], String> {
    let mut platform = BridgeVmPcPlatform::new();
    let mut reads = 0usize;
    let (stop_tx, stop_rx) = mpsc::channel();
    let watchdog = std::thread::spawn(move || {
        if stop_rx.recv_timeout(Duration::from_secs(2)).is_err() {
            let _ = hv_vcpus_exit(&vcpu, 1);
        }
    });
    let run_result = (|| {
        loop {
            status("run vCPU", hv_vcpu_run(vcpu))?;
            if (*exit).reason != EXIT_EXCEPTION {
                return Err(format!("unexpected vCPU exit reason {}", (*exit).reason));
            }
            let esr = (*exit).exception.syndrome;
            match (esr >> 26) & 0x3f {
                EC_DATA_ABORT => {
                    if reads >= ENDPOINTS.len() || (esr >> 24) & 1 == 0 {
                        return Err(format!("unexpected PCIe data abort ESR={esr:#x}"));
                    }
                    let size = 1u8 << ((esr >> 22) & 0x3);
                    let register = ((esr >> 16) & 0x1f) as u32;
                    let ipa = (*exit).exception.physical_address;
                    let expected = ENDPOINTS[reads];
                    if size != 4
                        || (esr >> 6) & 1 != 0
                        || register != RESULT_REGS[reads]
                        || ipa != contract::ecam_gpa(expected.bdf)
                    {
                        return Err(format!(
                            "PCIe access {reads} is outside its fixed contract: ESR={esr:#x} IPA={ipa:#x}"
                        ));
                    }
                    let value = match platform.on_mmio(ipa, MmioOp::Read { size }) {
                        MmioOutcome::ReadValue(value) => value,
                        outcome => return Err(format!("PCIe access was not handled: {outcome:?}")),
                    };
                    status("set MMIO result", hv_vcpu_set_reg(vcpu, register, value))?;
                    let mut pc = 0;
                    status("get PC", hv_vcpu_get_reg(vcpu, HV_REG_PC, &mut pc))?;
                    status("advance PC", hv_vcpu_set_reg(vcpu, HV_REG_PC, pc + 4))?;
                    reads += 1;
                }
                EC_HVC if reads == ENDPOINTS.len() => break,
                ec => return Err(format!("unexpected guest EC={ec:#x} ESR={esr:#x}")),
            }
        }
        let mut observed = [0; ENDPOINTS.len()];
        for (value, register) in observed.iter_mut().zip(RESULT_REGS) {
            status("read guest result", hv_vcpu_get_reg(vcpu, register, value))?;
        }
        contract::validate(&observed)?;
        Ok(observed)
    })();
    let _ = stop_tx.send(());
    watchdog
        .join()
        .map_err(|_| "PCIe probe watchdog panicked".to_string())?;
    run_result
}

unsafe fn run_unsafe() -> Result<(), String> {
    let page = GuestPage::new()?;
    let config = hv_vm_config_create();
    if config.is_null() {
        return Err("hv_vm_config_create returned null".to_string());
    }
    let mut max_ipa = 0;
    status("max IPA query", hv_vm_config_get_max_ipa_size(&mut max_ipa))?;
    status("set max IPA", hv_vm_config_set_ipa_size(config, max_ipa))?;
    status("create VM", hv_vm_create(config))?;
    status(
        "map guest page",
        hv_vm_map(
            page.pointer.as_ptr().cast(),
            board::RAM_BASE,
            page.layout.size(),
            HV_MEMORY_READ | HV_MEMORY_WRITE | HV_MEMORY_EXEC,
        ),
    )?;
    let mut vcpu = 0;
    let mut exit = null_mut();
    status(
        "create vCPU",
        hv_vcpu_create(&mut vcpu, &mut exit, null_mut()),
    )?;
    status("set PC", hv_vcpu_set_reg(vcpu, HV_REG_PC, board::RAM_BASE))?;
    status("set CPSR", hv_vcpu_set_reg(vcpu, HV_REG_CPSR, 0x3c5))?;
    for (register, endpoint) in (1u32..=8).zip(ENDPOINTS) {
        status(
            "set ECAM address",
            hv_vcpu_set_reg(vcpu, register, contract::ecam_gpa(endpoint.bdf)),
        )?;
    }
    let run_result = execute(vcpu, exit);
    let destroy_vcpu = hv_vcpu_destroy(vcpu);
    let unmap = hv_vm_unmap(board::RAM_BASE, page.layout.size());
    let destroy_vm = hv_vm_destroy();
    let observed = run_result?;
    status("destroy vCPU", destroy_vcpu)?;
    status("unmap guest page", unmap)?;
    status("destroy VM", destroy_vm)?;

    println!("BridgeVM Virtual ARM PC PCIe enumeration probe: PASS");
    println!("board={} abi={}", board::BOARD_ID, board::BOARD_ABI_VERSION);
    println!(
        "ecam_base={:#x} ecam_size={:#x} reads={}",
        board::PCIE_ECAM.base,
        board::PCIE_ECAM.size,
        observed.len()
    );
    for (endpoint, value) in ENDPOINTS.iter().zip(observed) {
        println!(
            "role={} bdf={:02x}:{:02x}.{} identity={value:#010x}",
            endpoint.role, endpoint.bdf.0, endpoint.bdf.1, endpoint.bdf.2
        );
    }
    println!("LIVE PROOF: guest MMIO enumerated all BridgeVM PC v1 PCIe identities");
    Ok(())
}

pub fn run() -> Result<(), String> {
    unsafe { run_unsafe() }
}
