//! Host names Windows cannot keep apart. Two host names with one Windows name
//! (case twins on a case-sensitive volume, `foo` next to `foo.` or `foo `)
//! would share a single guest file, and a name below a folder whose name ends
//! in a dot or a space never matches the name the guest stores. Such names are
//! held: none is uploaded, and no guest state under that Windows name reaches
//! the host, nor is the guest copy deleted, until a host rename or deletion
//! ends the hold. A guest edit made meanwhile is noted, so a hold that ends
//! with the host name gone fetches the edit instead of deleting it. A lone
//! `foo.` is not held: `share_win_names` pairs it with the guest's `foo`.
//! Each held name is reported once when it becomes held.

use super::*;

/// Whether a folder in `name` ends in a dot or a space, which Win32 trims.
fn folder_trimmed_by_windows(name: &str) -> bool {
    let folders = name.rsplit_once('/').map_or("", |(folders, _)| folders);
    folders.split('/').any(|part| part.ends_with(['.', ' ']))
}

impl ShareSync {
    /// Drop held names from the scan, then match case-only renames among the
    /// rest, so a record never moves to (or from) a name Windows cannot keep.
    pub(super) fn hold_windows_aliases(&mut self, actions: &mut Vec<SyncAction>) {
        // Per-component keys, as for unseen names: holding a name Windows
        // might in fact keep apart only pauses its sync; missing one loses data.
        let mut groups: HashMap<String, Vec<&String>> = HashMap::new();
        for name in self.host_files_scratch.keys() {
            let key = share_host_unseen::unseen_key(name);
            groups.entry(key).or_default().push(name);
        }
        let mut held = HashSet::new();
        let mut dropped = Vec::new();
        for (key, mut names) in groups {
            if names.len() == 1 && !folder_trimmed_by_windows(names[0]) {
                continue;
            }
            names.sort_unstable();
            let names: Vec<String> = names.into_iter().cloned().collect();
            if !self.host_aliased.contains(&key) {
                let name = names[0].clone();
                let reason = SkipReason::WindowsAlias(names.clone());
                actions.push(SyncAction::Skip { name, reason });
            }
            dropped.extend(names);
            held.insert(key);
        }
        for name in &dropped {
            self.host_files_scratch.remove(name);
        }
        self.host_aliased = held;
        self.adopt_host_renames();
    }

    /// The host name exists but the scan could not read it, or it is held as
    /// a Windows alias: its guest copy is neither deleted nor pulled back.
    pub(super) fn host_held(&self, name: &str) -> bool {
        let aliased = &self.host_aliased;
        self.host_unseen_covers(name)
            || (!aliased.is_empty() && aliased.contains(&share_host_unseen::unseen_key(name)))
    }

    /// A refused PUT is forgotten once its host file changes, but not while the
    /// name is held: the guest copy may be truncated and must never be noted
    /// as a guest edit.
    pub(super) fn retain_put_rejected(&mut self) {
        let mut rejected = std::mem::take(&mut self.put_rejected);
        let host = &self.host_files_scratch;
        rejected.retain(|name, stat| host.get(name) == Some(stat) || self.host_held(name));
        self.put_rejected = rejected;
    }
}
