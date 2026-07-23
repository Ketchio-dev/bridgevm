//! vCPU register reset, context, and watchpoint helpers.

use crate::*;

pub(crate) fn read_reg(vcpu: HvVcpuT, reg: u32) -> u64 {
    let mut value = 0u64;
    unsafe {
        hv_vcpu_get_reg(vcpu, reg, &mut value);
    }
    value
}

pub(crate) fn read_sys_reg(vcpu: HvVcpuT, reg: u16) -> u64 {
    let mut value = 0u64;
    unsafe {
        hv_vcpu_get_sys_reg(vcpu, reg, &mut value);
    }
    value
}

pub(crate) fn print_gpr_context(vcpu: HvVcpuT) {
    for &(start, end) in &[(0u32, 8u32), (8, 16), (16, 24), (24, 29)] {
        print!("GPRS[x{start}..x{}]:", end - 1);
        for index in start..end {
            let value = read_reg(vcpu, HV_REG_X0 + index);
            print!(" x{index}={value:#x}");
        }
        println!();
    }
}

pub(crate) fn reset_vcpu_for_boot(vcpu: HvVcpuT) {
    // SAFETY: Category 8 - FFI boundary. `vcpu` is the live HVF vCPU handle
    // reset on the run-loop thread while it is not inside `hv_vcpu_run`; all
    // register identifiers are HVF constants, and every output pointer below
    // is a stack local valid for the duration of its call.
    unsafe {
        for reg in HV_REG_X0..=HV_REG_LR {
            hv_vcpu_set_reg(vcpu, reg, 0);
        }
        for reg in [
            HV_SYS_REG_SCTLR_EL1,
            HV_SYS_REG_TTBR0_EL1,
            HV_SYS_REG_TTBR1_EL1,
            HV_SYS_REG_TCR_EL1,
            HV_SYS_REG_SPSR_EL1,
            HV_SYS_REG_ELR_EL1,
            HV_SYS_REG_ESR_EL1,
            HV_SYS_REG_FAR_EL1,
            HV_SYS_REG_MAIR_EL1,
            HV_SYS_REG_VBAR_EL1,
            HV_SYS_REG_SP_EL0,
            HV_SYS_REG_SP_EL1,
            HV_SYS_REG_CNTP_CTL_EL0,
            HV_SYS_REG_CNTP_CVAL_EL0,
            HV_SYS_REG_CNTV_CTL_EL0,
            HV_SYS_REG_CNTV_CVAL_EL0,
        ] {
            hv_vcpu_set_sys_reg(vcpu, reg, 0);
        }
        hv_vcpu_set_sys_reg(vcpu, HV_SYS_REG_MPIDR_EL1, 0x8000_0000);
        let mut dfr0_before = 0u64;
        let dfr0_read_status =
            hv_vcpu_get_sys_reg(vcpu, HV_SYS_REG_ID_AA64DFR0_EL1, &mut dfr0_before);
        let dfr0_after = (dfr0_before & !(0xf << 8)) | (0x1 << 8);
        let dfr0_set_status = hv_vcpu_set_sys_reg(vcpu, HV_SYS_REG_ID_AA64DFR0_EL1, dfr0_after);
        println!(
            "ID_AA64DFR0_EL1 PMUVer: before={dfr0_before:#x} read={dfr0_read_status:#x} after={dfr0_after:#x} set={dfr0_set_status:#x}"
        );
        let mut rdbase = 0u64;
        let rdr = hv_gic_get_redistributor_base(vcpu, &mut rdbase);
        println!("hv_gic_get_redistributor_base(vcpu0) = {rdr:#x} -> {rdbase:#x}");
        hv_vcpu_set_reg(vcpu, HV_REG_PC, 0x0);
        hv_vcpu_set_reg(vcpu, HV_REG_CPSR, 0x3c5);
        hv_vcpu_set_reg(vcpu, HV_REG_X0, machine::RAM_BASE);
        hv_vcpu_set_vtimer_mask(vcpu, false);
    }
}

pub(crate) fn arm_watchpoint_for_boot(vcpu: HvVcpuT, watch_addr: Option<u64>) {
    let Some(addr) = watch_addr else {
        return;
    };
    // SAFETY: Category 8 - FFI boundary. `vcpu` is the live HVF vCPU handle,
    // `addr` is written as a guest debug address value, and `&mut mdscr` is a
    // valid stack output pointer for the single `hv_vcpu_get_sys_reg` call.
    unsafe {
        hv_vcpu_set_trap_debug_exceptions(vcpu, true);
        hv_vcpu_set_sys_reg(vcpu, HV_SYS_REG_DBGWVR0_EL1, addr);
        hv_vcpu_set_sys_reg(vcpu, HV_SYS_REG_DBGWCR0_EL1, DBGWCR_STORE_8B);
        let mut mdscr = 0u64;
        hv_vcpu_get_sys_reg(vcpu, HV_SYS_REG_MDSCR_EL1, &mut mdscr);
        hv_vcpu_set_sys_reg(vcpu, HV_SYS_REG_MDSCR_EL1, mdscr | (1 << 15));
    }
    println!("watchpoint armed on {addr:#x} (store)");
}
