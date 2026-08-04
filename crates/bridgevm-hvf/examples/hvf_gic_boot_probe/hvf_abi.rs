//! Hypervisor.framework ABI declarations, constants, and lifetime guards.

use crate::*;

pub(crate) type HvReturn = i32;

pub(crate) type HvVcpuT = u64;

pub(crate) type HvGicConfig = *mut c_void;

#[repr(C)]
pub(crate) struct HvVcpuExitException {
    pub(crate) syndrome: u64,
    pub(crate) virtual_address: u64,
    pub(crate) physical_address: u64,
}

#[repr(C)]
pub(crate) struct HvVcpuExit {
    pub(crate) reason: u32,
    pub(crate) exception: HvVcpuExitException,
}

#[link(name = "Hypervisor", kind = "framework")]
extern "C" {
    pub(crate) fn hv_vm_create(config: *mut c_void) -> HvReturn;
    pub(crate) fn hv_vm_config_create() -> *mut c_void;
    pub(crate) fn hv_vm_config_set_ipa_size(config: *mut c_void, ipa_bit_length: u32) -> HvReturn;
    pub(crate) fn hv_vm_config_get_max_ipa_size(ipa_bit_length: *mut u32) -> HvReturn;
    pub(crate) fn hv_vm_config_get_el2_supported(el2_supported: *mut bool) -> HvReturn;
    pub(crate) fn hv_vm_config_get_el2_enabled(
        config: *mut c_void,
        el2_enabled: *mut bool,
    ) -> HvReturn;
    pub(crate) fn hv_vm_config_set_el2_enabled(config: *mut c_void, el2_enabled: bool) -> HvReturn;
    pub(crate) fn hv_vm_destroy() -> HvReturn;
    pub(crate) fn hv_vm_map(addr: *mut c_void, ipa: u64, size: usize, flags: u64) -> HvReturn;
    pub(crate) fn hv_vm_unmap(ipa: u64, size: usize) -> HvReturn;
    pub(crate) fn hv_vcpu_create(
        vcpu: *mut HvVcpuT,
        exit: *mut *mut HvVcpuExit,
        config: *mut c_void,
    ) -> HvReturn;
    pub(crate) fn hv_vcpu_destroy(vcpu: HvVcpuT) -> HvReturn;
    pub(crate) fn hv_vcpu_run(vcpu: HvVcpuT) -> HvReturn;
    pub(crate) fn hv_vcpus_exit(vcpus: *const HvVcpuT, vcpu_count: u32) -> HvReturn;
    pub(crate) fn hv_vcpu_get_reg(vcpu: HvVcpuT, reg: u32, value: *mut u64) -> HvReturn;
    pub(crate) fn hv_vcpu_set_reg(vcpu: HvVcpuT, reg: u32, value: u64) -> HvReturn;
    pub(crate) fn hv_vcpu_set_sys_reg(vcpu: HvVcpuT, reg: u16, value: u64) -> HvReturn;
    pub(crate) fn hv_vcpu_get_sys_reg(vcpu: HvVcpuT, reg: u16, value: *mut u64) -> HvReturn;
    pub(crate) fn hv_vcpu_set_vtimer_mask(vcpu: HvVcpuT, vtimer_is_masked: bool) -> HvReturn;
    pub(crate) fn hv_vcpu_get_vtimer_offset(vcpu: HvVcpuT, vtimer_offset: *mut u64) -> HvReturn;
    pub(crate) fn hv_vcpu_set_trap_debug_exceptions(vcpu: HvVcpuT, value: bool) -> HvReturn;
    pub(crate) fn hv_gic_get_redistributor_base(vcpu: HvVcpuT, base: *mut u64) -> HvReturn;
    // Apple in-kernel GICv3 (macOS 15+).
    pub(crate) fn hv_gic_config_create() -> HvGicConfig;
    pub(crate) fn hv_gic_config_set_distributor_base(config: HvGicConfig, base: u64) -> HvReturn;
    pub(crate) fn hv_gic_config_set_redistributor_base(config: HvGicConfig, base: u64) -> HvReturn;
    pub(crate) fn hv_gic_config_set_msi_region_base(config: HvGicConfig, base: u64) -> HvReturn;
    pub(crate) fn hv_gic_config_set_msi_interrupt_range(
        config: HvGicConfig,
        intid_base: u32,
        intid_count: u32,
    ) -> HvReturn;
    pub(crate) fn hv_gic_create(config: HvGicConfig) -> HvReturn;
    pub(crate) fn hv_gic_reset() -> HvReturn;
    pub(crate) fn hv_gic_send_msi(address: u64, intid: u32) -> HvReturn;
    pub(crate) fn hv_gic_set_spi(intid: u32, level: bool) -> HvReturn;
    pub(crate) fn hv_gic_get_spi_interrupt_range(
        intid_base: *mut u32,
        intid_count: *mut u32,
    ) -> HvReturn;
}

pub(crate) const HV_REG_X0: u32 = 0;

pub(crate) const HV_REG_FP: u32 = 29;

pub(crate) const HV_REG_LR: u32 = 30;

pub(crate) const HV_REG_PC: u32 = 31;

pub(crate) const HV_REG_CPSR: u32 = 34;

pub(crate) const HV_MEMORY_READ: u64 = 1;

pub(crate) const HV_MEMORY_WRITE: u64 = 2;

pub(crate) const HV_MEMORY_EXEC: u64 = 4;

pub(crate) const EXIT_CANCELED: u32 = 0;

pub(crate) const EXIT_EXCEPTION: u32 = 1;

pub(crate) const EXIT_VTIMER: u32 = 2;

pub(crate) const EC_DATA_ABORT: u64 = 0x24;

pub(crate) const EC_HVC: u64 = 0x16;

pub(crate) const EC_SYS_REG_TRAP: u64 = 0x18;

pub(crate) const EC_WATCHPOINT_LOWER: u64 = 0x34;

pub(crate) const EC_WATCHPOINT_SAME: u64 = 0x35;

pub(crate) const EC_SOFTSTEP_LOWER: u64 = 0x32;

pub(crate) const EC_SOFTSTEP_SAME: u64 = 0x33;

pub(crate) const HV_SYS_REG_DBGWVR0_EL1: u16 = 0x8006;

pub(crate) const HV_SYS_REG_DBGWCR0_EL1: u16 = 0x8007;

pub(crate) const HV_SYS_REG_MDSCR_EL1: u16 = 0x8012;

pub(crate) const HV_SYS_REG_MPIDR_EL1: u16 = 0xc005;

pub(crate) const HV_SYS_REG_ID_AA64DFR0_EL1: u16 = 0xc028;

pub(crate) const HV_SYS_REG_SCTLR_EL1: u16 = 0xc080;

pub(crate) const HV_SYS_REG_TTBR0_EL1: u16 = 0xc100;

pub(crate) const HV_SYS_REG_TTBR1_EL1: u16 = 0xc101;

pub(crate) const HV_SYS_REG_TCR_EL1: u16 = 0xc102;

pub(crate) const HV_SYS_REG_SPSR_EL1: u16 = 0xc200;

pub(crate) const HV_SYS_REG_ELR_EL1: u16 = 0xc201;

pub(crate) const HV_SYS_REG_ESR_EL1: u16 = 0xc290;

pub(crate) const HV_SYS_REG_FAR_EL1: u16 = 0xc300;

pub(crate) const HV_SYS_REG_MAIR_EL1: u16 = 0xc510;

pub(crate) const HV_SYS_REG_VBAR_EL1: u16 = 0xc600;

pub(crate) const HV_SYS_REG_SP_EL0: u16 = 0xc208;

pub(crate) const HV_SYS_REG_SP_EL1: u16 = 0xe208;

pub(crate) const HV_SYS_REG_CNTP_CTL_EL0: u16 = 0xdf11;

pub(crate) const HV_SYS_REG_CNTP_CVAL_EL0: u16 = 0xdf12;

pub(crate) const HV_SYS_REG_CNTV_CTL_EL0: u16 = 0xdf19;

pub(crate) const HV_SYS_REG_CNTV_CVAL_EL0: u16 = 0xdf1a;

pub(crate) const HV_GIC_REG_GICM_SET_SPI_NSR: u64 = 0x40;

// Watch the poll target for stores: 8-byte aligned address, BAS=0xFF (8 bytes),
// LSC=0b10 (store), PAC=0b11 (EL0+EL1), E=1. = 0x1FF7.
pub(crate) const WATCH_TARGET: u64 = 0x5ffd_f798;

pub(crate) const DBGWCR_STORE_8B: u64 = 0x1ff7;

pub(crate) const DEFAULT_MAX_EXITS: u64 = 50_000_000;

pub(crate) const WATCHDOG_MS: u64 = 8000;

pub(crate) const DEFAULT_MAX_REBOOTS: u64 = 8;

pub(crate) const PSCI_SUCCESS: u64 = 0;

pub(crate) const PSCI_NOT_SUPPORTED: u64 = (-1i64) as u64;

pub(crate) const PSCI_INVALID_PARAMS: u64 = (-2i64) as u64;

pub(crate) const PSCI_ALREADY_ON: u64 = (-4i64) as u64;

pub(crate) const PSCI_VERSION: u64 = 0x8400_0000;

pub(crate) const PSCI_CPU_OFF: u64 = 0x8400_0002;

pub(crate) const PSCI_CPU_ON_32: u64 = 0x8400_0003;

pub(crate) const PSCI_CPU_ON_64: u64 = 0xc400_0003;

pub(crate) const PSCI_AFFINITY_INFO_32: u64 = 0x8400_0004;

pub(crate) const PSCI_AFFINITY_INFO_64: u64 = 0xc400_0004;

pub(crate) const PSCI_SYSTEM_OFF: u64 = 0x8400_0008;

pub(crate) const PSCI_SYSTEM_RESET: u64 = 0x8400_0009;

pub(crate) const PSCI_FEATURES: u64 = 0x8400_000A;

pub(crate) const SMCCC_VERSION: u64 = 0x8000_0000;

pub(crate) const TRNG_VERSION: u64 = 0x8400_0050;

pub(crate) const TRNG_FEATURES: u64 = 0x8400_0051;

pub(crate) const TRNG_GET_UUID: u64 = 0x8400_0052;

pub(crate) const TRNG_RND_32: u64 = 0x8400_0053;

pub(crate) const TRNG_RND_64: u64 = 0xc400_0053;

pub(crate) const MPIDR_RES1_BIT: u64 = 0x8000_0000;

pub(crate) struct HvVmGuard;

impl Drop for HvVmGuard {
    fn drop(&mut self) {
        unsafe {
            hv_vm_destroy();
        }
    }
}

pub(crate) struct HvVcpuGuard {
    pub(crate) vcpu: HvVcpuT,
}

impl Drop for HvVcpuGuard {
    fn drop(&mut self) {
        unsafe {
            hv_vcpu_destroy(self.vcpu);
        }
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PsciState {
    Off,
    OnPending,
    On,
}
