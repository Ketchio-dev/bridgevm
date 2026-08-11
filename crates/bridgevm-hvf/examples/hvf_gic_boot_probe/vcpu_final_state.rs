//! Owner-thread vCPU snapshots and bounded post-stop guest diagnostics.
//!
//! Secondary register access is valid only on the thread that owns that HVF
//! vCPU. Capture therefore happens in `secondary_vcpu_thread` after its run
//! loop has stopped, while guest-memory interpretation waits until every vCPU
//! has joined. Nothing in this module writes guest state.

use crate::*;

const HV_SUCCESS: HvReturn = 0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CapturedRegister {
    pub(crate) status: HvReturn,
    pub(crate) value: u64,
}

impl CapturedRegister {
    const NOT_READ: Self = Self {
        status: -1,
        value: 0,
    };

    pub(super) fn value(self) -> Option<u64> {
        (self.status == HV_SUCCESS).then_some(self.value)
    }

    fn general(vcpu: HvVcpuT, reg: u32) -> Self {
        let mut value = 0;
        // SAFETY: the caller is the owning vCPU thread (or CPU0's primary
        // thread), `vcpu` is stopped, `reg` is an HVF register ID, and the
        // output pointer remains valid for the call.
        let status = unsafe { hv_vcpu_get_reg(vcpu, reg, &mut value) };
        Self { status, value }
    }

    fn system(vcpu: HvVcpuT, reg: u16) -> Self {
        let mut value = 0;
        // SAFETY: same owner-thread/stopped-vCPU contract as `general`; `reg`
        // is an HVF system-register ID and `value` is a live stack output.
        let status = unsafe { hv_vcpu_get_sys_reg(vcpu, reg, &mut value) };
        Self { status, value }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VcpuFinalState {
    pub(crate) index: u64,
    pub(crate) generation: u64,
    pub(crate) psci_state: PsciState,
    pub(crate) expected_mpidr: u64,
    pub(crate) exits: u64,
    pub(crate) x: [CapturedRegister; 31],
    pub(crate) pc: CapturedRegister,
    pub(crate) cpsr: CapturedRegister,
    pub(crate) mpidr_el1: CapturedRegister,
    pub(crate) sp_el0: CapturedRegister,
    pub(crate) sp_el1: CapturedRegister,
    pub(crate) sctlr_el1: CapturedRegister,
    pub(crate) tcr_el1: CapturedRegister,
    pub(crate) ttbr0_el1: CapturedRegister,
    pub(crate) ttbr1_el1: CapturedRegister,
    pub(crate) mair_el1: CapturedRegister,
}

#[derive(Debug, Default)]
pub(crate) struct SecondaryVcpuStopResult {
    pub(crate) exit_counts: Vec<(u64, u64)>,
    pub(crate) run_error: bool,
    pub(crate) final_states: Vec<VcpuFinalState>,
    pub(crate) missing_final_states: Vec<u64>,
}

impl VcpuControl {
    pub(crate) fn publish_final_state(&self, state: VcpuFinalState) -> bool {
        let mut published = self
            .final_state
            .lock()
            .expect("secondary vCPU final-state mutex");
        if published.is_some() {
            return false;
        }
        *published = Some(state);
        true
    }

    pub(crate) fn take_final_state(&self) -> Option<VcpuFinalState> {
        self.final_state
            .lock()
            .expect("secondary vCPU final-state mutex")
            .take()
    }
}

impl SecondaryVcpuSet {
    pub(crate) fn shutdown_and_join(self) -> SecondaryVcpuStopResult {
        self.shutdown.store(true, Ordering::SeqCst);
        for control in &self.controls {
            control.notify_shutdown();
        }
        for control in &self.controls {
            control.request_exit_if_published();
        }
        for handle in self.handles {
            handle.join().expect("join secondary vCPU thread");
        }
        let run_error = self
            .controls
            .iter()
            .any(|control| control.run_error.load(Ordering::SeqCst));
        let exit_counts = self
            .controls
            .iter()
            .map(|control| (control.index, control.exits.load(Ordering::SeqCst)))
            .collect();
        let mut final_states = Vec::new();
        let mut missing_final_states = Vec::new();
        for control in &self.controls {
            if let Some(state) = control.take_final_state() {
                final_states.push(state);
            } else if control.created.load(Ordering::Acquire) {
                missing_final_states.push(control.index);
            }
        }
        SecondaryVcpuStopResult {
            exit_counts,
            run_error,
            final_states,
            missing_final_states,
        }
    }
}

impl SecondaryVcpuStopResult {
    pub(crate) fn report_with_primary(
        &self,
        mem: &dyn GuestMemoryMut,
        primary_vcpu: HvVcpuT,
        primary_exits: u64,
        generation: u64,
    ) {
        let primary = VcpuFinalState::capture_on_owner_thread(
            0,
            generation,
            PsciState::On,
            0x8000_0000 | machine::cpu_mpidr(0),
            primary_exits,
            primary_vcpu,
        );
        report_vcpu_final_states(
            mem,
            &primary,
            &self.final_states,
            &self.missing_final_states,
        );
    }
}

impl VcpuFinalState {
    /// Capture must be called by the thread that owns `vcpu`, after that vCPU
    /// has left `hv_vcpu_run`. Every HVF status is retained; a failed read is
    /// never represented as a successful zero register.
    pub(crate) fn capture_on_owner_thread(
        index: u64,
        generation: u64,
        psci_state: PsciState,
        expected_mpidr: u64,
        exits: u64,
        vcpu: HvVcpuT,
    ) -> Self {
        let mut x = [CapturedRegister::NOT_READ; 31];
        for (register, slot) in x.iter_mut().enumerate() {
            *slot = CapturedRegister::general(vcpu, HV_REG_X0 + register as u32);
        }
        Self {
            index,
            generation,
            psci_state,
            expected_mpidr,
            exits,
            x,
            pc: CapturedRegister::general(vcpu, HV_REG_PC),
            cpsr: CapturedRegister::general(vcpu, HV_REG_CPSR),
            mpidr_el1: CapturedRegister::system(vcpu, HV_SYS_REG_MPIDR_EL1),
            sp_el0: CapturedRegister::system(vcpu, HV_SYS_REG_SP_EL0),
            sp_el1: CapturedRegister::system(vcpu, HV_SYS_REG_SP_EL1),
            sctlr_el1: CapturedRegister::system(vcpu, HV_SYS_REG_SCTLR_EL1),
            tcr_el1: CapturedRegister::system(vcpu, HV_SYS_REG_TCR_EL1),
            ttbr0_el1: CapturedRegister::system(vcpu, HV_SYS_REG_TTBR0_EL1),
            ttbr1_el1: CapturedRegister::system(vcpu, HV_SYS_REG_TTBR1_EL1),
            mair_el1: CapturedRegister::system(vcpu, HV_SYS_REG_MAIR_EL1),
        }
    }

    pub(super) fn stage1_context(&self) -> Result<Stage1Context, String> {
        for (name, register) in [
            ("SCTLR_EL1", self.sctlr_el1),
            ("TCR_EL1", self.tcr_el1),
            ("TTBR0_EL1", self.ttbr0_el1),
            ("TTBR1_EL1", self.ttbr1_el1),
            ("MAIR_EL1", self.mair_el1),
        ] {
            if register.status != HV_SUCCESS {
                return Err(format!("{name} read failed status={:#x}", register.status));
            }
        }
        Ok(Stage1Context {
            sctlr_el1: self.sctlr_el1.value,
            tcr_el1: self.tcr_el1.value,
            ttbr0_el1: self.ttbr0_el1.value,
            ttbr1_el1: self.ttbr1_el1.value,
            mair_el1: self.mair_el1.value,
        })
    }

    pub(super) fn required_x(&self, index: usize) -> Result<u64, String> {
        self.x[index]
            .value()
            .ok_or_else(|| format!("x{index} read failed status={:#x}", self.x[index].status))
    }

    #[cfg(test)]
    pub(crate) fn test_state(index: u64) -> Self {
        let ok = CapturedRegister {
            status: HV_SUCCESS,
            value: 0,
        };
        Self {
            index,
            generation: 3,
            psci_state: PsciState::On,
            expected_mpidr: 0x8000_0000 | index,
            exits: 7,
            x: [ok; 31],
            pc: ok,
            cpsr: ok,
            mpidr_el1: ok,
            sp_el0: ok,
            sp_el1: ok,
            sctlr_el1: ok,
            tcr_el1: ok,
            ttbr0_el1: ok,
            ttbr1_el1: ok,
            mair_el1: ok,
        }
    }
}

fn print_register(label: &str, register: CapturedRegister) {
    if register.status == HV_SUCCESS {
        print!(" {label}={:#x}", register.value);
    } else {
        print!(" {label}=<read-failed:{:#x}>", register.status);
    }
}

pub(crate) fn report_vcpu_final_states(
    mem: &dyn GuestMemoryMut,
    primary: &VcpuFinalState,
    secondaries: &[VcpuFinalState],
    missing_secondaries: &[u64],
) {
    let expected = 1 + secondaries.len() + missing_secondaries.len();
    println!(
        "VCPU-FINAL: captured={} expected={expected} missing={missing_secondaries:?} (owner-thread registers; post-join RAM)",
        1 + secondaries.len(),
    );
    if missing_secondaries.is_empty() {
        println!("VCPU-FINAL-STATUS: complete");
    } else {
        println!("VCPU-FINAL-STATUS: incomplete; created secondary snapshots are missing");
    }
    report_vcpu_final_state(mem, primary);
    for state in secondaries {
        report_vcpu_final_state(mem, state);
    }
}

fn report_vcpu_final_state(mem: &dyn GuestMemoryMut, state: &VcpuFinalState) {
    print!(
        "VCPU-FINAL[{}]: generation={} psci={:?} exits={} expected_mpidr={:#x}",
        state.index, state.generation, state.psci_state, state.exits, state.expected_mpidr
    );
    print_register("mpidr_el1", state.mpidr_el1);
    print_register("pc", state.pc);
    print_register("cpsr", state.cpsr);
    print_register("sp_el0", state.sp_el0);
    print_register("sp_el1", state.sp_el1);
    println!();
    for (start, end) in [(0usize, 8usize), (8, 16), (16, 24), (24, 31)] {
        print!("VCPU-FINAL[{}]-GPRS[x{start}..x{}]:", state.index, end - 1);
        for index in start..end {
            print_register(&format!("x{index}"), state.x[index]);
        }
        println!();
    }

    let context = match state.stage1_context() {
        Ok(context) => context,
        Err(reason) => {
            println!("VCPU-FINAL[{}]-STAGE1: unavailable: {reason}", state.index);
            return;
        }
    };
    println!(
        "VCPU-FINAL[{}]-STAGE1: SCTLR={:#x} MMU={} TCR={:#x} TTBR0={:#x} TTBR1={:#x} MAIR={:#x}",
        state.index,
        context.sctlr_el1,
        context.sctlr_el1 & 1 != 0,
        context.tcr_el1,
        context.ttbr0_el1,
        context.ttbr1_el1,
        context.mair_el1
    );
    println!(
        "VCPU-FINAL[{}]-IDENTITY: translated execution bytes are authoritative input; PE shape alone is not loaded-module proof",
        state.index
    );
    for (name, register, before) in [
        ("pc", state.pc, 0x20),
        ("lr", state.x[HV_REG_LR as usize], 0x28),
    ] {
        let Some(va) = register.value() else {
            println!(
                "VCPU-FINAL[{}]-{name}: register read failed status={:#x}",
                state.index, register.status
            );
            continue;
        };
        let label = format!("vcpu{}-{name}", state.index);
        let ipa = print_stage1_translation(mem, &context, &label, va);
        print_translated_pe_owner(mem, &label, ipa);
        dump_translated_guest_bytes(mem, &format!("VCPU-CODE[{label}]"), ipa, before, 0x80);
        print_translated_instruction_words(mem, &label, va, ipa, before, 0x80);
    }
    if let Some(fp) = state.x[HV_REG_FP as usize].value().filter(|value| *value != 0) {
        let label = format!("vcpu{}-fp", state.index);
        let fp_ipa = print_stage1_translation(mem, &context, &label, fp);
        dump_translated_guest_bytes(mem, &format!("VCPU-FRAME[{label}]"), fp_ipa, 0, 0x80);
        let limit = usize::try_from(env_u64("BRIDGEVM_FRAME_CHAIN_LIMIT", 12)).unwrap_or(12);
        print_frame_chain(mem, &context, fp, limit.min(64));
    }
    if let Some(sp) = state.sp_el1.value().filter(|value| *value != 0) {
        let label = format!("vcpu{}-sp_el1", state.index);
        let sp_ipa = print_stage1_translation(mem, &context, &label, sp);
        dump_translated_guest_bytes(mem, &format!("VCPU-STACK[{label}]"), sp_ipa, 0, 0x100);
    }
    viogpu_final_state::report_viogpu_waiter(mem, &context, state);
}
