//! Shared-folder session state and one-time root identity binding.

use super::*;
#[path = "share_state_destination.rs"]
mod destination;
#[path = "share_root_feedback.rs"]
mod feedback;
#[path = "share_root.rs"]
mod root;
#[path = "share_guest_listing_names.rs"]
mod guest_listing_names;

pub(in crate::agent_console) struct ShareState {
    pub(in crate::agent_console) engine: ShareSync,
    pub(in crate::agent_console) host_dir: PathBuf,
    pub(in crate::agent_console) guest_dir: String,
    pub(in crate::agent_console) interval: Duration,
    pub(in crate::agent_console) last_poll: Option<Instant>,
    pub(in crate::agent_console) host_skip_seen: HashSet<(String, u128, HostSkipKind)>,
    pub(in crate::agent_console) guest_ls_scratch: Vec<LsEntry>,
    pub(in crate::agent_console) host_scan_scratch: Vec<HostFile>,
    pub(in crate::agent_console) host_scan_work: Vec<HostFile>,
    root: root::ShareRoot,
    root_blocked: bool,
    /// A missing guest folder is created at most once per session.
    pub(in crate::agent_console) guest_root_create_sent: bool,
    /// Windows key of a share key -> the name the last listing showed for it,
    /// where that is not the key's own guest form (share_guest_listing_names.rs).
    pub(in crate::agent_console) guest_names: std::collections::HashMap<String, String>,
    /// What the last host scan left to do, and when it ran (share_host_change.rs).
    pub(in crate::agent_console) host_actions: VecDeque<SyncAction>,
    pub(in crate::agent_console) host_actions_at: Option<Instant>,
    #[cfg(test)]
    pub(in crate::agent_console) host_scans: usize,
}

impl ShareState {
    pub(in crate::agent_console) fn new(
        engine: ShareSync,
        host_dir: PathBuf,
        guest_dir: String,
        interval: Duration,
    ) -> Self {
        let root = root::ShareRoot::new(&host_dir);
        Self {
            engine,
            host_dir,
            guest_dir,
            interval,
            last_poll: None,
            host_skip_seen: HashSet::new(),
            guest_ls_scratch: Vec::new(),
            host_scan_scratch: Vec::new(),
            host_scan_work: Vec::new(),
            root,
            root_blocked: false,
            guest_root_create_sent: false,
            guest_names: std::collections::HashMap::new(),
            host_actions: VecDeque::new(),
            host_actions_at: None,
            #[cfg(test)]
            host_scans: 0,
        }
    }
}
