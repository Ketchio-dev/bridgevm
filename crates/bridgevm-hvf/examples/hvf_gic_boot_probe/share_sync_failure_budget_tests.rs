//! Failed-name retention is bounded without evicting data-loss protection.

use super::*;

#[test]
fn refused_name_churn_stops_at_the_failed_record_limit() {
    let mut sync = ShareSync::new(1024);
    for i in 0..4096 {
        let name = format!("blocked/child-{i}");
        sync.on_guest_file(name.clone(), b"x".to_vec(), Some("g1"));
        assert_eq!(sync.on_host_write_failed(&name), i == 4095);
        sync.on_guest_listing(Vec::new());
        sync.forget_absent_failed(|_| false);
    }
    assert_eq!(sync.records.len(), 4096);
}

#[test]
fn failed_key_byte_budget_precedes_record_limit_and_retries_count_once() {
    let mut sync = ShareSync::new(1024);
    let name = "x".repeat(2 * 1024 * 1024);
    for _ in 0..4 {
        sync.on_guest_file(name.clone(), b"x".to_vec(), Some("g1"));
        assert!(!sync.on_host_write_failed(&name));
    }
    let other = format!("y{}", &name[1..]);
    sync.on_guest_file(other.clone(), b"x".to_vec(), Some("g1"));
    assert!(sync.on_host_write_failed(&other));
    assert_eq!(sync.records.len(), 2);
}
