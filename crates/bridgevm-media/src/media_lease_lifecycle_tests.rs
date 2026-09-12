use super::*;
use std::io::Write;
use std::os::unix::net::UnixStream;

#[test]
fn duplicate_descriptions_do_not_delay_owner_unlock() {
    let s = Scratch::new("lease-duplicate-lifetime");
    let disk = s.write("disk", b"disk");
    let lease = MediaLease::acquire([disk.as_path()]).unwrap();
    let inherited: Vec<_> = lease.files.iter().map(|f| f.try_clone().unwrap()).collect();
    drop(lease);
    let next = MediaLease::acquire([disk.as_path()]).unwrap();
    drop(inherited);
    assert!(MediaLease::acquire([disk.as_path()]).is_err());
    drop(next);
    MediaLease::acquire([disk.as_path()]).unwrap();
}

#[test]
fn forked_child_before_exec_does_not_delay_owner_unlock() {
    let s = Scratch::new("lease-fork-lifetime");
    let disk = s.write("disk", b"disk");
    let lease = MediaLease::acquire([disk.as_path()]).unwrap();
    let (mut parent, child) = UnixStream::pair().unwrap();
    let parent_fd = parent.as_raw_fd();
    let child_fd = child.as_raw_fd();
    // SAFETY: the child uses only async-signal-safe libc calls and stack data,
    // then _exit without running inherited Rust destructors. Its alarm bounds
    // lifetime even if the parent test panics; the parent always reaps normally.
    let pid = unsafe {
        let pid = libc::fork();
        if pid == 0 {
            libc::alarm(5);
            libc::close(parent_fd);
            let mut byte = 0u8;
            let count = libc::read(child_fd, (&mut byte as *mut u8).cast(), 1);
            libc::_exit(if count == 1 { 0 } else { 2 });
        }
        pid
    };
    assert!(pid > 0, "fork failed: {}", io::Error::last_os_error());
    drop(child);
    drop(lease);
    let reacquired = MediaLease::acquire([disk.as_path()]);
    let signaled = parent.write_all(&[1]);
    let mut status = 0;
    let waited = loop {
        // SAFETY: pid is our child; status points to initialized writable storage.
        let result = unsafe { libc::waitpid(pid, &mut status, 0) };
        if result < 0 && io::Error::last_os_error().kind() == io::ErrorKind::Interrupted {
            continue;
        }
        break result;
    };
    assert_eq!(waited, pid);
    assert!(libc::WIFEXITED(status));
    assert_eq!(libc::WEXITSTATUS(status), 0);
    signaled.unwrap();
    reacquired.unwrap();
}
