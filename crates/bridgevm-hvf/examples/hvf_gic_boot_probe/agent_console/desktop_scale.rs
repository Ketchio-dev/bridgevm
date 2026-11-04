//! Sets the Windows display scale of the virtio-gpu display through the guest
//! agent, for `RESIZE WxH@P`.

use std::sync::atomic::{AtomicU32, Ordering};

/// The scales Windows offers, in percent.
pub const DESKTOP_SCALES: [u32; 12] = [100, 125, 150, 175, 200, 225, 250, 300, 350, 400, 450, 500];

/// Set while the latest requested scale has not been handed to the agent.
const PENDING: u32 = 1 << 31;

fn requested() -> &'static AtomicU32 {
    #[cfg(not(test))]
    {
        static REQUESTED: AtomicU32 = AtomicU32::new(0);
        &REQUESTED
    }
    // Tests drive harnesses on their own threads; keep their requests apart.
    #[cfg(test)]
    {
        thread_local!(static REQUESTED: &'static AtomicU32 = Box::leak(Box::new(AtomicU32::new(0))));
        REQUESTED.with(|slot| *slot)
    }
}

/// Ask the agent to set the display scale to `percent`, one of
/// [`DESKTOP_SCALES`]; the latest wins.
pub fn request_desktop_scale(percent: u32) {
    debug_assert!(DESKTOP_SCALES.contains(&percent));
    requested().store(percent | PENDING, Ordering::Relaxed);
}
