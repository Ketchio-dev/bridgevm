//! Command/completion trace records, the ring buffer, and BRIDGEVM_TRACE_NVME formatting.

use super::*;
use std::sync::OnceLock;

pub(crate) fn nvme_trace_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        matches!(
            std::env::var("BRIDGEVM_TRACE_NVME").ok().as_deref(),
            Some("1") | Some("true") | Some("TRUE") | Some("yes") | Some("YES")
        )
    })
}

pub(crate) fn identify_cns_name(cns: u32) -> &'static str {
    match cns {
        IDENTIFY_CNS_NAMESPACE => "namespace",
        IDENTIFY_CNS_CONTROLLER => "controller",
        IDENTIFY_CNS_ACTIVE_NAMESPACE_LIST => "active-ns-list",
        IDENTIFY_CNS_NAMESPACE_DESCRIPTOR_LIST => "ns-desc-list",
        IDENTIFY_CNS_COMMAND_SET_CONTROLLER => "command-set-controller",
        _ => "unknown",
    }
}

pub(crate) fn hex_preview(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        use std::fmt::Write;
        let _ = write!(&mut out, "{b:02x}");
    }
    out
}

/// Number of recent commands retained for live bring-up diagnostics.
pub const COMMAND_TRACE_CAPACITY: usize = 256;

/// Completion routing metadata captured with a processed NVMe command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NvmeCompletionTrace {
    pub cqid: u16,
    pub vector: u16,
}

/// A recent NVMe submission entry processed by the controller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NvmeCommandTrace {
    pub sqid: u16,
    pub cqid: u16,
    pub sq_head: u16,
    pub sq_tail: u16,
    pub sq_entry_gpa: u64,
    pub opcode: u8,
    pub command_id: u16,
    pub nsid: u32,
    pub prp1: u64,
    pub prp2: u64,
    pub cdw10: u32,
    pub cdw11: u32,
    pub cdw12: u32,
    pub cdw13: u32,
    pub cdw14: u32,
    pub cdw15: u32,
    pub status: u16,
    pub completion_posted: bool,
    pub completion: Option<NvmeCompletionTrace>,
}

impl NvmeController {
    /// Snapshot recent commands processed by the queue engine, oldest first.
    pub fn recent_command_trace(&self) -> Vec<NvmeCommandTrace> {
        self.command_trace.iter().copied().collect()
    }

    pub(crate) fn record_command_trace(&mut self, trace: NvmeCommandTrace) {
        if self.command_trace.len() == COMMAND_TRACE_CAPACITY {
            self.command_trace.pop_front();
        }
        self.command_trace.push_back(trace);
    }
}
