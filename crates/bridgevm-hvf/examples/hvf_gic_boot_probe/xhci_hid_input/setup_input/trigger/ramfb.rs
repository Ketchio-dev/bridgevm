use std::time::Instant;

use bridgevm_hvf::fwcfg::GuestMemoryMut;
use bridgevm_hvf::platform_virt::VirtPlatform;
use bridgevm_hvf::xhci::{SetupInputAction, XhciSetupInputQueueError};

use super::XhciSetupInputTrigger;

impl XhciSetupInputTrigger {
    #[cfg(test)]
    pub(crate) fn maybe_fire_with_ramfb_checkpoints<F>(
        &mut self,
        platform: &mut VirtPlatform,
        mut emit_checkpoint: F,
    ) where
        F: FnMut(&str),
    {
        self.maybe_fire_with_ramfb_checkpoints_at(platform, Instant::now(), &mut emit_checkpoint);
    }

    #[cfg(test)]
    pub(crate) fn maybe_fire_with_ramfb_checkpoints_at<F>(
        &mut self,
        platform: &mut VirtPlatform,
        now: Instant,
        mut emit_checkpoint: F,
    ) -> bool
    where
        F: FnMut(&str),
    {
        let mut context = ();
        self.maybe_fire_with_ramfb_checkpoint_context_at(
            platform,
            &mut context,
            now,
            |platform, _context, actions| platform.queue_xhci_setup_input_actions(actions),
            |label, _context| emit_checkpoint(label),
        )
    }

    pub(crate) fn maybe_fire_with_mem_and_ramfb_checkpoints_at<F>(
        &mut self,
        platform: &mut VirtPlatform,
        mem: &mut dyn GuestMemoryMut,
        now: Instant,
        mut emit_checkpoint: F,
    ) -> bool
    where
        F: FnMut(&str, &dyn GuestMemoryMut),
    {
        self.maybe_fire_with_ramfb_checkpoint_context_at(
            platform,
            mem,
            now,
            |platform, mem, actions| platform.queue_xhci_setup_input_actions_with_mem(actions, mem),
            |label, mem| emit_checkpoint(label, mem),
        )
    }

    fn maybe_fire_with_ramfb_checkpoint_context_at<C, Q, F>(
        &mut self,
        platform: &mut VirtPlatform,
        context: &mut C,
        now: Instant,
        queue: Q,
        mut emit_checkpoint: F,
    ) -> bool
    where
        C: ?Sized,
        Q: FnOnce(
            &mut VirtPlatform,
            &mut C,
            &[SetupInputAction],
        ) -> Result<(), XhciSetupInputQueueError>,
        F: FnMut(&str, &C),
    {
        let can_checkpoint = self.ready_for_ramfb_checkpoints_at(platform, now);
        if can_checkpoint {
            emit_checkpoint("setup-input-before", context);
        }
        let fired = self.maybe_fire_by_at(platform, now, |platform, actions| {
            queue(platform, context, actions)
        });
        if fired {
            self.fired_at = Some(now);
            if can_checkpoint {
                emit_checkpoint("setup-input-after", context);
            }
        }
        self.emit_due_ramfb_delay_checkpoints(now, &mut |label| emit_checkpoint(label, context));
        fired
    }

    fn emit_due_ramfb_delay_checkpoints<F>(&mut self, now: Instant, emit_checkpoint: &mut F)
    where
        F: FnMut(&str),
    {
        let Some(fired_at) = self.fired_at else {
            return;
        };
        let Some(elapsed) = now.checked_duration_since(fired_at) else {
            return;
        };
        for checkpoint in &mut self.ramfb_delay_checkpoints {
            if !checkpoint.emitted && elapsed >= checkpoint.delay {
                checkpoint.emitted = true;
                emit_checkpoint(&checkpoint.label);
            }
        }
    }

    fn ready_for_ramfb_checkpoints_at(&mut self, platform: &VirtPlatform, now: Instant) -> bool {
        if !self.fire_delay_elapsed_at(platform, now) {
            return false;
        }
        let stats = platform.xhci_setup_input_report_stats();
        let emitted_reports = stats
            .emitted_key_reports
            .saturating_add(stats.emitted_release_reports);
        stats.queued_reports == emitted_reports
    }
}
