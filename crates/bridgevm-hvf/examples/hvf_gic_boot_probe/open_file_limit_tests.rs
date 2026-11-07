use super::*;

#[test]
fn launchds_soft_limit_rises_to_what_a_vm_needs() {
    assert_eq!(target(256, libc::RLIM_INFINITY), Some(WANT));
    assert_eq!(target(256, 4096), Some(4096), "never past the hard limit");
    assert_eq!(target(WANT, libc::RLIM_INFINITY), None);
    assert_eq!(
        target(1 << 20, libc::RLIM_INFINITY),
        None,
        "a higher limit is kept"
    );
}

#[test]
fn raising_leaves_this_process_at_least_at_the_target() {
    raise();
    let mut limit = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    assert_eq!(
        unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut limit) },
        0
    );
    assert!(
        limit.rlim_cur >= limit.rlim_max.min(WANT),
        "{}",
        limit.rlim_cur
    );
}
