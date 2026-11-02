//! Blocking startup access and non-waiting polling share one pinned-file path.

use super::{checked_access, InputControlFile};
use std::fs::File;

impl InputControlFile {
    pub(crate) fn with_exclusive<T>(&mut self, action: impl FnOnce(&mut File) -> T) -> Option<T> {
        self.with_lock(false, action)
    }

    /// A busy writer defers polling without invoking the reader's state changes.
    pub(crate) fn try_with_exclusive<T>(
        &mut self,
        action: impl FnOnce(&mut File) -> T,
    ) -> Option<T> {
        self.with_lock(true, action)
    }

    fn with_lock<T>(
        &mut self,
        nonblocking: bool,
        action: impl FnOnce(&mut File) -> T,
    ) -> Option<T> {
        if self.file.is_none() {
            self.file = checked_access::open(&self.path);
        }
        checked_access::with_exclusive(&self.path, self.file.as_mut()?, nonblocking, action)
    }
}
