//! Pair shutdown notification with the secondary owner's wait predicate lock.

use crate::*;

impl VcpuControl {
    pub(crate) fn notify_shutdown(&self) {
        // Shutdown is already published. Wait until an Off owner atomically
        // releases this mutex into Condvar::wait, so this wake cannot be lost.
        // Notification does not interpret or repair poisoned PSCI state. Keep
        // waking other owners; their existing owner/join errors remain fatal.
        let _state = self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        self.condvar.notify_all();
    }
}
