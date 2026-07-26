//! Depth-1, latest-wins mailbox for asynchronous scanout present.
//!
//! Today `RESOURCE_FLUSH` runs `present_3d_scanout` inline: the device thread
//! blocks on the renderer worker for the IOSurface blit and the optional CPU
//! readback, so the vCPU pays renderer latency on the critical path.
//!
//! This mailbox lets the device thread hand a present to the worker and move
//! on. The bound is strict:
//!
//!   * at most ONE present executing, and
//!   * at most ONE latest replacement retained.
//!
//! A frame arriving while one is already in flight replaces any previously
//! retained frame rather than queueing behind it -- for a display, the newest
//! frame is the only one worth showing, and an unbounded queue would grow
//! without limit under load and present stale frames.
//!
//! Ordering is enforced by epoch and resource identity, not by hope: a
//! completion is applied only if it still matches the scanout the device
//! thread cares about. Anything else is cancelled.

/// Monotonic generation of the scanout configuration. Bumped whenever the
/// scanout resource or geometry changes, so in-flight presents issued against
/// the previous configuration can be recognised and dropped.
pub(crate) type PresentEpoch = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PresentRequest {
    pub(crate) resource_id: u32,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) epoch: PresentEpoch,
}

/// Why a completed present was not applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PresentCancelReason {
    /// The scanout configuration changed while this present was in flight.
    StaleEpoch,
    /// The scanout switched to a different resource.
    ResourceChanged,
    /// The resource was destroyed while this present was in flight.
    ResourceGone,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PresentAdmission {
    /// No present was in flight; dispatch this one now.
    Dispatch(PresentRequest),
    /// A present is in flight; this frame was retained as the latest.
    Retained,
    /// A present is in flight and this frame replaced an earlier retained
    /// frame, which is dropped without ever being presented.
    ReplacedRetained,
}

#[derive(Debug, Default)]
pub(crate) struct AsyncPresentMailbox {
    in_flight: Option<PresentRequest>,
    retained: Option<PresentRequest>,
    epoch: PresentEpoch,
    dispatched: u64,
    superseded: u64,
    cancelled: u64,
}

impl AsyncPresentMailbox {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Completed presents discarded because they no longer matched the scanout.
    pub(crate) fn cancelled(&self) -> u64 {
        self.cancelled
    }

    /// Invalidate in-flight and retained work. Call when the scanout resource
    /// or geometry changes, so a present computed against the old
    /// configuration cannot be applied to the new one.
    pub(crate) fn bump_epoch(&mut self) -> PresentEpoch {
        self.epoch = self.epoch.saturating_add(1);
        // A retained frame has not been sent anywhere and can simply be
        // dropped. The in-flight one is left alone: the worker still owns its
        // buffers, so it must be allowed to finish and be rejected on
        // completion rather than abandoned mid-flight.
        if self.retained.take().is_some() {
            self.superseded = self.superseded.saturating_add(1);
        }
        self.epoch
    }

    /// Offer a frame. Never queues more than one pending frame.
    pub(crate) fn offer(&mut self, resource_id: u32, width: u32, height: u32) -> PresentAdmission {
        let request = PresentRequest {
            resource_id,
            width,
            height,
            epoch: self.epoch,
        };
        if self.in_flight.is_none() {
            self.in_flight = Some(request);
            self.dispatched = self.dispatched.saturating_add(1);
            return PresentAdmission::Dispatch(request);
        }
        let replaced = self.retained.replace(request).is_some();
        if replaced {
            self.superseded = self.superseded.saturating_add(1);
            PresentAdmission::ReplacedRetained
        } else {
            PresentAdmission::Retained
        }
    }

    /// Resolve a completed present against the current scanout.
    ///
    /// `current_resource` is the resource the scanout points at now, or `None`
    /// if it is gone. Returns `Ok(())` when the completion may be applied.
    pub(crate) fn resolve_completion(
        &self,
        completed: PresentRequest,
        current_resource: Option<u32>,
    ) -> Result<(), PresentCancelReason> {
        if completed.epoch != self.epoch {
            return Err(PresentCancelReason::StaleEpoch);
        }
        match current_resource {
            None => Err(PresentCancelReason::ResourceGone),
            Some(current) if current != completed.resource_id => {
                Err(PresentCancelReason::ResourceChanged)
            }
            Some(_) => Ok(()),
        }
    }

    /// Retire the in-flight present and return the next frame to dispatch, if
    /// one was retained while it ran.
    pub(crate) fn complete(
        &mut self,
        current_resource: Option<u32>,
    ) -> (Option<PresentCancelReason>, Option<PresentRequest>) {
        let outcome = match self.in_flight.take() {
            Some(completed) => self.resolve_completion(completed, current_resource).err(),
            None => None,
        };
        if outcome.is_some() {
            self.cancelled = self.cancelled.saturating_add(1);
        }
        let next = self.retained.take();
        if let Some(request) = next {
            // Promote the retained frame, refreshed to the live epoch so a
            // frame retained across an epoch bump is not dispatched stale.
            let promoted = PresentRequest {
                epoch: self.epoch,
                ..request
            };
            self.in_flight = Some(promoted);
            self.dispatched = self.dispatched.saturating_add(1);
            return (outcome, Some(promoted));
        }
        (outcome, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_offer_dispatches_immediately() {
        let mut mailbox = AsyncPresentMailbox::new();
        assert_eq!(
            mailbox.offer(7, 1280, 800),
            PresentAdmission::Dispatch(PresentRequest {
                resource_id: 7,
                width: 1280,
                height: 800,
                epoch: 0,
            })
        );
        assert_eq!(mailbox.dispatched, 1);
    }

    #[test]
    fn depth_stays_at_one_executing_and_one_retained() {
        let mut mailbox = AsyncPresentMailbox::new();
        mailbox.offer(7, 1280, 800);
        assert_eq!(mailbox.offer(7, 1280, 800), PresentAdmission::Retained);
        // Every further frame replaces the retained one; nothing accumulates.
        for _ in 0..100 {
            assert_eq!(
                mailbox.offer(7, 1280, 800),
                PresentAdmission::ReplacedRetained
            );
        }
        assert!(mailbox.in_flight.is_some());
        assert!(mailbox.retained.is_some());
        assert_eq!(mailbox.superseded, 100);
    }

    #[test]
    fn latest_frame_wins_over_the_retained_one() {
        let mut mailbox = AsyncPresentMailbox::new();
        mailbox.offer(7, 1280, 800);
        mailbox.offer(7, 640, 480);
        mailbox.offer(7, 1920, 1080);
        let retained = mailbox.retained.expect("a frame is retained");
        assert_eq!((retained.width, retained.height), (1920, 1080));
    }

    #[test]
    fn completion_promotes_the_retained_frame() {
        let mut mailbox = AsyncPresentMailbox::new();
        mailbox.offer(7, 1280, 800);
        mailbox.offer(7, 1920, 1080);
        let (cancel, next) = mailbox.complete(Some(7));
        assert_eq!(cancel, None);
        let next = next.expect("the retained frame is promoted");
        assert_eq!((next.width, next.height), (1920, 1080));
        assert!(mailbox.retained.is_none());
    }

    #[test]
    fn completion_with_nothing_retained_leaves_the_mailbox_idle() {
        let mut mailbox = AsyncPresentMailbox::new();
        mailbox.offer(7, 1280, 800);
        let (cancel, next) = mailbox.complete(Some(7));
        assert_eq!(cancel, None);
        assert_eq!(next, None);
        assert!(mailbox.in_flight.is_none());
    }

    #[test]
    fn stale_epoch_completion_is_cancelled() {
        let mut mailbox = AsyncPresentMailbox::new();
        mailbox.offer(7, 1280, 800);
        mailbox.bump_epoch();
        let (cancel, _) = mailbox.complete(Some(7));
        assert_eq!(cancel, Some(PresentCancelReason::StaleEpoch));
        assert_eq!(mailbox.cancelled(), 1);
    }

    #[test]
    fn completion_for_a_different_resource_is_cancelled() {
        let mut mailbox = AsyncPresentMailbox::new();
        mailbox.offer(7, 1280, 800);
        let (cancel, _) = mailbox.complete(Some(9));
        assert_eq!(cancel, Some(PresentCancelReason::ResourceChanged));
    }

    #[test]
    fn completion_for_a_destroyed_resource_is_cancelled() {
        let mut mailbox = AsyncPresentMailbox::new();
        mailbox.offer(7, 1280, 800);
        let (cancel, _) = mailbox.complete(None);
        assert_eq!(cancel, Some(PresentCancelReason::ResourceGone));
    }

    #[test]
    fn epoch_bump_drops_the_retained_frame_but_not_the_in_flight_one() {
        let mut mailbox = AsyncPresentMailbox::new();
        mailbox.offer(7, 1280, 800);
        mailbox.offer(7, 1920, 1080);
        mailbox.bump_epoch();
        // The worker still owns the in-flight frame's buffers, so it must be
        // allowed to finish and be rejected, never abandoned mid-flight.
        assert!(mailbox.in_flight.is_some());
        assert!(mailbox.retained.is_none());
    }

    #[test]
    fn a_frame_retained_across_an_epoch_bump_is_promoted_at_the_live_epoch() {
        let mut mailbox = AsyncPresentMailbox::new();
        mailbox.offer(7, 1280, 800);
        mailbox.offer(7, 1920, 1080);
        let epoch = mailbox.bump_epoch();
        mailbox.offer(7, 800, 600);
        let (_, next) = mailbox.complete(Some(7));
        let next = next.expect("the newly retained frame is promoted");
        assert_eq!(next.epoch, epoch);
        // and it now resolves cleanly rather than as a stale-epoch cancel
        assert_eq!(mailbox.resolve_completion(next, Some(7)), Ok(()));
    }

    #[test]
    fn counters_do_not_wrap() {
        let mut mailbox = AsyncPresentMailbox::new();
        mailbox.dispatched = u64::MAX;
        mailbox.superseded = u64::MAX;
        mailbox.cancelled = u64::MAX;
        mailbox.offer(1, 2, 2);
        mailbox.offer(1, 2, 2);
        mailbox.offer(1, 2, 2);
        mailbox.complete(None);
        assert_eq!(mailbox.dispatched, u64::MAX);
        assert_eq!(mailbox.superseded, u64::MAX);
        assert_eq!(mailbox.cancelled(), u64::MAX);
    }
}
