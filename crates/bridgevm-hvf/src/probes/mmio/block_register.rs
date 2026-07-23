//! Split out of mmio.rs by responsibility.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HvfMmioBlockRegisterProbe {
    pub name: &'static str,
    pub ipa: u64,
    pub expected_value: u64,
    pub run_attempted: bool,
    pub exit_observed: bool,
    pub handled_by_device: bool,
    pub value_injected: bool,
    pub pc_read_after_exit: bool,
    pub pc_advanced: bool,
    pub run_status: Option<i32>,
    pub exit_reason: Option<u32>,
    pub exit_syndrome: Option<u64>,
    pub exit_virtual_address: Option<u64>,
    pub exit_physical_address: Option<u64>,
    pub watchdog_cancel_status: Option<i32>,
    pub value_set_status: Option<i32>,
    pub pc_read_status: Option<i32>,
    pub pc_after_exit: Option<u64>,
    pub pc_advance_status: Option<i32>,
}
