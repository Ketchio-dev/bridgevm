//! Split out of run_loop_render.rs: blocker list.

use super::*;

impl WindowsArmUefiFirmwareRunLoopProbe {
    pub(crate) fn render_blockers(&self, output: &mut String) {
        if self.blockers.is_empty() {
            output.push_str("Blockers: none\n");
        } else {
            output.push_str("Blockers:\n");
            for blocker in &self.blockers {
                output.push_str(&format!("- {blocker}\n"));
            }
        }
    }
}
