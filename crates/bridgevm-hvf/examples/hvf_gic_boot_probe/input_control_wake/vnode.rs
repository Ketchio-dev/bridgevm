//! Event-driven live-input wake with a bounded polling fallback.

use std::io::ErrorKind;
use std::os::fd::AsRawFd;
use std::ptr::{null, null_mut};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Duration;

use crate::live_input::InputControlFile;
use crate::HvVcpuT;

const FALLBACK_POLL: Duration = Duration::from_millis(2);

pub(super) fn watch(mut file: InputControlFile, vcpu: HvVcpuT, fired: Arc<AtomicBool>) {
    let queue = file
        .file()
        .map(|source| source.as_raw_fd())
        .and_then(VnodeQueue::new);
    if let Some(queue) = queue {
        while queue.wait() {
            request_wake(vcpu, &fired);
        }
        request_wake(vcpu, &fired);
    }
    poll(file, vcpu, fired);
}

fn request_wake(vcpu: HvVcpuT, fired: &AtomicBool) {
    if fired
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_ok()
    {
        super::exit_vcpu(vcpu);
    }
}

fn poll(mut file: InputControlFile, vcpu: HvVcpuT, fired: Arc<AtomicBool>) -> ! {
    let mut length = file.length().unwrap_or(0);
    loop {
        if input_length_changed(&mut length, file.length()) {
            request_wake(vcpu, &fired);
        }
        std::thread::sleep(FALLBACK_POLL);
    }
}

fn input_length_changed(previous: &mut u64, observed: Option<u64>) -> bool {
    let Some(observed) = observed else {
        return false;
    };
    if observed == *previous {
        return false;
    }
    *previous = observed;
    true
}

struct VnodeQueue(i32);

impl VnodeQueue {
    fn new(file: i32) -> Option<Self> {
        let queue = unsafe { libc::kqueue() };
        if queue < 0 {
            return None;
        }
        let change = libc::kevent {
            ident: file as usize,
            filter: libc::EVFILT_VNODE,
            flags: libc::EV_ADD | libc::EV_CLEAR,
            fflags: libc::NOTE_WRITE | libc::NOTE_EXTEND,
            data: 0,
            udata: null_mut(),
        };
        let status = unsafe { libc::kevent(queue, &change, 1, null_mut(), 0, null()) };
        if status == 0 {
            Some(Self(queue))
        } else {
            unsafe { libc::close(queue) };
            None
        }
    }

    fn wait(&self) -> bool {
        self.wait_with_timeout(null())
    }

    #[cfg(test)]
    fn wait_for(&self, timeout: Duration) -> bool {
        let timeout = libc::timespec {
            tv_sec: timeout.as_secs() as libc::time_t,
            tv_nsec: timeout.subsec_nanos() as libc::c_long,
        };
        self.wait_with_timeout(&timeout)
    }

    fn wait_with_timeout(&self, timeout: *const libc::timespec) -> bool {
        loop {
            let mut event = std::mem::MaybeUninit::uninit();
            let status = unsafe {
                libc::kevent(self.0, null(), 0, event.as_mut_ptr(), 1, timeout)
            };
            if status > 0 {
                return true;
            }
            if status == 0 {
                return false;
            }
            if std::io::Error::last_os_error().kind() != ErrorKind::Interrupted {
                return false;
            }
        }
    }
}

impl Drop for VnodeQueue {
    fn drop(&mut self) {
        unsafe { libc::close(self.0) };
    }
}

#[cfg(test)]
#[test]
fn only_real_control_file_length_changes_request_a_wake() {
    let mut length = 0;
    assert!(!input_length_changed(&mut length, None));
    assert!(!input_length_changed(&mut length, Some(0)));
    assert!(input_length_changed(&mut length, Some(31)));
    assert!(!input_length_changed(&mut length, Some(31)));
    assert!(input_length_changed(&mut length, Some(63)));
}

#[cfg(test)]
#[test]
fn vnode_queue_reports_an_append_without_polling() {
    use std::fs::OpenOptions;
    use std::io::Write;
    use std::time::{SystemTime, UNIX_EPOCH};

    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("bridgevm-vnode-{unique}"));
    let mut writer = OpenOptions::new()
        .create_new(true)
        .append(true)
        .open(&path)
        .unwrap();
    let mut input = InputControlFile::from_path(path.clone());
    let source = input.file().unwrap().as_raw_fd();
    let queue = VnodeQueue::new(source).expect("register vnode queue");

    writeln!(writer, "POINTER click:1:1").unwrap();
    writer.flush().unwrap();
    assert!(queue.wait_for(Duration::from_secs(1)));

    drop(queue);
    drop(input);
    drop(writer);
    std::fs::remove_file(path).unwrap();
}
