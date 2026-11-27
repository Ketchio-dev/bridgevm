//! A swtpm whose runner was killed (SIGKILL, a crash) never gets stopped: launchd adopts it, it
//! keeps serving, and it keeps the lock on the VM's vTPM state. Every later swtpm for that VM
//! then exits at once ("Could not lock access to lockfile"), so Windows could not start again
//! until the Mac restarted. Before a run starts its swtpm, a holder of this state's lock is
//! stopped, but only when it is an orphan (its parent is launchd) whose executable is swtpm. A
//! live runner's swtpm is its child, so it is never touched.
use std::ffi::OsStr;
use std::fs::File;
use std::os::fd::AsRawFd;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::time::{Duration, Instant};

/// swtpm's lock file in its state directory (SWTPM_NVRAM_Lock_Dir), a POSIX record lock.
const LOCK_FILE: &str = ".lock";
const GRACE: Duration = Duration::from_secs(2);

pub(super) fn stop_orphaned_holder(state_dir: &Path) {
    let Some(pid) = lock_holder(state_dir) else {
        return;
    };
    if !orphaned_swtpm(pid) {
        return;
    }
    eprintln!("vTPM: stopping swtpm {pid}, left running by an earlier run of this VM");
    for signal in [libc::SIGTERM, libc::SIGKILL] {
        // SAFETY: kill has no pointer arguments.
        unsafe { libc::kill(pid, signal) };
        let deadline = Instant::now() + GRACE;
        while lock_holder(state_dir) == Some(pid) {
            if Instant::now() >= deadline {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        if lock_holder(state_dir) != Some(pid) {
            return;
        }
    }
}

/// The process holding a write lock on the state's lock file, if any.
fn lock_holder(state_dir: &Path) -> Option<libc::pid_t> {
    let file = File::open(state_dir.join(LOCK_FILE)).ok()?;
    // SAFETY: flock is plain data; zero is a valid whole-file query (start 0, length 0).
    let mut lock: libc::flock = unsafe { std::mem::zeroed() };
    lock.l_type = libc::F_WRLCK as libc::c_short;
    lock.l_whence = libc::SEEK_SET as libc::c_short;
    // SAFETY: the fd is open for the call and `lock` outlives it.
    if unsafe { libc::fcntl(file.as_raw_fd(), libc::F_GETLK, &mut lock) } != 0 {
        return None;
    }
    (lock.l_type != libc::F_UNLCK as libc::c_short && lock.l_pid > 0).then_some(lock.l_pid)
}

fn orphaned_swtpm(pid: libc::pid_t) -> bool {
    // SAFETY: proc_bsdinfo is plain data, and proc_pidinfo writes at most `size` bytes into it.
    let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of::<libc::proc_bsdinfo>() as libc::c_int;
    let read = unsafe {
        libc::proc_pidinfo(
            pid,
            libc::PROC_PIDTBSDINFO,
            0,
            (&mut info as *mut libc::proc_bsdinfo).cast(),
            size,
        )
    };
    if read != size || info.pbi_ppid != 1 {
        return false;
    }
    let mut path = [0u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
    // SAFETY: proc_pidpath writes at most `path.len()` bytes.
    let length = unsafe { libc::proc_pidpath(pid, path.as_mut_ptr().cast(), path.len() as u32) };
    length > 0
        && Path::new(OsStr::from_bytes(&path[..length as usize])).file_name()
            == Some(OsStr::new("swtpm"))
}
