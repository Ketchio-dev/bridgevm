//! What the ramfb export thread waits for: a stop, a frame request from the
//! app's display tick, or the fallback deadline, whichever comes first.
//!
//! A request is the app's `pwrite` into the display file's header
//! (`bridgevm_hvf::display_fb::requests`). The thread sees it as a kqueue
//! vnode write, which wakes it like any other event instead of through a
//! timer: on this host every timed wait wakes milliseconds late, at default
//! and at user-interactive QoS alike (display-export-20261023). Without a
//! kqueue the wait is the plain timed one of `StopSignal`.

use std::fs::File;
use std::io::ErrorKind;
use std::mem::MaybeUninit;
use std::os::fd::AsRawFd;
use std::ptr::{null, null_mut};
use std::time::Instant;

use super::stop::StopSignal;

pub(super) enum Wake {
    Stop,
    Deadline,
    /// The watched file was written; the caller checks for a new request.
    Written,
}

#[derive(Default)]
pub(super) struct ExportWake {
    stop: StopSignal,
    /// None: timed waits only (an inert thread, or no kqueue).
    queue: Option<i32>,
}

impl ExportWake {
    /// A wait that can also watch a file; timed only when kqueue fails.
    pub(super) fn new() -> Self {
        let queue = unsafe { libc::kqueue() };
        let mut wake = Self {
            stop: StopSignal::default(),
            queue: (queue >= 0).then_some(queue),
        };
        // The stop is user event 0 on the same queue.
        let flags = libc::EV_ADD | libc::EV_CLEAR;
        if queue >= 0 && wake.change(libc::EVFILT_USER, 0, flags, 0) != 0 {
            unsafe { libc::close(queue) };
            wake.queue = None;
        }
        if wake.queue.is_none() {
            eprintln!("ramfb display export: no kqueue, frame requests unanswered");
        }
        wake
    }

    pub(super) fn stop(&self) {
        self.stop.stop();
        self.change(libc::EVFILT_USER, 0, 0, libc::NOTE_TRIGGER);
    }

    /// Wake on every write to `file`; false when it cannot be watched.
    pub(super) fn watch(&self, file: &File) -> bool {
        let (ident, flags) = (file.as_raw_fd() as usize, libc::EV_ADD | libc::EV_CLEAR);
        self.change(libc::EVFILT_VNODE, ident, flags, libc::NOTE_WRITE) == 0
    }

    pub(super) fn wait(&self, deadline: Instant) -> Wake {
        let Some(queue) = self.queue else {
            return self.timed(deadline);
        };
        loop {
            // A deadline already passed: returns at once, true once stopped.
            if self.stop.wait_until(Instant::now()) {
                return Wake::Stop;
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Wake::Deadline;
            }
            let timeout = libc::timespec {
                tv_sec: left.as_secs() as libc::time_t,
                tv_nsec: left.subsec_nanos() as libc::c_long,
            };
            let mut event = MaybeUninit::<libc::kevent>::uninit();
            let got = unsafe { libc::kevent(queue, null(), 0, event.as_mut_ptr(), 1, &timeout) };
            if got > 0 && unsafe { event.assume_init() }.filter == libc::EVFILT_VNODE {
                return Wake::Written;
            }
            if got < 0 && std::io::Error::last_os_error().kind() != ErrorKind::Interrupted {
                return self.timed(deadline);
            }
        }
    }

    fn timed(&self, deadline: Instant) -> Wake {
        if self.stop.wait_until(deadline) {
            Wake::Stop
        } else {
            Wake::Deadline
        }
    }

    fn change(&self, filter: i16, ident: usize, flags: u16, fflags: u32) -> i32 {
        let Some(queue) = self.queue else {
            return -1;
        };
        let change = libc::kevent {
            ident,
            filter,
            flags,
            fflags,
            data: 0,
            udata: null_mut(),
        };
        unsafe { libc::kevent(queue, &change, 1, null_mut(), 0, null()) }
    }
}

impl Drop for ExportWake {
    fn drop(&mut self) {
        if let Some(queue) = self.queue {
            unsafe { libc::close(queue) };
        }
    }
}

/// `CLOCK_UPTIME_RAW` in nanoseconds, the clock the app stamps requests with.
pub(super) fn uptime_ns() -> u64 {
    let mut now = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    unsafe { libc::clock_gettime(libc::CLOCK_UPTIME_RAW, &mut now) };
    (now.tv_sec as u64).saturating_mul(1_000_000_000) + now.tv_nsec as u64
}
