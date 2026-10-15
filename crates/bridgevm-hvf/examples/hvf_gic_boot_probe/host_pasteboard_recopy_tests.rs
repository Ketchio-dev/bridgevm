//! Epoch bounds and FIFO admission using the real worker/channel endpoints.

use super::tests::MemoryPasteboard;
use super::*;

#[test]
fn exhausted_recopy_epoch_stops_without_applying_queued_writes() {
    let (bridge, mut worker) = HostPasteboard::test_pair();
    let mut mac = MemoryPasteboard::new("host");
    assert!(worker.poll(&mut mac));
    assert_eq!(bridge.try_changed().as_deref(), Some("host"));
    worker.recopy.store(u64::MAX, Ordering::Release);
    bridge.set("queued-guest".into());
    mac.count += 1;
    assert!(!worker.poll(&mut mac));
    assert!(!worker.apply_pending(&mut mac));
    assert_eq!(mac.text, "host");
    assert!(mac.writes.is_empty());
    assert_eq!(bridge.try_changed(), None);
    assert_eq!(worker.recopy.load(Ordering::Acquire), u64::MAX);
}

#[test]
fn recopy_refusal_advances_report_revision_before_new_guest_admission() {
    let (bridge, mut worker) = HostPasteboard::test_pair();
    let mut mac = MemoryPasteboard::new("host");
    assert!(worker.poll(&mut mac));
    assert_eq!(bridge.try_changed().as_deref(), Some("host"));
    let initial = bridge.observed.get();
    bridge.set("old-guest".into());
    mac.count += 1;
    assert!(worker.poll(&mut mac));
    assert!(worker.apply_pending(&mut mac));
    assert_eq!(worker.revision, Some(initial + 1));
    bridge.set("guest-before-report-received".into());
    assert!(worker.apply_pending(&mut mac));
    assert!(mac.writes.is_empty());
    assert_eq!(bridge.try_changed().as_deref(), Some("host"));
    assert_eq!(bridge.observed.get(), initial + 1);
    bridge.set("guest-after-report-received".into());
    assert!(worker.apply_pending(&mut mac));
    assert_eq!(mac.writes, ["guest-after-report-received"]);
    assert!(worker.poll(&mut mac));
    assert_eq!(bridge.try_changed(), None);
}

#[test]
fn disconnected_recopy_owner_cannot_contaminate_fresh_bridge_epoch() {
    let (old, mut old_worker) = HostPasteboard::test_pair();
    let mut mac = MemoryPasteboard::new("host");
    assert!(old_worker.poll(&mut mac));
    assert_eq!(old.try_changed().as_deref(), Some("host"));
    old.set("old-guest".into());
    mac.count += 1;
    assert!(old_worker.poll(&mut mac));
    assert_eq!(old_worker.recopy.load(Ordering::Acquire), 1);
    drop(old);
    assert!(!old_worker.apply_pending(&mut mac));
    assert!(mac.writes.is_empty());
    let (fresh, mut worker) = HostPasteboard::test_pair();
    assert_eq!(worker.recopy.load(Ordering::Acquire), 0);
    assert!(worker.poll(&mut mac));
    assert_eq!(fresh.try_changed().as_deref(), Some("host"));
    fresh.set("fresh-guest".into());
    assert!(worker.apply_pending(&mut mac));
    assert_eq!(mac.writes, ["fresh-guest"]);
    assert!(worker.poll(&mut mac));
    assert_eq!(fresh.try_changed(), None);
}
