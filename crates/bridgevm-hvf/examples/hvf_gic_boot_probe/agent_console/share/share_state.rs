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
#[path = "share_listing_pause.rs"]
mod listing_pause;
#[path = "share_listing_same.rs"]
pub(in crate::agent_console) mod listing_same;
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
    /// The latest guest listing was refused for its size: the share is paused (share_listing_pause.rs).
    listing_too_large: bool,
    /// The last applied listing, for an LSSAME reply (share_listing_same.rs).
    pub(in crate::agent_console) listing_same: listing_same::ListingSame,
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
    /// Guest files reported as skipped for the Mac's free space (a large file's precheck, or a
    /// write the Mac refused with ENOSPC or EDQUOT): name and listed time.
    pub(in crate::agent_console) low_space_seen: HashSet<(String, Option<String>)>,
    /// Synced versions and PUT marks beside the VM's UEFI vars (share_sync_file.rs).
    pub(in crate::agent_console) sync_file: Option<sync_file::SyncStateFile>,
    /// The marks number the next PUT waits for, and since when (share_sync_file.rs).
    marks_due: Option<(u64, Instant)>,
    /// A PUT held until its marks are on disk.
    pub(in crate::agent_console) held_put: Option<ServiceReq>,
    /// A state turned off whose files are not removed yet: no PUT goes out until they are.
    sync_off: Option<sync_file::SyncStateFile>,
    sync_stuck_told: bool,
    #[cfg(test)]
    pub(in crate::agent_console) host_scans: usize,
}

/// A PUT waits at most this long for its marks; then the state turns off and the PUT goes.
const MARKS_WAIT: Duration = Duration::from_secs(10);

impl ShareState {
    pub(in crate::agent_console) fn new(
        engine: ShareSync,
        host_dir: PathBuf,
        guest_dir: String,
        interval: Duration,
    ) -> Self {
        let root = root::ShareRoot::new(&host_dir);
        let mut share = Self {
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
            listing_too_large: false,
            listing_same: listing_same::ListingSame::default(),
            guest_names: std::collections::HashMap::new(),
            host_actions: VecDeque::new(),
            host_actions_at: None,
            host_actions_scanned: std::time::UNIX_EPOCH,
            host_watch: HostWatch::Off,
            host_walk: None,
            big: None,
            low_space_seen: HashSet::new(),
            sync_file: None,
            marks_due: None,
            held_put: None,
            sync_off: None,
            sync_stuck_told: false,
            #[cfg(test)]
            host_scans: 0,
        };
        // The runtime's first word on its root, so the app's notice from Start (a folder missing
        // then) settles either way: `blocked` now (the first scan that finds it prints `ready`), or
        // `ready` for a folder that came back between the app's check and this.
        let blocked = !share.root.matches(&share.host_dir);
        share.root_blocked = !blocked;
        share.report_root_state(blocked);
        share
    }

    /// Keep the synced state beside the VM's UEFI vars at `vars`, and take what it holds.
    pub(in crate::agent_console) fn attach_sync_state(&mut self, vars: &Path) {
        let (file, files, marks, unremoved) = sync_file::SyncStateFile::open(vars, &self.host_dir);
        self.engine.load_synced(files, marks);
        self.sync_file = Some(file);
        // A state it could not take whole and could not remove: off, and PUTs wait until it is
        // gone (release_held_put).
        if let Some(error) = unremoved {
            self.sync_state_off(&format!("an unusable state could not be removed: {error}"));
        }
    }

    /// Before a PUT of `name` goes out: its mark on disk first. The state thread writes it, and
    /// the PUT waits for it (`after_marks`), so vCPU 0 does not wait for the barrier (1.6-4.5 ms
    /// measured in a burst, on top of the scan's own tick). The same write also marks every
    /// other file this scan pass may still send (they settle if it does not), so a pass of many
    /// PUTs writes once (0.9 ms each when every PUT wrote its own). A mark that cannot be
    /// written turns the state off; the share goes on as without one.
    pub(in crate::agent_console) fn mark_put(&mut self, name: &str, now: Instant) {
        self.engine.mark_put(name);
        let mut names = vec![share_sync::normalize_rel(name)];
        if self.sync_file.is_some() {
            for action in &self.host_actions {
                // PUTs wait for the first listing, so its decisions are made before any of these.
                if let SyncAction::Get { name } = action {
                    if self.engine.mark_ahead(name, now) {
                        names.push(share_sync::normalize_rel(name));
                    }
                }
            }
        }
        let Some(file) = self.sync_file.as_mut() else {
            return;
        };
        if let Some(seq) = file.ask_marks(names.iter().map(String::as_str)) {
            let since = self.marks_due.map_or(now, |(_, since)| since);
            self.marks_due = Some((seq, since));
        }
    }

    /// `req`, a PUT, to send now, or None: held until its marks are on disk (`release_held_put`).
    pub(in crate::agent_console) fn after_marks(&mut self, req: ServiceReq) -> Option<ServiceReq> {
        if self.marks_due.is_none() && self.sync_off.is_none() {
            return Some(req);
        }
        debug_assert!(self.held_put.is_none());
        self.held_put = Some(req);
        None
    }

    /// The held PUT once its marks are on disk. When a write failed, the state thread is gone or
    /// the write takes longer than MARKS_WAIT, the state turns off, and the PUT goes once its
    /// files are removed (the share then works as without a state). Not before: a PUT without
    /// its mark, cut short, would leave a guest copy the stale state takes for a guest edit.
    pub(in crate::agent_console) fn release_held_put(&mut self, now: Instant) -> Option<ServiceReq> {
        self.held_put.as_ref()?;
        if let Some((seq, since)) = self.marks_due {
            match self.sync_file.as_ref().and_then(|file| file.marks_durable(seq)) {
                Some(true) => self.marks_due = None,
                Some(false) if now.duration_since(since) < MARKS_WAIT => return None,
                Some(false) => self.sync_state_off("a mark took too long to write"),
                None => self.sync_state_off("a mark could not be written"),
            }
        }
        if !self.invalidate_sync_state() {
            return None;
        }
        self.held_put.take()
    }

    /// A state that is off but still on disk: PUTs wait, while the listing and the copies from
    /// Windows go on.
    pub(in crate::agent_console) fn sync_state_stuck(&self) -> bool {
        self.sync_off.is_some()
    }

    /// No state that is off is left on disk (tried again on every tick a PUT waits: an unlink,
    /// on this failure path only). A folder that stays unwritable keeps the PUTs held, which
    /// is the safe side.
    fn invalidate_sync_state(&mut self) -> bool {
        let Some(file) = self.sync_off.as_mut() else {
            return true;
        };
        match file.invalidate() {
            Ok(true) => {
                self.sync_off = None;
                true
            }
            Ok(false) => false,
            Err(error) => {
                if !self.sync_stuck_told {
                    println!("BVAGENT SHARE sync-state stuck {error}");
                    self.sync_stuck_told = true;
                }
                false
            }
        }
    }

    /// Before a fetched copy of `name` is published here: marked on disk, settling from `now`.
    /// The publish goes on when the mark fails (the state turns off): unlike a PUT it needs no
    /// wait, because the versions on disk predate the copy it publishes, so a later session that
    /// still found them would see the Mac copy differ from its last synced version: the Mac wins.
    pub(in crate::agent_console) fn mark_fetched(&mut self, name: &str, now: Instant) {
        self.engine.mark_settling(name, now);
        self.mark_on_disk(&[share_sync::normalize_rel(name)]);
    }

    fn mark_on_disk(&mut self, names: &[String]) {
        let Some(file) = self.sync_file.as_ref() else {
            return;
        };
        if let Err(error) = file.mark(names.iter().map(String::as_str)) {
            self.sync_state_off(&error.to_string());
        }
    }

    /// The marks writer for a thread that publishes a fetched copy (share_stage_writer.rs).
    pub(in crate::agent_console) fn sync_marks(&self) -> Option<sync_file::SharedMarks> {
        self.sync_file.as_ref().map(sync_file::SyncStateFile::marks)
    }

    fn sync_state_off(&mut self, why: &str) {
        println!("BVAGENT SHARE sync-state off {why}");
        self.marks_due = None;
        if let Some(mut file) = self.sync_file.take() {
            file.turn_off();
            self.sync_off = Some(file);
            self.invalidate_sync_state();
        }
    }

    pub(in crate::agent_console) fn flush_sync_state(&mut self, now: Instant) {
        let Some(file) = self.sync_file.as_mut() else {
            return;
        };
        if !file.flush(&mut self.engine, now) {
            self.sync_state_off("a mark could not be written");
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
