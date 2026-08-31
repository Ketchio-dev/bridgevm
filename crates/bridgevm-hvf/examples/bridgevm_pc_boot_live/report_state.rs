use super::super::vcpu_state::VcpuState;
#[path = "report_start_failure.rs"]
mod start_failure;

pub(super) fn write(state: &VcpuState, serial: &str, ram: &[u8]) {
    println!(
        "vcpu_final=pc:{:#x},cpsr:{:#x},exit:{},esr:{:#x},va:{:#x},pa:{:#x}",
        state.pc,
        state.cpsr,
        state.exit_reason,
        state.syndrome,
        state.virtual_address,
        state.physical_address
    );
    let VcpuState { x0, x1, x2, .. } = state;
    println!("gpr0_2=x0:{x0:#x},x1:{x1:#x},x2:{x2:#x}");
    if !serial.is_empty() {
        println!("serial={serial:?}");
    }
    start_failure::write(ram);
}
