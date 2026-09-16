//! The runner owns signal policy; handlers only latch an atomic request.

use std::io;
use std::sync::atomic::{AtomicBool, Ordering};

static CANCELLED: AtomicBool = AtomicBool::new(false);
static INSTALLED: AtomicBool = AtomicBool::new(false);

extern "C" fn request(_: libc::c_int) {
    CANCELLED.store(true, Ordering::Relaxed);
}
pub(super) fn requested() -> bool {
    CANCELLED.load(Ordering::Relaxed)
}

pub(super) struct CancellationSignals {
    term: libc::sigaction,
    interrupt: libc::sigaction,
}

impl CancellationSignals {
    pub(super) fn install() -> io::Result<Self> {
        if INSTALLED.swap(true, Ordering::SeqCst) {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "runtime signal owner already installed",
            ));
        }
        CANCELLED.store(false, Ordering::Relaxed);
        // SAFETY: sigaction is a C record; handler only writes a lock-free atomic.
        let result = unsafe {
            let mut action: libc::sigaction = std::mem::zeroed();
            action.sa_sigaction = request as *const () as usize;
            libc::sigemptyset(&mut action.sa_mask);
            action.sa_flags = libc::SA_RESTART;
            let mut term = std::mem::zeroed();
            let mut interrupt = std::mem::zeroed();
            if libc::sigaction(libc::SIGTERM, &action, &mut term) != 0 {
                Err(io::Error::last_os_error())
            } else if libc::sigaction(libc::SIGINT, &action, &mut interrupt) != 0 {
                let error = io::Error::last_os_error();
                libc::sigaction(libc::SIGTERM, &term, std::ptr::null_mut());
                Err(error)
            } else {
                Ok(Self { term, interrupt })
            }
        };
        if result.is_err() {
            INSTALLED.store(false, Ordering::SeqCst);
        }
        result
    }
}

impl Drop for CancellationSignals {
    fn drop(&mut self) {
        // SAFETY: these are the previous actions returned during installation;
        // every owned child and lease has been released before this guard drops.
        unsafe {
            libc::sigaction(libc::SIGTERM, &self.term, std::ptr::null_mut());
            libc::sigaction(libc::SIGINT, &self.interrupt, std::ptr::null_mut());
        }
        INSTALLED.store(false, Ordering::SeqCst);
    }
}
