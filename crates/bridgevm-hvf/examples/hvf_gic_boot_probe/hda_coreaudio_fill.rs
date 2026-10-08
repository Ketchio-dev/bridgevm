//! Non-waiting callback fill and continuity recording.

use super::hda_coreaudio_continuity::CallbackFill;
use super::hda_coreaudio_stats::Shared;
use std::sync::TryLockError;

/// Fill one silent callback buffer from the ring without waiting, and record
/// what it received. Runs on CoreAudio's real-time callback thread.
pub(super) fn fill_and_record(destination: &mut [u8], shared: &Shared) {
    let capacity_bytes = destination.len();
    let mut ring = match shared.ring.try_lock() {
        Ok(ring) => ring,
        Err(TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
        Err(TryLockError::WouldBlock) => {
            return shared
                .continuity
                .record(CallbackFill::Contended { capacity_bytes });
        }
    };
    let stream_idle = shared.continuity.stream_idle();
    let pcm_bytes = shared.prefill.drain(&mut ring, destination, stream_idle);
    drop(ring);
    let fill = CallbackFill::Drained {
        pcm_bytes,
        capacity_bytes,
        stream_idle,
    };
    shared.continuity.record(fill);
}
