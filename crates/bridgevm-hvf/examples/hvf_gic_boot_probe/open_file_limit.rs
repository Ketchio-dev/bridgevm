//! The open-file limit a VM needs. An app opened from the Finder or the Dock
//! gets launchd's soft limit of 256 descriptors (hard: unlimited), and the
//! runner it spawns inherits it. The NAT alone may hold 256 UDP, 256 TCP and
//! 32 ICMP host sockets (its flow caps). Measured at soft 256: once the probe
//! held about 255 descriptors, 74 of one round's 570 guest connections failed,
//! with nothing in the log.

/// The soft limit a run asks for: plenty for the NAT's flow tables, and the
/// classic OPEN_MAX, which macOS always accepts.
const WANT: u64 = 10_240;

/// The soft limit to set, or None when the current one already suffices.
fn target(soft: u64, hard: u64) -> Option<u64> {
    let want = hard.min(WANT);
    (want > soft).then_some(want)
}

/// Raise the soft open-file limit toward WANT (never past the hard limit).
pub(crate) fn raise() {
    let mut limit = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    if unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut limit) } != 0 {
        return;
    }
    let Some(want) = target(limit.rlim_cur, limit.rlim_max) else {
        return;
    };
    let raised = libc::rlimit {
        rlim_cur: want,
        rlim_max: limit.rlim_max,
    };
    if unsafe { libc::setrlimit(libc::RLIMIT_NOFILE, &raised) } == 0 {
        println!("host: open-file limit {} -> {want}", limit.rlim_cur);
    } else {
        let error = std::io::Error::last_os_error();
        println!("host: open-file limit stays {} ({error})", limit.rlim_cur);
    }
}

#[cfg(test)]
mod tests {
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
}
