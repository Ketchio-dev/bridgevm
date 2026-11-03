use std::cell::{Cell, RefCell};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use super::*;
use crate::machine::{FLASH_CODE, RAM_BASE};

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

/// Guest RAM as the exporter reads it: 1 GiB at `RAM_BASE`.
const RAM_END: u64 = RAM_BASE + (1 << 30);

/// The exporter's view of guest RAM; the watch looks only at its bounds.
struct Ram;

impl GuestMemoryMut for Ram {
    fn write_bytes(&mut self, _: u64, _: &[u8]) -> bool {
        false
    }

    fn read_bytes(&self, _: u64, _: usize) -> Option<Vec<u8>> {
        None
    }

    fn host_ptr(&self, gpa: u64, len: usize) -> Option<*mut u8> {
        let end = gpa.checked_add(u64::try_from(len).ok()?)?;
        (RAM_BASE <= gpa && end <= RAM_END).then_some(std::ptr::null_mut())
    }
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
    assert!(watch.may_have_changed(Some(FB), &Ram), "a new range is always read");
    assert_eq!(calls(), [(PAGES.0, PAGES.1, READ_ONLY)]);
    assert!(!watch.may_have_changed(Some(FB), &Ram), "no write, no read");
    assert!(!watch.may_have_changed(Some(FB), &Ram));
    assert_eq!(calls(), []);

    assert!(watch.take_write_fault(FB.0 + 0x80_000, WRITE_FAULT));
    assert_eq!(calls(), [(PAGES.0, PAGES.1, WRITABLE)], "writable again for the retry");
    assert!(watch.may_have_changed(Some(FB), &Ram));
    assert_eq!(calls(), [(PAGES.0, PAGES.1, READ_ONLY)], "protected again before the read");
    assert!(!watch.may_have_changed(Some(FB), &Ram), "seen once");
}

#[test]
fn only_write_permission_faults_inside_the_range_are_taken() {
    let watch = FbWatch::new(fake);
    assert!(!watch.take_write_fault(FB.0, WRITE_FAULT), "nothing protected yet");
    watch.may_have_changed(Some(FB), &Ram);
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
    watch.may_have_changed(Some(FB), &Ram);
    calls();
    let moved = (0x5000_0000, 0x4000);
    assert!(watch.may_have_changed(Some(moved), &Ram));
    assert_eq!(calls(), [(PAGES.0, PAGES.1, WRITABLE), (moved.0, moved.1, READ_ONLY)]);
    assert!(watch.may_have_changed(None, &Ram), "inactive: the caller decides");
    assert_eq!(calls(), [(moved.0, moved.1, WRITABLE)]);
    assert!(watch.may_have_changed(None, &Ram), "no watch, always read");
    assert_eq!(calls(), []);
}

#[test]
fn a_refused_protection_falls_back_to_reading_every_tick() {
    let watch = FbWatch::new(fake);
    REFUSE.with(|refuse| refuse.set(true));
    assert!(watch.may_have_changed(Some(FB), &Ram));
    REFUSE.with(|refuse| refuse.set(false));
    assert_eq!(calls(), [(PAGES.0, PAGES.1, READ_ONLY), (PAGES.0, PAGES.1, WRITABLE)]);
    assert!(watch.may_have_changed(Some(FB), &Ram), "every tick reads");
    assert!(watch.may_have_changed(Some(FB), &Ram));
    assert_eq!(calls(), [], "and nothing is retried");
    assert!(watch.take_write_fault(FB.0, WRITE_FAULT), "a store there still completes");
    assert_eq!(calls(), []);
}

#[test]
fn a_framebuffer_outside_guest_ram_is_never_protected() {
    let in_firmware_code = (FLASH_CODE.base + 0x1000, 0x10_0000);
    let above_ram = (RAM_END, 0x10_0000);
    for outside in [in_firmware_code, above_ram] {
        let watch = FbWatch::new(fake);
        assert!(watch.may_have_changed(Some(outside), &Ram));
        assert!(watch.may_have_changed(Some(outside), &Ram), "every tick reads");
        assert_eq!(calls(), [], "nothing protected at {outside:x?}");
        assert!(!watch.take_write_fault(outside.0, WRITE_FAULT), "its faults go the usual way");
        assert!(watch.may_have_changed(Some(FB), &Ram), "and the watch stays off");
        assert_eq!(calls(), []);
    }
}

#[test]
fn a_framebuffer_straddling_the_end_of_guest_ram_is_refused() {
    let watch = FbWatch::new(fake);
    assert!(watch.may_have_changed(Some((RAM_END - 0x2000, 0x4000)), &Ram));
    assert!(watch.may_have_changed(Some((RAM_END - 0x2000, 0x4000)), &Ram));
    assert_eq!(calls(), [], "its last page is not RAM");
    let watch = FbWatch::new(fake);
    assert!(watch.may_have_changed(Some((RAM_END - PAGE, PAGE)), &Ram));
    assert_eq!(calls(), [(RAM_END - PAGE, PAGE, READ_ONLY)], "the last RAM page is watched");
}

#[test]
fn a_refused_reprotection_falls_back_to_reading_every_tick() {
    let watch = FbWatch::new(fake);
    watch.may_have_changed(Some(FB), &Ram);
    assert!(watch.take_write_fault(FB.0, WRITE_FAULT));
    calls();
    REFUSE.with(|refuse| refuse.set(true));
    assert!(watch.may_have_changed(Some(FB), &Ram), "the write is read");
    REFUSE.with(|refuse| refuse.set(false));
    assert!(watch.may_have_changed(Some(FB), &Ram), "every later tick reads");
    assert!(watch.may_have_changed(Some(FB), &Ram));
    assert_eq!(
        calls(),
        [(PAGES.0, PAGES.1, READ_ONLY), (PAGES.0, PAGES.1, WRITABLE)],
        "left writable and not retried"
    );
}

#[test]
fn a_store_that_faulted_before_the_watch_moved_or_stopped_is_still_retried() {
    let watch = FbWatch::new(fake);
    let moved = (0x5000_0000, 0x4000);
    let late = FB.0 + 0x80_000;
    watch.may_have_changed(Some(FB), &Ram);
    // A store faulted on the protected frame, and its vCPU handles the fault
    // only after the exporter made the pages writable and moved the watch.
    watch.may_have_changed(Some(moved), &Ram);
    calls();
    assert!(watch.take_write_fault(late, WRITE_FAULT), "retried, not emulated as MMIO");
    assert_eq!(calls(), [], "on the page the exporter made writable");
    assert!(!watch.may_have_changed(Some(moved), &Ram), "the watched frame was not written");
    watch.release();
    calls();
    assert!(watch.take_write_fault(moved.0, WRITE_FAULT), "after the exporter stopped too");
    calls();
    assert!(!watch.take_write_fault(moved.0 + moved.1, WRITE_FAULT), "never protected");
    assert_eq!(calls(), []);
}

/// The watch whose first write protection races a vCPU store.
static RACED: FbWatch = FbWatch::new(protect_racing_a_store);
static RACING_VCPU: Mutex<Option<JoinHandle<bool>>> = Mutex::new(None);

/// The moment the framebuffer first turns read-only, a vCPU stores to it and
/// handles the fault on its own thread. The exporter waits a while before it
/// goes on, so a handler that does not wait for the watch to be armed runs
/// in the window between the protection and the arming.
fn protect_racing_a_store(ipa: u64, _len: u64, flags: u64) -> bool {
    static RACED_ONCE: AtomicBool = AtomicBool::new(false);
    if flags == READ_ONLY && !RACED_ONCE.swap(true, Ordering::Relaxed) {
        let (handled, finished) = mpsc::channel();
        let vcpu = std::thread::spawn(move || {
            let retried = RACED.take_write_fault(ipa, WRITE_FAULT);
            handled.send(()).ok();
            retried
        });
        finished.recv_timeout(Duration::from_millis(200)).ok();
        *RACING_VCPU.lock().unwrap() = Some(vcpu);
    }
    true
}

#[test]
fn a_store_that_faults_as_the_pages_turn_read_only_is_retried() {
    assert!(RACED.may_have_changed(Some(FB), &Ram));
    let vcpu = RACING_VCPU.lock().unwrap().take().expect("the store raced the protection");
    assert!(vcpu.join().unwrap(), "retried, not emulated as MMIO");
    assert!(RACED.may_have_changed(Some(FB), &Ram), "and the frame is read again");
}

#[test]
fn release_leaves_guest_ram_writable() {
    let watch = FbWatch::new(fake);
    watch.may_have_changed(Some(FB), &Ram);
    calls();
    watch.release();
    assert_eq!(calls(), [(PAGES.0, PAGES.1, WRITABLE)]);
    watch.release();
    assert_eq!(calls(), [], "once");
}
