use std::cell::{Cell, RefCell};

use super::*;

thread_local! {
    static CALLS: RefCell<Vec<(u64, u64, u64)>> = const { RefCell::new(Vec::new()) };
    static REFUSE: Cell<bool> = const { Cell::new(false) };
}

fn fake(ipa: u64, len: u64, flags: u64) -> bool {
    CALLS.with(|calls| calls.borrow_mut().push((ipa, len, flags)));
    !REFUSE.with(Cell::get)
}

fn calls() -> Vec<(u64, u64, u64)> {
    CALLS.with(|calls| std::mem::take(&mut *calls.borrow_mut()))
}

const FB: (u64, u64) = (0x4123_4100, 0x10_0000);
const PAGES: (u64, u64) = (0x4123_4000, 0x10_4000);
/// A stage-2 write permission fault (DFSC level 3, WnR) and its neighbours.
const WRITE_FAULT: u64 = 0x9200_004f;
const READ_FAULT: u64 = 0x9200_000f;
const TRANSLATION_FAULT: u64 = 0x9200_0047;

#[test]
fn the_framebuffer_is_rounded_out_to_whole_stage2_pages() {
    assert_eq!(page_range(FB), Some(PAGES));
    assert_eq!(page_range((0x4000_0000, 0x4000)), Some((0x4000_0000, 0x4000)));
    assert_eq!(page_range((0x4000_0000, 0)), None);
}

#[test]
fn an_unwritten_frame_is_not_read_again_and_a_write_is_seen_once() {
    let watch = FbWatch::new(fake);
    assert!(watch.may_have_changed(Some(FB)), "a new range is always read");
    assert_eq!(calls(), [(PAGES.0, PAGES.1, READ_ONLY)]);
    assert!(!watch.may_have_changed(Some(FB)), "no write, no read");
    assert!(!watch.may_have_changed(Some(FB)));
    assert_eq!(calls(), []);

    assert!(watch.take_write_fault(FB.0 + 0x80_000, WRITE_FAULT));
    assert_eq!(calls(), [(PAGES.0, PAGES.1, WRITABLE)], "writable again for the retry");
    assert!(watch.may_have_changed(Some(FB)));
    assert_eq!(calls(), [(PAGES.0, PAGES.1, READ_ONLY)], "protected again before the read");
    assert!(!watch.may_have_changed(Some(FB)), "seen once");
}

#[test]
fn only_write_permission_faults_inside_the_range_are_taken() {
    let watch = FbWatch::new(fake);
    assert!(!watch.take_write_fault(FB.0, WRITE_FAULT), "nothing armed yet");
    watch.may_have_changed(Some(FB));
    calls();
    assert!(!watch.take_write_fault(FB.0, TRANSLATION_FAULT), "MMIO exits are translation faults");
    assert!(!watch.take_write_fault(FB.0, READ_FAULT));
    assert!(!watch.take_write_fault(PAGES.0 - 1, WRITE_FAULT));
    assert!(!watch.take_write_fault(PAGES.0 + PAGES.1, WRITE_FAULT));
    assert!(watch.take_write_fault(PAGES.0, WRITE_FAULT), "the rounded-out page counts");
    assert!(watch.take_write_fault(PAGES.0 + PAGES.1 - 1, WRITE_FAULT));
    assert_eq!(calls().len(), 2);
}

#[test]
fn a_new_geometry_or_an_inactive_ramfb_moves_or_drops_the_watch() {
    let watch = FbWatch::new(fake);
    watch.may_have_changed(Some(FB));
    calls();
    let moved = (0x5000_0000, 0x4000);
    assert!(watch.may_have_changed(Some(moved)));
    assert_eq!(calls(), [(PAGES.0, PAGES.1, WRITABLE), (moved.0, moved.1, READ_ONLY)]);
    assert!(watch.may_have_changed(None), "inactive: the caller decides");
    assert_eq!(calls(), [(moved.0, moved.1, WRITABLE)]);
    assert!(!watch.take_write_fault(moved.0, WRITE_FAULT));
    assert!(watch.may_have_changed(None), "no watch, always read");
    assert_eq!(calls(), []);
}

#[test]
fn a_refused_protection_falls_back_to_reading_every_tick() {
    let watch = FbWatch::new(fake);
    REFUSE.with(|refuse| refuse.set(true));
    assert!(watch.may_have_changed(Some(FB)));
    REFUSE.with(|refuse| refuse.set(false));
    assert_eq!(calls(), [(PAGES.0, PAGES.1, READ_ONLY), (PAGES.0, PAGES.1, WRITABLE)]);
    assert!(watch.may_have_changed(Some(FB)), "every tick reads");
    assert!(watch.may_have_changed(Some(FB)));
    assert_eq!(calls(), [], "and nothing is retried");
    assert!(!watch.take_write_fault(FB.0, WRITE_FAULT));
}

#[test]
fn release_leaves_guest_ram_writable() {
    let watch = FbWatch::new(fake);
    watch.may_have_changed(Some(FB));
    calls();
    watch.release();
    assert_eq!(calls(), [(PAGES.0, PAGES.1, WRITABLE)]);
    watch.release();
    assert_eq!(calls(), [], "once");
}
