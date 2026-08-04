//! PSCI 1.1 state logic (Arm DEN 0022).
//!
//! The CPU lifecycle answers a guest relies on are pure functions of the
//! topology state, so they live here rather than inside the vCPU run loops.
//! That is what makes the `CPU_ON` state table and the `AFFINITY_INFO` affinity
//! search testable without Hypervisor.framework.
//!
//! The run loop still owns locking. This module decides *what* the answer is;
//! the caller performs the state check and the `Off -> OnPending` transition
//! inside one critical section so two racing callers cannot both win.

/// Return codes from DEN 0022.
pub mod status {
    pub const SUCCESS: u64 = 0;
    pub const NOT_SUPPORTED: u64 = (-1i64) as u64;
    pub const INVALID_PARAMS: u64 = (-2i64) as u64;
    pub const DENIED: u64 = (-3i64) as u64;
    pub const ALREADY_ON: u64 = (-4i64) as u64;
    pub const ON_PENDING: u64 = (-5i64) as u64;
    pub const INTERNAL_FAILURE: u64 = (-6i64) as u64;
}

/// `AFFINITY_INFO` return values. These are small positive integers, not the
/// error codes above.
pub mod affinity {
    pub const ON: u64 = 0;
    pub const OFF: u64 = 1;
    pub const ON_PENDING: u64 = 2;
}

/// Per-CPU power state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CpuState {
    Off,
    OnPending,
    On,
}

/// MPIDR bit 31 is RES1 and is not part of the affinity value.
pub const MPIDR_RES1_BIT: u64 = 0x8000_0000;

/// Strip the RES1 bit so two spellings of the same CPU compare equal.
pub const fn normalized_mpidr(value: u64) -> u64 {
    value & !MPIDR_RES1_BIT
}

/// What `CPU_ON` should do for a target in `current`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CpuOnDecision {
    /// Transition the target to `OnPending` and return `SUCCESS`.
    Start,
    /// Return this status and change nothing.
    Reject(u64),
}

/// The DEN 0022 `CPU_ON` state table.
///
/// `Off` starts; `OnPending` is `ON_PENDING`, not `ALREADY_ON`. Reporting a
/// pending CPU as already on tells a caller the CPU is running when it has not
/// yet taken its first instruction.
pub const fn cpu_on_decision(current: CpuState) -> CpuOnDecision {
    match current {
        CpuState::Off => CpuOnDecision::Start,
        CpuState::OnPending => CpuOnDecision::Reject(status::ON_PENDING),
        CpuState::On => CpuOnDecision::Reject(status::ALREADY_ON),
    }
}

/// Number of MPIDR affinity levels addressable by `AFFINITY_INFO`.
pub const MAX_AFFINITY_LEVEL: u64 = 3;

/// Mask keeping only the affinity fields at or above `level`.
///
/// Affinity fields are 8 bits each: Aff0 in `[7:0]`, Aff1 in `[15:8]`,
/// Aff2 in `[23:16]`, Aff3 in `[39:32]`.
pub fn affinity_mask(level: u64) -> Option<u64> {
    match level {
        0 => Some(0xff_00ff_ffff),
        1 => Some(0xff_00ff_ff00),
        2 => Some(0xff_00ff_0000),
        3 => Some(0xff_0000_0000),
        _ => None,
    }
}

/// Resolve `AFFINITY_INFO` over a whole topology.
///
/// `states` must cover every CPU including CPU0. The affinity level selects how
/// much of the MPIDR is compared, so a request can address a cluster rather
/// than a single CPU.
///
/// Returns `ON` if any matching CPU is on, else `ON_PENDING` if any is pending,
/// else `OFF`. A request matching no CPU, or an invalid level, is
/// `INVALID_PARAMS`.
pub fn affinity_info<I>(topology: I, target_mpidr: u64, level: u64) -> u64
where
    I: IntoIterator<Item = (u64, CpuState)>,
{
    let Some(mask) = affinity_mask(level) else {
        return status::INVALID_PARAMS;
    };
    let target = normalized_mpidr(target_mpidr) & mask;

    let mut matched = false;
    let mut any_pending = false;
    for (mpidr, state) in topology {
        if normalized_mpidr(mpidr) & mask != target {
            continue;
        }
        matched = true;
        match state {
            CpuState::On => return affinity::ON,
            CpuState::OnPending => any_pending = true,
            CpuState::Off => {}
        }
    }

    if !matched {
        // A target that names no CPU is a caller error. Reporting OFF would
        // tell the caller a nonexistent CPU is merely powered down.
        return status::INVALID_PARAMS;
    }
    if any_pending {
        affinity::ON_PENDING
    } else {
        affinity::OFF
    }
}

#[cfg(test)]
#[path = "psci_tests.rs"]
mod tests;
