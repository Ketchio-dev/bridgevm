//! Listed guest names become share keys (share_guest_names.rs).

use super::*;
use std::borrow::Cow;

impl ShareState {
    /// Keys for the listing in `guest_ls_scratch`. A guest name that is not
    /// its key's own guest form (a decomposed name a Windows program wrote) is
    /// remembered, so requests for that key reach the file. When both a
    /// decomposed and a composed copy are listed, the composed one is the
    /// file and the other is left alone.
    pub(in crate::agent_console) fn key_guest_listing(&mut self) {
        self.guest_names.clear();
        let mut listed = Vec::new();
        for (i, entry) in self.guest_ls_scratch.iter_mut().enumerate() {
            if let Cow::Owned(key) = share_sync::key_from_guest(&entry.name) {
                let guest = std::mem::replace(&mut entry.name, key);
                if !guest.chars().eq(entry.name.chars().map(share_sync::guest_char)) {
                    listed.push((i, guest));
                }
            }
        }
        if listed.is_empty() {
            return;
        }
        let mut other_form = vec![false; self.guest_ls_scratch.len()];
        for (i, _) in &listed {
            other_form[*i] = true;
        }
        let mut taken: HashSet<String> = self
            .guest_ls_scratch
            .iter()
            .zip(&other_form)
            .filter(|(_, other)| !**other)
            .map(|(entry, _)| entry.name.clone())
            .collect();
        let mut keep = vec![true; self.guest_ls_scratch.len()];
        for (i, guest) in listed {
            let key = &self.guest_ls_scratch[i].name;
            if taken.insert(key.clone()) {
                self.guest_names.insert(key.clone(), guest);
            } else {
                keep[i] = false;
            }
        }
        let mut keep = keep.into_iter();
        self.guest_ls_scratch.retain(|_| keep.next().unwrap_or(true));
    }
}
