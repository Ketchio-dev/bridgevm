//! Unexpected owner-thread exits must not become healthy joined results.
use crate::*;

#[derive(Clone, Copy)]
pub(crate) enum SecondaryUnexpectedStop {
    ExitReason(u32),
    SysReg(SysRegTrap, u64, u64),
    Exception(u64, u64, u64),
    ExitCap(u64),
}

impl VcpuControl {
    pub(crate) fn stop_unexpected_secondary(
        &self,
        primary_vcpu: HvVcpuT,
        cause: SecondaryUnexpectedStop,
    ) -> bool {
        finish_unexpected_secondary_stop(self.index, cause, || self.record_run_error(primary_vcpu))
    }
}

fn finish_unexpected_secondary_stop(
    index: u64,
    cause: SecondaryUnexpectedStop,
    record_error: impl FnOnce(),
) -> bool {
    match cause {
        SecondaryUnexpectedStop::ExitReason(reason) => {
            println!("secondary vCPU{index} stopped on exit reason {reason}")
        }
        SecondaryUnexpectedStop::SysReg(trap, esr, pc) => println!(
            "secondary vCPU{index} unsupported system register trap {} ESR {esr:#x} @ PC {pc:#x}",
            trap.describe()
        ),
        SecondaryUnexpectedStop::Exception(ec, esr, pc) => {
            println!("secondary vCPU{index} exception EC {ec:#x} ESR {esr:#x} @ PC {pc:#x}")
        }
        SecondaryUnexpectedStop::ExitCap(max_exits) => {
            println!("secondary vCPU{index} exit cap {max_exits}")
        }
    }
    record_error();
    true
}

#[cfg(test)]
#[path = "secondary_stop_tests.rs"]
mod tests;
