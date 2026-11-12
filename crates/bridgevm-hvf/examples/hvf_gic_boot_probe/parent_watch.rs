//! Runner lifetime watch; direct script launches do not opt in.

/// Returns only after the original runner exits. No runner marker means no watch.
/// Capturing and registering before VM setup closes the parent-death startup race.
pub(crate) fn wait_for_runner_exit() -> bool {
    if std::env::var_os("BRIDGEVM_RESET_GENERATION").is_none() {
        return false;
    }
    // SAFETY: getppid has no preconditions.
    let parent = unsafe { libc::getppid() };
    if parent == 1 {
        return true;
    }
    // SAFETY: the returned descriptor is exclusively owned by this thread.
    let queue = unsafe { libc::kqueue() };
    if queue >= 0 {
        let change = libc::kevent {
            ident: parent as usize,
            filter: libc::EVFILT_PROC,
            flags: libc::EV_ADD | libc::EV_ONESHOT,
            fflags: libc::NOTE_EXIT,
            data: 0,
            udata: std::ptr::null_mut(),
        };
        // SAFETY: initialized change, no event output requested.
        let registered =
            unsafe { libc::kevent(queue, &change, 1, std::ptr::null_mut(), 0, std::ptr::null()) };
        if registered == 0 {
            let mut event = std::mem::MaybeUninit::<libc::kevent>::uninit();
            loop {
                // SAFETY: output has room for one event; no timeout or change list.
                let got = unsafe {
                    libc::kevent(
                        queue,
                        std::ptr::null(),
                        0,
                        event.as_mut_ptr(),
                        1,
                        std::ptr::null(),
                    )
                };
                if got > 0 {
                    // SAFETY: queue is owned here and is no longer used.
                    unsafe { libc::close(queue) };
                    return true;
                }
                if std::io::Error::last_os_error().kind() != std::io::ErrorKind::Interrupted {
                    break;
                }
            }
        }
        // SAFETY: queue is owned here and is no longer used.
        unsafe { libc::close(queue) };
    }
    // ESRCH during registration, or a resource failure: parent identity still decides.
    // getppid cannot confuse a reused PID with this process's original parent.
    while unsafe { libc::getppid() } == parent {
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
    true
}
