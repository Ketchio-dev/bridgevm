use super::{uptime_ns, DAMAGE_RECT_OFFSET, DAMAGE_TAG_OFFSET, PUBLISH_TIME_OFFSET};
use crate::display_fb::requests::{FRAME_REQUEST_MARKER_OFFSET, FRAME_REQUEST_OFFSET};
use crate::display_fb::DisplayFramebuffer;
use crate::ramfb::DRM_FORMAT_XRGB8888;
use crate::virtio_gpu::{FbSink, Rect};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

fn test_path() -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "bridgevm-fb-sink-header-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("display.fb")
}

fn word(path: &Path, offset: usize) -> u64 {
    let file = std::fs::read(path).unwrap();
    u64::from_le_bytes(file[offset..offset + 8].try_into().unwrap())
}

/// The publish time and the sequence a reader sees once the write returned.
fn stamped(path: &Path) -> (u64, u64) {
    (word(path, PUBLISH_TIME_OFFSET), word(path, 24))
}

#[test]
fn every_published_frame_carries_the_time_its_bytes_were_complete() {
    let path = test_path();
    let mut sink = FbSink::at_path(path.clone());
    let before = uptime_ns();
    sink.write(1, 1, 4, DRM_FORMAT_XRGB8888, &[1, 2, 3, 4]);
    let (first, sequence) = stamped(&path);
    assert!(
        before <= first && first <= uptime_ns(),
        "stamped while publishing"
    );
    assert_eq!(sequence, 2);
    // A damaged frame is a published frame too, and is stamped again.
    let whole = Rect {
        x: 0,
        y: 0,
        width: 1,
        height: 1,
    };
    sink.write_damage(1, 1, 4, DRM_FORMAT_XRGB8888, &[9; 4], whole);
    let (second, sequence) = stamped(&path);
    assert!(first <= second && second <= uptime_ns());
    assert_eq!(sequence, 4);
    // A refused write publishes nothing and keeps the last frame's time.
    sink.write(4, 4, 16, DRM_FORMAT_XRGB8888, &[0; 3]);
    assert_eq!(stamped(&path), (second, 4));
    drop(sink);
    std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
}

#[test]
fn the_ramfb_writer_stamps_its_frames_and_leaves_the_request_words_alone() {
    let path = test_path();
    let mut fb = DisplayFramebuffer::at_path(&path);
    let before = uptime_ns();
    fb.publish(1, 1, 4, DRM_FORMAT_XRGB8888, &[1, 2, 3, 4])
        .unwrap();
    let (time, sequence) = stamped(&path);
    assert!(before <= time && time <= uptime_ns());
    assert_eq!(sequence, 2);
    assert_eq!(word(&path, FRAME_REQUEST_OFFSET), 0, "no request posted");
    let marker = std::fs::read(&path).unwrap()[FRAME_REQUEST_MARKER_OFFSET..][..4].to_vec();
    assert_eq!(marker, [0; 4], "a plain publish takes no requests");
    // The next process's writer stamps over a time an earlier writer left.
    drop(fb);
    let mut next = DisplayFramebuffer::at_path(&path);
    next.publish(1, 1, 4, DRM_FORMAT_XRGB8888, &[5, 6, 7, 8])
        .unwrap();
    let (later, sequence) = stamped(&path);
    assert!(time <= later && later <= uptime_ns());
    assert_eq!(sequence, 4, "the sequence continues across writers");
    std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
}

/// The damage words a reader sees once the write returned: (tag, [x, y, width, height]).
fn damage(path: &Path) -> (u32, [u16; 4]) {
    let file = std::fs::read(path).unwrap();
    let tag = u32::from_le_bytes(file[DAMAGE_TAG_OFFSET..][..4].try_into().unwrap());
    let rect = [0, 2, 4, 6]
        .map(|at| u16::from_le_bytes(file[DAMAGE_RECT_OFFSET + at..][..2].try_into().unwrap()));
    (tag, rect)
}

fn rect(x: u32, y: u32, width: u32, height: u32) -> Rect {
    Rect {
        x,
        y,
        width,
        height,
    }
}

#[test]
fn every_frame_records_what_it_changed_tagged_with_its_sequence() {
    let path = test_path();
    let mut sink = FbSink::at_path(path.clone());
    // The first frame is copied whole, damage or not, and says so.
    sink.write_damage(4, 3, 16, DRM_FORMAT_XRGB8888, &[1; 48], rect(1, 1, 1, 1));
    assert_eq!(damage(&path), (2, [0, 0, 4, 3]));
    // On its own frame, a damaged write records its rectangle clipped to the frame.
    sink.write_damage(4, 3, 16, DRM_FORMAT_XRGB8888, &[2; 48], rect(3, 1, 10, 10));
    assert_eq!(damage(&path), (4, [3, 1, 1, 2]));
    // Damage outside the frame changes nothing: 0x0.
    sink.write_damage(4, 3, 16, DRM_FORMAT_XRGB8888, &[3; 48], rect(8, 8, 2, 2));
    assert_eq!(damage(&path), (6, [0, 0, 0, 0]));
    // A full write records the whole frame.
    sink.write(4, 3, 16, DRM_FORMAT_XRGB8888, &[4; 48]);
    assert_eq!(damage(&path), (8, [0, 0, 4, 3]));
    // A refused write leaves the last frame's words.
    sink.write(4, 3, 16, DRM_FORMAT_XRGB8888, &[0; 3]);
    assert_eq!(damage(&path), (8, [0, 0, 4, 3]));
    drop(sink);
    std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
}

#[test]
fn a_frame_copied_whole_records_the_whole_frame_for_every_writer() {
    let path = test_path();
    let mut device = FbSink::at_path(path.clone());
    let mut fb = DisplayFramebuffer::at_path(&path);
    device.write(2, 2, 8, DRM_FORMAT_XRGB8888, &[1; 16]);
    // Another writer of the file (the ramfb exporter in its own process here) publishes whole frames.
    fb.publish(2, 2, 8, DRM_FORMAT_XRGB8888, &[2; 16]).unwrap();
    let (tag, changed) = damage(&path);
    assert_eq!(changed, [0, 0, 2, 2]);
    assert_eq!(
        u64::from(tag),
        word(&path, 24),
        "tagged with the sequence it published"
    );
    // A layout change copies the whole frame even when damage was given.
    device.write_damage(1, 2, 4, DRM_FORMAT_XRGB8888, &[3; 8], rect(0, 0, 1, 1));
    let (tag, changed) = damage(&path);
    assert_eq!((u64::from(tag), changed), (word(&path, 24), [0, 0, 1, 2]));
    drop((device, fb));
    std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
}

#[test]
fn a_frame_too_wide_for_the_damage_words_gets_a_tag_no_frame_has() {
    let path = test_path();
    let mut sink = FbSink::at_path(path.clone());
    let width = u32::from(u16::MAX) + 1;
    sink.write(
        width,
        1,
        width * 4,
        DRM_FORMAT_XRGB8888,
        &vec![0; width as usize * 4],
    );
    assert_eq!(word(&path, 24), 2);
    assert_eq!(damage(&path), (3, [0; 4]), "odd: readers copy it whole");
    drop(sink);
    std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
}
