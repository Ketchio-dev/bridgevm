//! Shared fixtures for these tests.

use crate::probe_mmio::*;
use crate::*;

pub(super) fn sysreg_trap_syndrome(
    is_read: bool,
    register: u8,
    op0: u8,
    op1: u8,
    crn: u8,
    crm: u8,
    op2: u8,
) -> u64 {
    (AARCH64_SYSREG_TRAP_EXCEPTION_CLASS << 26)
        | (u64::from(op0) << 20)
        | (u64::from(op2) << 17)
        | (u64::from(op1) << 14)
        | (u64::from(crn) << 10)
        | (u64::from(register) << 5)
        | (u64::from(crm) << 1)
        | u64::from(is_read as u8)
}

pub(super) fn gic_cpu_write(
    cpu: &mut GicV3CpuInterfaceState,
    bus: &mut MmioBus,
    sys_reg: u16,
    value: u64,
) -> Option<GicV3CpuInterfaceAction> {
    cpu.handle_system_register_access(
        bus,
        DecodedSystemRegisterAccess {
            is_read: false,
            register: 0,
            sys_reg,
            op0: 3,
            op1: 0,
            crn: 0,
            crm: 0,
            op2: 0,
        },
        Some(value),
    )
}

pub(super) fn gic_cpu_read(
    cpu: &mut GicV3CpuInterfaceState,
    bus: &mut MmioBus,
    sys_reg: u16,
) -> Option<GicV3CpuInterfaceAction> {
    cpu.handle_system_register_access(
        bus,
        DecodedSystemRegisterAccess {
            is_read: true,
            register: 1,
            sys_reg,
            op0: 3,
            op1: 0,
            crn: 0,
            crm: 0,
            op2: 0,
        },
        None,
    )
}
