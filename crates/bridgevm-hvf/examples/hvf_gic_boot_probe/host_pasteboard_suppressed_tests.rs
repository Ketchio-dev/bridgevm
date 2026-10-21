//! Counter and startup boundaries for host changes without transferable text.

use super::super::*;

struct Clipboard {
    text: Option<String>,
    count: i64,
    writes: Vec<String>,
}

impl PasteboardBackend for Clipboard {
    fn change_count(&mut self) -> i64 {
        self.count
    }
    fn get_string(&mut self) -> Option<String> {
        self.text.clone()
    }
    fn set_string(&mut self, text: &str) -> bool {
        self.text = Some(text.into());
        self.writes.push(text.into());
        self.count += 1;
        true
    }
}

#[test]
fn empty_and_nontext_startup_keep_guest_admission_compatibility() {
    for text in [None, Some("")] {
        let (bridge, mut worker) = HostPasteboard::test_pair();
        let mut mac = Clipboard {
            text: text.map(str::to_owned), count: 1, writes: Vec::new()
        };
        bridge.set("startup-guest".into());
        assert!(worker.apply_pending(&mut mac));
        assert_eq!(mac.writes, ["startup-guest"]);
        assert_eq!(worker.recopy.load(Ordering::Acquire), 0);
        assert!(worker.poll(&mut mac));
        assert_eq!(bridge.try_changed(), None);
    }
}

#[test]
fn exhausted_nontext_epoch_refuses_without_wrapping_or_writing() {
    for text in [None, Some("")] {
        let (bridge, mut worker) = HostPasteboard::test_pair();
        let mut mac = Clipboard {
            text: Some("host".into()), count: 1, writes: Vec::new()
        };
        assert!(worker.poll(&mut mac));
        assert_eq!(bridge.try_changed().as_deref(), Some("host"));
        worker.recopy.store(u64::MAX, Ordering::Release);
        bridge.set("queued-guest".into());
        mac.text = text.map(str::to_owned);
        mac.count += 1;
        assert!(!worker.poll(&mut mac));
        assert!(!worker.apply_pending(&mut mac));
        assert!(mac.writes.is_empty());
        assert_eq!(mac.text.as_deref(), text);
        assert_eq!(worker.recopy.load(Ordering::Acquire), u64::MAX);
        assert_eq!(bridge.try_changed(), None);
    }
}

#[test]
fn unchanged_nontext_polls_do_not_invalidate_later_guest_writes() {
    for text in [None, Some("")] {
        let (bridge, mut worker) = HostPasteboard::test_pair();
        let mut mac = Clipboard {
            text: Some("host".into()), count: 1, writes: Vec::new()
        };
        assert!(worker.poll(&mut mac));
        assert_eq!(bridge.try_changed().as_deref(), Some("host"));
        mac.text = text.map(str::to_owned);
        mac.count += 1;
        assert!(worker.poll(&mut mac));
        assert_eq!(worker.recopy.load(Ordering::Acquire), 1);
        assert_eq!(bridge.try_changed(), None);
        bridge.set("later-guest".into());
        for _ in 0..3 {
            assert!(worker.poll(&mut mac));
        }
        assert_eq!(worker.recopy.load(Ordering::Acquire), 1);
        assert!(worker.apply_pending(&mut mac));
        assert_eq!(mac.writes, ["later-guest"]);
        assert!(worker.poll(&mut mac));
        assert_eq!(bridge.try_changed(), None);
    }
}
