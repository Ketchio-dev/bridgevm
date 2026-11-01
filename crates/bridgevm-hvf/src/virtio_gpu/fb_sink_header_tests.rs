use super::{uptime_ns, PUBLISH_TIME_OFFSET};
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
