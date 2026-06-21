use std::collections::VecDeque;

pub const RECENT_REQUEST_TRACE_LIMIT: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VirtioBlockRequestTrace {
    pub sequence: u64,
    pub request_type: u32,
    pub sector: u64,
    pub data_len: u32,
    pub status: u8,
}

#[derive(Debug, Default)]
pub struct RecentVirtioBlockRequests {
    entries: VecDeque<VirtioBlockRequestTrace>,
}

impl RecentVirtioBlockRequests {
    pub fn record(&mut self, entry: VirtioBlockRequestTrace) {
        if self.entries.len() == RECENT_REQUEST_TRACE_LIMIT {
            self.entries.pop_front();
        }
        self.entries.push_back(entry);
    }

    pub fn snapshot(&self) -> Vec<VirtioBlockRequestTrace> {
        self.entries.iter().copied().collect()
    }
}
