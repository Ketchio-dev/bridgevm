use std::ffi::c_void;

pub(super) type HvReturn = i32;
pub(super) type HvVcpu = u64;
pub(super) const HV_REG_PC: u32 = 31;
pub(super) const HV_REG_CPSR: u32 = 34;
pub(super) const HV_SYS_REG_MPIDR_EL1: u16 = 0xc005;
pub(super) const HV_SYS_REG_CNTV_CVAL_EL0: u16 = 0xdf1a;
pub(super) const VTIMER_DEADLINE_TICKS: u64 = 50_000_000;
pub(super) const HV_MEMORY_READ: u64 = 1;
pub(super) const HV_MEMORY_WRITE: u64 = 2;
pub(super) const HV_MEMORY_EXEC: u64 = 4;
pub(super) const EXIT_CANCELED: u32 = 0;
pub(super) const EXIT_EXCEPTION: u32 = 1;
pub(super) const EXIT_VTIMER: u32 = 2;
pub(super) const HV_INTERRUPT_TYPE_IRQ: u32 = 0;

#[repr(C)]
pub(super) struct HvVcpuExitException {
    pub(super) syndrome: u64,
    pub(super) virtual_address: u64,
    pub(super) physical_address: u64,
}

#[repr(C)]
pub(super) struct HvVcpuExit {
    pub(super) reason: u32,
    pub(super) exception: HvVcpuExitException,
}

#[link(name = "Hypervisor", kind = "framework")]
unsafe extern "C" {
    pub(super) fn hv_vm_config_create() -> *mut c_void;
    pub(super) fn hv_vm_config_get_max_ipa_size(bits: *mut u32) -> HvReturn;
    pub(super) fn hv_vm_config_set_ipa_size(config: *mut c_void, bits: u32) -> HvReturn;
    pub(super) fn hv_vm_create(config: *mut c_void) -> HvReturn;
    pub(super) fn hv_vm_destroy() -> HvReturn;
    pub(super) fn hv_vm_map(addr: *mut c_void, ipa: u64, size: usize, flags: u64) -> HvReturn;
    pub(super) fn hv_vm_protect(ipa: u64, size: usize, flags: u64) -> HvReturn;
    pub(super) fn hv_vcpu_create(
        vcpu: *mut HvVcpu,
        exit: *mut *mut HvVcpuExit,
        config: *mut c_void,
    ) -> HvReturn;
    pub(super) fn hv_vcpu_destroy(vcpu: HvVcpu) -> HvReturn;
    pub(super) fn hv_vcpu_run(vcpu: HvVcpu) -> HvReturn;
    pub(super) fn hv_vcpus_exit(vcpus: *const HvVcpu, count: u32) -> HvReturn;
    pub(super) fn hv_vcpu_get_reg(vcpu: HvVcpu, reg: u32, value: *mut u64) -> HvReturn;
    pub(super) fn hv_vcpu_set_reg(vcpu: HvVcpu, reg: u32, value: u64) -> HvReturn;
    pub(super) fn hv_vcpu_set_sys_reg(vcpu: HvVcpu, reg: u16, value: u64) -> HvReturn;
    pub(super) fn hv_vcpu_set_vtimer_mask(vcpu: HvVcpu, masked: bool) -> HvReturn;
    pub(super) fn hv_vcpu_set_pending_interrupt(
        vcpu: HvVcpu,
        interrupt_type: u32,
        pending: bool,
    ) -> HvReturn;
    pub(super) fn mach_absolute_time() -> u64;
}

pub(super) fn status(label: &str, value: HvReturn) -> Result<(), String> {
    (value == 0)
        .then_some(())
        .ok_or_else(|| format!("{label} failed: {value:#x}"))
}
