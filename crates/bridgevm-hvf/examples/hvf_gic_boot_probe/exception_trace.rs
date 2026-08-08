//! Exception syndrome and trapped system-register diagnostics.

use crate::*;

impl SysRegTrap {
    pub(crate) fn decode(esr: u64) -> Self {
        let iss = esr & 0x01ff_ffff;
        Self {
            op0: ((iss >> 20) & 0x3) as u8,
            op2: ((iss >> 17) & 0x7) as u8,
            op1: ((iss >> 14) & 0x7) as u8,
            crn: ((iss >> 10) & 0xf) as u8,
            rt: ((iss >> 5) & 0x1f) as u32,
            crm: ((iss >> 1) & 0xf) as u8,
            is_read: (iss & 1) != 0,
        }
    }
    pub(crate) fn name(self) -> &'static str {
        match (self.op0, self.op1, self.crn, self.crm, self.op2) {
            (2, 0, 1, 0, 4) => "OSLAR_EL1",
            (2, 0, 1, 1, 4) => "OSLSR_EL1",
            (2, 0, 1, 3, 4) => "OSDLR_EL1",
            (3, 3, 9, 12, 0) => "PMCR_EL0",
            (3, 3, 9, 12, 1) => "PMCNTENSET_EL0",
            (3, 3, 9, 12, 2) => "PMCNTENCLR_EL0",
            (3, 3, 9, 12, 3) => "PMOVSCLR_EL0",
            (3, 0, 9, 14, 2) => "PMINTENCLR_EL1",
            (3, 3, 9, 14, 0) => "PMUSERENR_EL0",
            (3, 3, 9, 14, 1) => "PMINTENSET_EL1",
            (3, 3, 14, 15, 7) => "PMCCFILTR_EL0",
            _ => "<unknown>",
        }
    }
    pub(crate) fn describe(self) -> String {
        let dir = if self.is_read { "MRS" } else { "MSR" };
        format!(
            "{dir} {} (S{}_{}_C{}_C{}_{}, Rt=x{})",
            self.name(),
            self.op0,
            self.op1,
            self.crn,
            self.crm,
            self.op2,
            self.rt
        )
    }
}

pub(crate) fn exception_class_name(ec: u64) -> &'static str {
    match ec {
        0x00 => "unknown/uncategorized",
        0x01 => "trapped WFI/WFE",
        0x07 => "trapped FP/SIMD/SVE",
        0x15 => "SVC AArch64",
        0x16 => "HVC AArch64",
        0x18 => "trapped MSR/MRS system register",
        0x20 => "instruction abort lower EL",
        0x21 => "instruction abort same EL",
        0x24 => "data abort lower EL",
        0x25 => "data abort same EL",
        0x26 => "SP alignment fault",
        0x2f => "SError",
        0x30 => "breakpoint lower EL",
        0x31 => "breakpoint same EL",
        0x32 => "software step lower EL",
        0x33 => "software step same EL",
        0x34 => "watchpoint lower EL",
        0x35 => "watchpoint same EL",
        _ => "<unknown EC>",
    }
}

pub(crate) fn describe_esr(esr: u64) -> String {
    let ec = (esr >> 26) & 0x3f;
    if ec == EC_SYS_REG_TRAP {
        let trap = SysRegTrap::decode(esr);
        return format!("{}: {}", exception_class_name(ec), trap.describe());
    }
    if ec == 0x15 || ec == EC_HVC {
        let iss = esr & 0x01ff_ffff;
        let imm16 = iss & 0xffff;
        return format!(
            "{} EC={ec:#x} ISS={iss:#x} imm16={imm16:#x}",
            exception_class_name(ec)
        );
    }
    format!(
        "{} EC={ec:#x} ISS={:#x}",
        exception_class_name(ec),
        esr & 0x01ff_ffff
    )
}

#[cfg(test)]
mod esr_tests {
    use super::*;

    #[test]
    fn describes_svc_immediate() {
        assert_eq!(
            describe_esr(0x5600_1004),
            "SVC AArch64 EC=0x15 ISS=0x1004 imm16=0x1004"
        );
    }

    #[test]
    fn describes_hvc_immediate() {
        assert_eq!(
            describe_esr((EC_HVC << 26) | 0xabcd),
            "HVC AArch64 EC=0x16 ISS=0xabcd imm16=0xabcd"
        );
    }
}

pub(crate) unsafe fn emulate_debug_os_lock_sysreg(vcpu: HvVcpuT, trap: SysRegTrap) -> bool {
    match (
        trap.op0,
        trap.op1,
        trap.crn,
        trap.crm,
        trap.op2,
        trap.is_read,
    ) {
        // Linux clears the Arm debug OS lock / double lock while bringing up
        // debug infrastructure. HVF traps these implementation-defined debug
        // registers; treating the writes as no-ops and reads as unlocked lets
        // the guest proceed without exposing host debug state.
        (2, 0, 1, 0, 4, false) | (2, 0, 1, 3, 4, false) => true,
        (2, 0, 1, 1, 4, true) | (2, 0, 1, 3, 4, true) => {
            if trap.rt != 31 {
                hv_vcpu_set_reg(vcpu, HV_REG_X0 + trap.rt, 0);
            }
            true
        }
        // PMCCNTR_EL0: winload measures CPU frequency with cycle-counter
        // deltas and __fastfail()s (BRK #0xf004) when the delta is zero
        // (usgic-diag2 stall: MRS x9,S3_3_C9_C13_0; MUL; CBNZ; BRK). A
        // counter that actually advances is required -- scale the 24 MHz
        // host timebase up to a plausible CPU clock.
        (3, 3, 9, 13, 0, true) => {
            if trap.rt != 31 {
                let cycles = crate::usgic_bridge::host_time_ticks().wrapping_mul(100);
                hv_vcpu_set_reg(vcpu, HV_REG_X0 + trap.rt, cycles);
            }
            true
        }
        // Remaining PMU group (PMCR/PMSELR/PMXEVCNTR/...): the
        // userspace-GIC configuration traps these (Windows reads them
        // during kernel bringup; first seen at usgic-boot3). RAZ/WI
        // matches a PMU that counts nothing.
        (3, 3, 9, 12..=14, _, is_read) | (3, 0, 9, 14, _, is_read) => {
            if is_read && trap.rt != 31 {
                hv_vcpu_set_reg(vcpu, HV_REG_X0 + trap.rt, 0);
            }
            true
        }
        (3, 3, 14, 15, 7, is_read) => {
            // PMCCFILTR_EL0.
            if is_read && trap.rt != 31 {
                hv_vcpu_set_reg(vcpu, HV_REG_X0 + trap.rt, 0);
            }
            true
        }
        // CNTPCT_EL0: the physical counter traps in the userspace-GIC
        // configuration (ntoskrnl reads it once past timer bringup;
        // usgic-diag6). The host timebase ticks at the advertised CNTFRQ
        // (24 MHz), so mach_absolute_time IS the counter.
        (3, 3, 14, 0, 1, true) => {
            if trap.rt != 31 {
                let ticks = crate::usgic_bridge::host_time_ticks();
                hv_vcpu_set_reg(vcpu, HV_REG_X0 + trap.rt, ticks);
            }
            true
        }
        _ => false,
    }
}
