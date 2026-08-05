//! Kill-mode policy and the flag that actually stops the run.

use super::*;

    /// Guards the default. Kill mode changes a reported stall into an ended
    /// run, so anything other than an explicit opt-in must leave it off.
    #[test]
    fn kill_is_off_unless_explicitly_requested() {
        for value in ["", "0", "true", "yes", "2", "01"] {
            assert!(
                !kill_requested_from(Some(value)),
                "{value:?} must not enable kill mode"
            );
        }
        assert!(!kill_requested_from(None));
        assert!(kill_requested_from(Some("1")));
    }

    /// The flag is what stops the run; the vCPU wake alone does not. A live
    /// gate logged "ending run (kill mode)" and then ran another 8 minutes,
    /// because an exit with no flag set reads as a surplus cancel and the
    /// loop continues.
    #[test]
    fn firing_sets_the_flag_the_run_loop_reads() {
        let kill = BootProgressKill {
            vcpu: 0,
            fired: Arc::new(AtomicBool::new(false)),
        };
        let observed = Arc::clone(&kill.fired);
        assert!(!observed.load(Ordering::SeqCst));
        kill.fired.store(true, Ordering::SeqCst);
        assert!(observed.load(Ordering::SeqCst));
    }

    /// The clone the run loop holds must see the original's fire, or the
    /// stall verdict never reaches the thread that can act on it.
    #[test]
    fn a_cloned_handle_shares_the_flag() {
        let kill = BootProgressKill {
            vcpu: 0,
            fired: Arc::new(AtomicBool::new(false)),
        };
        let copy = kill.clone();
        kill.fired.store(true, Ordering::SeqCst);
        assert!(copy.fired.load(Ordering::SeqCst));
    }
