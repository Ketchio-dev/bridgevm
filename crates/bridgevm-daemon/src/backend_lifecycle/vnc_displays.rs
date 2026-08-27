//! VNC display reservations derived from daemon-owned backend metadata.

use super::*;
use anyhow::Context;
use bridgevm_qemu::vnc_display_in_command;

impl DaemonState {
    /// Read every live child's recorded command before assigning a Compatibility
    /// Mode VNC display. A missing or unreadable record is an unsafe unknown: the
    /// child may already own `:0` without having bound TCP 5900 yet, so ignoring
    /// it can hand the same display to a second QEMU. Fail closed instead.
    pub(crate) fn live_vnc_displays(&self) -> Result<Vec<u16>> {
        let mut displays = Vec::new();
        for name in self.children.keys() {
            let metadata = self
                .store
                .runner_metadata(name)
                .with_context(|| {
                    format!("failed to read runner metadata for live backend '{name}'")
                })?
                .with_context(|| format!("live backend '{name}' has no runner metadata"))?;
            if metadata.dry_run || metadata.pid.is_none() {
                anyhow::bail!(
                    "live backend '{name}' has non-live runner metadata; refusing unsafe VNC allocation"
                );
            }
            if let Some(display) = vnc_display_in_command(&metadata.command) {
                displays.push(display);
            }
        }
        Ok(displays)
    }
}
