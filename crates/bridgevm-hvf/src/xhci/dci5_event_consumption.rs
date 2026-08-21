//! Prevent a queued pointer report from overtaking the previous DCI5 event.

use super::XhciController;

impl XhciController {
    pub(crate) fn dci5_previous_event_consumed(&self) -> bool {
        let Some((interrupter, erdp_at_post)) = self.slot1_dci5_last_event else {
            return true;
        };
        self.interrupters
            .get(interrupter)
            .is_none_or(|state| state.erdp != erdp_at_post)
    }

    pub(super) fn record_dci5_event_for_consumption(&mut self) {
        let event = self.event_lifecycle_stats();
        let erdp = self.interrupters[event.last_event_interrupter].erdp;
        self.slot1_dci5_last_event = Some((event.last_event_interrupter, erdp));
    }

    pub(super) fn clear_dci5_event_consumption(&mut self) {
        self.slot1_dci5_last_event = None;
    }
}
