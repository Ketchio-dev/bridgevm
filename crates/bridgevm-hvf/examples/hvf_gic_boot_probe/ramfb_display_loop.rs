//! The ramfb export thread's loop. It exports when the app asks for a frame
//! and keeps its deadline only as the fallback for when nobody asks
//! (display-export-20261023, F1 B2):
//!
//! - The app's display tick posts a request into the display file's header.
//!   The thread copies and compares the guest frame at once and publishes it
//!   if it changed: at most one frame per request, however many came since it
//!   last looked. The app shows it on its next tick, so the export follows
//!   the display instead of beating against it.
//! - A request re-arms the deadline a whole period ahead, so while an app
//!   asks at its refresh rate the deadline never fires. Without requests (no
//!   app, an older app, a hidden window, a file that cannot be watched) the
//!   thread exports on its deadline grid as before.
//! - The `ramfb display timing:` record keeps its grammar. A request tick is
//!   late by the time since the app stamped the request, a deadline tick by
//!   the time since its deadline, as before.

use std::fs::File;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use super::super::FramePublisher;
use super::timing::{self, RamfbDisplayTiming};
use super::wake::{uptime_ns, ExportWake, Wake};
use super::ConfigSource;
use bridgevm_hvf::fwcfg::GuestMemoryMut;

/// Between requests deadline-paced: a tick that overruns resynchronises
/// instead of bursting.
pub(super) fn export_loop(
    mut publisher: FramePublisher,
    mut source: impl ConfigSource,
    mem: &dyn GuestMemoryMut,
    interval: Duration,
    wake: &ExportWake,
    counts: &Mutex<RamfbDisplayTiming>,
) {
    let started = Instant::now();
    let (mut next, mut polls, mut published) = (started, 0u64, 0u64);
    let mut slowest = Duration::ZERO;
    let mut requests = FrameRequests::default();
    loop {
        let (deadline, requested) = match wake.wait(next) {
            Wake::Stop => break,
            Wake::Deadline => (next, false),
            Wake::Written => match requests.take(&publisher) {
                Some(posted) => (posted, true),
                None => continue,
            },
        };
        let tick = Instant::now();
        let fresh = publisher.publish_changed(source.poll(), mem);
        published += u64::from(fresh);
        polls += 1;
        let done = Instant::now();
        timing::lock(counts).note_tick(deadline, tick, fresh.then_some(done));
        slowest = slowest.max(done - tick);
        next = if requested {
            done + interval
        } else {
            (next + interval).max(done)
        };
        if fresh {
            requests.watch(&mut publisher, wake);
        }
    }
    let seconds = started.elapsed().as_secs_f64().max(f64::EPSILON);
    let (poll_hz, publish_hz) = (polls as f64 / seconds, published as f64 / seconds);
    println!(
        "ramfb display export: stopped polls={polls} published={published} \
         seconds={seconds:.1} poll_hz={poll_hz:.1} publish_hz={publish_hz:.1} \
         slowest_tick_us={}",
        slowest.as_micros()
    );
}

/// The app's frame requests as the export thread sees them.
#[derive(Default)]
struct FrameRequests {
    /// Held open while watched: closing it would end the watch.
    watched: Option<File>,
    tried: bool,
    last: u64,
}

impl FrameRequests {
    /// After the first published frame: watch the file, then mark it as
    /// answering requests, so the app asks only once it will be heard.
    fn watch(&mut self, publisher: &mut FramePublisher, wake: &ExportWake) {
        if std::mem::replace(&mut self.tried, true) {
            return;
        }
        let target = &mut publisher.target;
        let file = File::open(target.path())
            .ok()
            .filter(|file| wake.watch(file));
        match file {
            Some(file) if target.accept_frame_requests() => {
                self.last = target.frame_request().unwrap_or(0);
                self.watched = Some(file);
                println!("ramfb display export: answering frame requests");
            }
            _ => eprintln!("ramfb display export: frame requests unanswered, deadline only"),
        }
    }

    /// When the request word changed since the last look: the instant the
    /// app posted it, from the `CLOCK_UPTIME_RAW` time it carries.
    fn take(&mut self, publisher: &FramePublisher) -> Option<Instant> {
        let request = publisher.target.frame_request()?;
        if self.watched.is_none() || request == self.last {
            return None;
        }
        self.last = request;
        let age = Duration::from_nanos(uptime_ns().saturating_sub(request));
        let now = Instant::now();
        Some(now.checked_sub(age).unwrap_or(now))
    }
}
