//! Host names the latest scan found but could not read: over the size cap, a
//! symlink or special file, or a directory that could not be listed. They still
//! exist, so their guest copies are neither deleted nor pulled back over them
//! until a scan can read them again.

use super::*;

/// Windows key per component, so the guest's spelling of an unseen name, or of
/// a name below an unseen directory, still matches it.
fn unseen_key(name: &str) -> String {
    let mut key = String::with_capacity(name.len());
    for part in name.split('/').filter(|part| !part.is_empty()) {
        if !key.is_empty() {
            key.push('/');
        }
        key.push_str(&share_win_names::windows_key(part));
    }
    key
}

impl ShareSync {
    /// Replace the unseen names with those reported by the latest host scan.
    pub fn set_host_unseen(&mut self, names: impl IntoIterator<Item = String>) {
        self.host_unseen.clear();
        self.host_unseen
            .extend(names.into_iter().map(|name| unseen_key(&name)));
    }

    /// The name is unseen itself or lies below an unseen directory.
    pub(super) fn host_unseen_covers(&self, name: &str) -> bool {
        if self.host_unseen.is_empty() {
            return false;
        }
        let key = unseen_key(name);
        let mut parents = key.match_indices('/').map(|(end, _)| &key[..end]);
        self.host_unseen.contains(&key) || parents.any(|dir| self.host_unseen.contains(dir))
    }

    /// Windows keys, because the guest may store a held host name in another case.
    pub(super) fn pull_held_keys(&self) -> HashSet<String> {
        (self.pending_host_changed.iter())
            .chain(self.put_rejected.keys())
            .map(|name| share_win_names::windows_key(name))
            .collect()
    }

    /// The host version is not on the guest yet (queued, or refused after the
    /// guest may have truncated its copy), or the host file exists but could
    /// not be read: never pull the guest copy back over it.
    pub(super) fn pull_held(&self, held: &HashSet<String>, name: &str) -> bool {
        (!held.is_empty() && held.contains(&share_win_names::windows_key(name)))
            || self.host_unseen_covers(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unseen_names_cover_themselves_and_their_subtree_only() {
        let mut sync = ShareSync::new(512);
        assert!(!sync.host_unseen_covers("big.bin"));
        sync.set_host_unseen(["big.bin".to_string(), "sub.".to_string()]);
        for covered in [
            "big.bin",
            "BIG.BIN",
            "sub",
            "sub/x.txt",
            "SUB/deep/y.txt",
            "sub./x",
        ] {
            assert!(sync.host_unseen_covers(covered), "{covered}");
        }
        for other in ["big.bin2", "subway.txt", "sub2/x.txt", "x/sub/y.txt"] {
            assert!(!sync.host_unseen_covers(other), "{other}");
        }
        sync.set_host_unseen(Vec::new());
        assert!(!sync.host_unseen_covers("big.bin"));
    }
}
