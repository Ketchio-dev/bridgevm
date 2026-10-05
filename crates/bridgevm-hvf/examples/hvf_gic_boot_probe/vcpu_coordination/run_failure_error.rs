//! Context retained by the primary owner when a secondary fails.
use crate::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PrimaryRunError {
    Hypervisor(HvReturn),
    Secondary(u64),
}

impl std::fmt::Display for PrimaryRunError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Hypervisor(status) => write!(f, "hv_vcpu_run error {status:#x}"),
            Self::Secondary(index) => write!(f, "secondary vCPU{index} fatal run error"),
        }
    }
}
