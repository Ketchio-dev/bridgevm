//! vCPU0's debug exits: the debug OS-lock system registers, and the
//! BRIDGEVM_WATCH hardware watchpoint with the single step over each store.

use super::{BootGeneration, Probe};
use crate::*;

impl BootGeneration {
    /// A trapped system register: the debug OS-lock registers are emulated,
    /// anything else stops the run.
    pub(super) unsafe fn on_sys_reg_trap(&mut self, vcpu: HvVcpuT, esr: u64) -> Result<(), String> {
        let last_pc = self.last_pc;
        let trap = SysRegTrap::decode(esr);
        if emulate_debug_os_lock_sysreg(vcpu, trap) {
            hv_vcpu_set_reg(vcpu, HV_REG_PC, last_pc.wrapping_add(4));
            Ok(())
        } else {
            Err(format!(
                "unsupported system register trap {} ESR {esr:#x} @ PC {last_pc:#x}",
                trap.describe()
            ))
        }
    }

    /// The watched store hit: single-step over it to read the stored value.
    pub(super) unsafe fn on_watchpoint(&mut self, vcpu: HvVcpuT) {
        let last_pc = self.last_pc;
        self.watch_hits += 1;
        let watch_hits = self.watch_hits;
        let mut lr = 0u64;
        hv_vcpu_get_reg(vcpu, HV_REG_X0 + 30, &mut lr);
        self.last_watch_pc = last_pc;
        self.last_watch_lr = lr;
        print!("WATCH #{watch_hits}: store @ PC {last_pc:#x} LR {lr:#x}");
        // Single-step over the store: disable the watchpoint and arm
        // PSTATE.SS so the store retires and we can read the new value.
        hv_vcpu_set_sys_reg(vcpu, HV_SYS_REG_DBGWCR0_EL1, 0);
        let mut md = 0u64;
        hv_vcpu_get_sys_reg(vcpu, HV_SYS_REG_MDSCR_EL1, &mut md);
        hv_vcpu_set_sys_reg(vcpu, HV_SYS_REG_MDSCR_EL1, md | 1); // SS
        let mut cp = 0u64;
        hv_vcpu_get_reg(vcpu, HV_REG_CPSR, &mut cp);
        hv_vcpu_set_reg(vcpu, HV_REG_CPSR, cp | (1 << 21)); // PSTATE.SS
        // do NOT advance PC: re-execute the store under single-step.
    }

    /// The step over the watched store retired: print the stored value, clear
    /// single-step and re-arm the watchpoint unless it has hit enough.
    pub(super) unsafe fn on_softstep(&mut self, probe: &mut Probe<'_>) {
        let (vcpu, watch_target) = (probe.vcpu, probe.watch_target);
        let mut bytes = [0u8; 8];
        let cur = if probe.guest_ram.read_into(watch_target, &mut bytes) {
            u64::from_le_bytes(bytes)
        } else {
            0
        };
        println!(" -> {watch_target:#x} = {cur:#x}");
        // Clear single-step; re-arm the watchpoint unless we have enough.
        let mut md = 0u64;
        hv_vcpu_get_sys_reg(vcpu, HV_SYS_REG_MDSCR_EL1, &mut md);
        hv_vcpu_set_sys_reg(vcpu, HV_SYS_REG_MDSCR_EL1, md & !1);
        let mut cp = 0u64;
        hv_vcpu_get_reg(vcpu, HV_REG_CPSR, &mut cp);
        hv_vcpu_set_reg(vcpu, HV_REG_CPSR, cp & !(1 << 21));
        if self.watch_hits < 40 {
            hv_vcpu_set_sys_reg(vcpu, HV_SYS_REG_DBGWCR0_EL1, DBGWCR_STORE_8B);
        }
        // do NOT advance PC: the step already retired the instruction.
    }
}
