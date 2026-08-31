//! Terminal architectural state for first-failure analysis.

use super::super::vcpu_state::VcpuState;

pub(super) fn write(state: &VcpuState, serial: &str) {
    println!(
        "vcpu_final=pc:{:#x},cpsr:{:#x},exit:{},esr:{:#x},va:{:#x},pa:{:#x}",
        state.pc,
        state.cpsr,
        state.exit_reason,
        state.syndrome,
        state.virtual_address,
        state.physical_address
    );
    println!(
        "gpr0_2=x0:{:#x},x1:{:#x},x2:{:#x}",
        state.x0, state.x1, state.x2
    );
    if !serial.is_empty() {
        println!("serial={serial:?}");
    }
}
