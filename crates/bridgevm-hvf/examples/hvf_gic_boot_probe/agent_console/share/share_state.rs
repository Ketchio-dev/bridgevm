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
#[path = "share_host_watch.rs"]
pub(in crate::agent_console) mod host_watch;
#[path = "share_sync_file.rs"]
pub(in crate::agent_console) mod sync_file;
use host_watch::{HostWalk, HostWatch};

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
    /// Wall clock at that scan's start, against folder modification times.
    pub(in crate::agent_console) host_actions_scanned: std::time::SystemTime,
    /// FSEvents on `host_dir` and the last walk it vouches for (share_host_watch.rs).
    pub(in crate::agent_console) host_watch: HostWatch,
    host_walk: Option<HostWalk>,
    /// The large copy under way, either way (share_big.rs).
    pub(in crate::agent_console) big: Option<BigTransfer>,
    /// Large guest files reported as skipped for the Mac's free space: name and listed time.
    pub(in crate::agent_console) low_space_seen: HashSet<(String, Option<String>)>,
    /// Synced versions and PUT marks beside the VM's UEFI vars (share_sync_file.rs).
    pub(in crate::agent_console) sync_file: Option<sync_file::SyncStateFile>,
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
            host_actions_scanned: std::time::UNIX_EPOCH,
            host_watch: HostWatch::Off,
            host_walk: None,
            big: None,
            low_space_seen: HashSet::new(),
            sync_file: None,
            #[cfg(test)]
            host_scans: 0,
        }
    }

    /// Keep the synced state beside the VM's UEFI vars at `vars`, and take what it holds.
    pub(in crate::agent_console) fn attach_sync_state(&mut self, vars: &Path) {
        let (file, files, marks) = sync_file::SyncStateFile::open(vars, &self.host_dir);
        self.engine.load_synced(files, marks);
        self.sync_file = Some(file);
    }

    /// Before a PUT of `name` goes out: its mark on disk first. A mark that cannot be written
    /// turns the state off; the share goes on as without one.
    pub(in crate::agent_console) fn mark_put(&mut self, name: &str) {
        if !self.engine.mark_put(name) {
            return;
        }
        let Some(file) = self.sync_file.as_mut() else {
            return;
        };
        if let Err(error) = file.write_marks(self.engine.put_marks()) {
            println!("BVAGENT SHARE sync-state off {error}");
            if let Some(file) = self.sync_file.take() {
                file.remove();
            }
        }
    }

    pub(in crate::agent_console) fn flush_sync_state(&mut self, now: Instant) {
        if let Some(file) = self.sync_file.as_mut() {
            file.flush(&mut self.engine, now);
        }
    }
}

impl Drop for ShareState {
    fn drop(&mut self) {
        if let Some(file) = self.sync_file.as_mut() {
            file.finish(&mut self.engine);
        }
    }
}
