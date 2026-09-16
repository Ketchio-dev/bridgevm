//! Shared readiness deadline, delegated to keep lifecycle facts separate.
#[path = "controlled_swtpm_readiness.rs"]
mod readiness;
pub(crate) use readiness::wait_for_sockets;
