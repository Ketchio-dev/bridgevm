//! Exact root-state lines: initial readiness is silent and only changes print.

use super::root_state_transition;

#[test]
fn root_state_lines_print_only_changes_across_two_cycles() {
    let mut blocked = false;
    assert_eq!(root_state_transition(&mut blocked, false), None);
    for _ in 0..2 {
        assert_eq!(
            root_state_transition(&mut blocked, true),
            Some("BVAGENT SHARE root-state blocked")
        );
        assert!(blocked);
        assert_eq!(root_state_transition(&mut blocked, true), None);
        assert_eq!(
            root_state_transition(&mut blocked, false),
            Some("BVAGENT SHARE root-state ready")
        );
        assert!(!blocked);
        assert_eq!(root_state_transition(&mut blocked, false), None);
    }
}
