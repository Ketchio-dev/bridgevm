//! Wakes the VMM when a NAT host socket has something for the guest.
//!
//! The NAT reads its host sockets only when a vCPU leaves the guest (the
//! pre-run and MMIO drains). HVF keeps a vCPU in WFI, and takes its timer
//! interrupts, inside `hv_vcpu_run`, so a guest waiting for data made no exit
//! and the data waited in the host socket until some other exit came: during a
//! download vCPU0 spent about half its time in WFI (net-profile-20261106,
//! QEMU/HVF's vCPU0 17 %, whose main loop reads slirp's sockets itself).
//!
//! This thread sleeps on a kqueue of the NAT's sockets and, when one becomes
//! readable or finishes connecting, calls [`NET_RX_HOOK`]: the live VMM kicks
//! vCPU0, whose drain polls the NAT. One kick at a time; the next waits until a
//! poll has taken it, and a kick no poll answers within [`REKICK`] is repeated.
//! The thread exists only once the hook is set, so unit tests start none.

use std::sync::atomic::{AtomicBool, Ordering::SeqCst};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};
use std::thread::{self, Thread};
use std::time::Duration;

/// Called by the wake thread; the live VMM kicks vCPU0 so its drain polls the
/// NAT.
pub static NET_RX_HOOK: OnceLock<Box<dyn Fn() + Send + Sync>> = OnceLock::new();

/// A kick that no poll answered (it landed between two runs, or the VM is
/// paused) is sent again after this long.
const REKICK: Duration = Duration::from_millis(10);

/// What the thread and the NAT share. The kqueue is closed when both are done
/// with it, so a wake from `Drop` never meets a closed (or reused) descriptor.
#[derive(Debug)]
struct Shared {
    queue: i32,
    pending: AtomicBool,
    stop: AtomicBool,
    /// Sockets seen readable (or connected) since the last poll took them.
    ready: Mutex<Vec<i32>>,
}

impl Shared {
    fn ready(&self) -> MutexGuard<'_, Vec<i32>> {
        self.ready.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl Drop for Shared {
    fn drop(&mut self) {
        imp::close(self.queue);
    }
}

#[derive(Debug)]
pub(crate) struct HostWake {
    shared: Arc<Shared>,
    thread: Thread,
}

impl HostWake {
    /// The wake thread, when the VMM has set [`NET_RX_HOOK`] and a kqueue is
    /// available; otherwise the NAT polls on exits only, as before.
    pub(crate) fn start() -> Option<Self> {
        NET_RX_HOOK.get()?;
        imp::start()
    }

    /// The wake thread without the VMM's hook, for tests: events only mark
    /// sockets ready.
    #[cfg(test)]
    pub(crate) fn start_unhooked() -> Option<Self> {
        imp::start()
    }

    /// Tell the thread when `fd` has data to read (`writable`: once, when it
    /// can be written, as when a connect completes or a full socket drains).
    /// The registration ends when the socket is closed.
    /// False when the kqueue refused it: the socket is then read on every poll.
    pub(crate) fn watch(&self, fd: i32, writable: bool) -> bool {
        imp::watch(self.shared.queue, fd, writable)
    }

    /// A poll of the host sockets is starting: the kick it answers is done.
    pub(crate) fn polled(&self) {
        if self.shared.pending.swap(false, SeqCst) {
            self.thread.unpark();
        }
    }

    /// The sockets seen ready since the last call, into `out` (cleared first).
    pub(crate) fn take_ready(&self, out: &mut Vec<i32>) {
        out.clear();
        std::mem::swap(&mut *self.shared.ready(), out);
    }
}

impl Drop for HostWake {
    fn drop(&mut self) {
        self.shared.stop.store(true, SeqCst);
        self.thread.unpark();
        imp::interrupt(self.shared.queue);
    }
}

/// The thread's loop: wait for a socket event, then kick until a poll answers.
fn run(shared: &Shared, mut wait: impl FnMut() -> bool, mut kick: impl FnMut()) {
    while !shared.stop.load(SeqCst) {
        if !wait() {
            continue;
        }
        shared.pending.store(true, SeqCst);
        while shared.pending.load(SeqCst) && !shared.stop.load(SeqCst) {
            kick();
            thread::park_timeout(REKICK);
        }
    }
}

fn kick_vcpu() {
    if let Some(hook) = NET_RX_HOOK.get() {
        hook();
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use super::*;
    use std::ptr::{null, null_mut};

    const MAX_EVENTS: usize = 64;

    fn change(ident: usize, filter: i16, flags: u16, fflags: u32) -> libc::kevent {
        libc::kevent {
            ident,
            filter,
            flags,
            fflags,
            data: 0,
            udata: null_mut(),
        }
    }

    pub(super) fn start() -> Option<HostWake> {
        // SAFETY: Category 8 - plain kqueue/kevent calls on a queue owned here.
        let queue = unsafe { libc::kqueue() };
        if queue < 0 {
            return None;
        }
        let shared = Arc::new(Shared {
            queue,
            pending: AtomicBool::new(false),
            stop: AtomicBool::new(false),
            ready: Mutex::new(Vec::new()),
        });
        let user = change(0, libc::EVFILT_USER, libc::EV_ADD | libc::EV_CLEAR, 0);
        // SAFETY: Category 8 - `user` outlives the call.
        if unsafe { libc::kevent(queue, &user, 1, null_mut(), 0, null()) } != 0 {
            return None;
        }
        let ours = Arc::clone(&shared);
        let spawned = thread::Builder::new()
            .name("nat-wake".into())
            .spawn(move || {
                let mut events = [change(0, 0, 0, 0); MAX_EVENTS];
                let wait = || {
                    // SAFETY: Category 8 - `events` has room for MAX_EVENTS.
                    let got = unsafe {
                        libc::kevent(
                            queue,
                            null(),
                            0,
                            events.as_mut_ptr(),
                            MAX_EVENTS as i32,
                            null(),
                        )
                    };
                    let mut ready = ours.ready();
                    for event in &events[..got.max(0) as usize] {
                        if event.filter != libc::EVFILT_USER {
                            ready.push(event.ident as i32);
                        }
                    }
                    !ready.is_empty()
                };
                run(&ours, wait, kick_vcpu);
            });
        spawned.ok().map(|handle| HostWake {
            shared,
            thread: handle.thread().clone(),
        })
    }

    pub(super) fn watch(queue: i32, fd: i32, writable: bool) -> bool {
        let watched = if writable {
            change(
                fd as usize,
                libc::EVFILT_WRITE,
                libc::EV_ADD | libc::EV_ONESHOT,
                0,
            )
        } else {
            change(
                fd as usize,
                libc::EVFILT_READ,
                libc::EV_ADD | libc::EV_CLEAR,
                0,
            )
        };
        // SAFETY: Category 8 - `watched` outlives the call.
        unsafe { libc::kevent(queue, &watched, 1, null_mut(), 0, null()) == 0 }
    }

    pub(super) fn interrupt(queue: i32) {
        let trigger = change(0, libc::EVFILT_USER, 0, libc::NOTE_TRIGGER);
        // SAFETY: Category 8 - the caller's `Shared` keeps the queue open.
        unsafe { libc::kevent(queue, &trigger, 1, null_mut(), 0, null()) };
    }

    pub(super) fn close(queue: i32) {
        // SAFETY: Category 8 - the last owner of the queue closes it once.
        unsafe { libc::close(queue) };
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    use super::*;

    pub(super) fn start() -> Option<HostWake> {
        None
    }

    pub(super) fn watch(_queue: i32, _fd: i32, _writable: bool) -> bool {
        false
    }

    pub(super) fn interrupt(_queue: i32) {}

    pub(super) fn close(_queue: i32) {}
}

#[cfg(test)]
#[path = "host_wake_tests.rs"]
mod tests;
