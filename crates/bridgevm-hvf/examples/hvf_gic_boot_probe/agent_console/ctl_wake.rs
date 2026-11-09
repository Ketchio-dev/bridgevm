//! Event-driven agent service: a control-file append wakes the service thread
//! at once, and a guest agent reply wakes vCPU0, which drains it from the
//! device and hands it to the service (service_thread.rs). Neither waits for
//! the service's 250 ms tick or the 200 ms control-file poll. Input on the
//! ordered agent route was bounded by those (one command per round trip, about 4 Hz).

use std::os::fd::AsRawFd;
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use super::service_thread::ServiceSlot;
use super::{hv_vcpus_exit, HvVcpuT, CTL_ENV};

/// Set by the watcher on an append; the next service tick reads the file then.
static CTL_CHANGED: AtomicBool = AtomicBool::new(false);

pub(super) fn take_ctl_changed() -> bool {
    CTL_CHANGED.swap(false, Ordering::SeqCst)
}

/// The service reads the file and wakes vCPU0 itself once it has bytes to send.
pub(super) fn ctl_changed(service: &ServiceSlot) {
    CTL_CHANGED.store(true, Ordering::SeqCst);
    service.unpark();
}

/// Exit vCPU0 once per pending wake; a wake already pending covers this one.
pub(super) fn request_wake(vcpu: HvVcpuT, fired: &AtomicBool) {
    if fired
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_ok()
    {
        // SAFETY: `vcpu` is the live vCPU0 handle; the call only asks it to leave hv_vcpu_run.
        unsafe { hv_vcpus_exit(&vcpu, 1) };
    }
}

/// Install the guest-reply hook and watch the control file, when service mode has one.
pub(super) fn start(vcpu: HvVcpuT, fired: &Arc<AtomicBool>, service: &Arc<ServiceSlot>) {
    let hook_fired = Arc::clone(fired);
    let _ = bridgevm_hvf::virtio_console::AGENT_INBOUND_HOOK
        .set(Box::new(move || request_wake(vcpu, &hook_fired)));
    let Some(path) = std::env::var(CTL_ENV).ok().filter(|path| !path.is_empty()) else {
        return;
    };
    let service = Arc::clone(service);
    std::thread::spawn(move || loop {
        // The app creates the file with its first command; until then (or after a
        // rename) poll for it. A kqueue failure degrades to a 5 ms length poll.
        match std::fs::File::open(&path) {
            Ok(file) => {
                if !watch_until_replaced(&file, &service) {
                    poll_length(&path, &service);
                }
            }
            Err(_) => std::thread::sleep(Duration::from_millis(50)),
        }
    });
}

/// Wake on every write; return true when the file was deleted or renamed.
fn watch_until_replaced(file: &std::fs::File, service: &ServiceSlot) -> bool {
    // SAFETY: plain kqueue/kevent calls on descriptors this function owns or borrows.
    let queue = unsafe { libc::kqueue() };
    if queue < 0 {
        return false;
    }
    let change = libc::kevent {
        ident: file.as_raw_fd() as usize,
        filter: libc::EVFILT_VNODE,
        flags: libc::EV_ADD | libc::EV_CLEAR,
        fflags: libc::NOTE_WRITE | libc::NOTE_EXTEND | libc::NOTE_DELETE | libc::NOTE_RENAME,
        data: 0,
        udata: null_mut(),
    };
    let mut event: libc::kevent = unsafe { std::mem::zeroed() };
    // SAFETY: `change` and `event` are valid for the duration of each call.
    let mut ok = unsafe { libc::kevent(queue, &change, 1, null_mut(), 0, null()) } == 0;
    while ok {
        // SAFETY: as above; blocks until the next vnode event.
        let n = unsafe { libc::kevent(queue, null(), 0, &mut event, 1, null()) };
        if n < 0 && std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
            continue;
        }
        ok = n == 1;
        if ok {
            ctl_changed(service);
            if event.fflags & (libc::NOTE_DELETE | libc::NOTE_RENAME) != 0 {
                break;
            }
        }
    }
    // SAFETY: `queue` is owned here.
    unsafe { libc::close(queue) };
    ok
}

fn poll_length(path: &str, service: &ServiceSlot) {
    let mut last = std::fs::metadata(path).map(|m| m.len()).ok();
    loop {
        std::thread::sleep(Duration::from_millis(5));
        let now = std::fs::metadata(path).map(|m| m.len()).ok();
        if now.is_none() {
            return;
        }
        if now != last {
            last = now;
            ctl_changed(service);
        }
    }
}
