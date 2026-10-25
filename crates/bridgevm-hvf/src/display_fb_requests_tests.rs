use super::{FRAME_REQUEST_MARKER, FRAME_REQUEST_MARKER_OFFSET, FRAME_REQUEST_OFFSET};
use crate::display_fb::DisplayFramebuffer;
use crate::ramfb::DRM_FORMAT_XRGB8888;
use crate::virtio_gpu::{FbSink, Rect};
use std::fs::OpenOptions;
use std::os::unix::fs::FileExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

fn test_path() -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "bridgevm-display-fb-requests-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("display.fb")
}

/// What a reader does: `pwrite` the request word into the header.
fn post(path: &Path, request: u64) {
    let file = OpenOptions::new().write(true).open(path).unwrap();
    let at = FRAME_REQUEST_OFFSET as u64;
    file.write_all_at(&request.to_le_bytes(), at).unwrap();
}

fn words(path: &Path) -> (u64, u32) {
    let file = std::fs::read(path).unwrap();
    let request = &file[FRAME_REQUEST_OFFSET..FRAME_REQUEST_OFFSET + 8];
    let marker = &file[FRAME_REQUEST_MARKER_OFFSET..FRAME_REQUEST_MARKER_OFFSET + 4];
    (
        u64::from_le_bytes(request.try_into().unwrap()),
        u32::from_le_bytes(marker.try_into().unwrap()),
    )
}

#[test]
fn a_writer_marks_its_file_and_reads_the_request_word_once_mapped() {
    let path = test_path();
    let mut fb = DisplayFramebuffer::at_path(&path);
    assert!(!fb.accept_frame_requests(), "nothing is mapped yet");
    assert_eq!(fb.frame_request(), None);
    fb.publish(1, 1, 4, DRM_FORMAT_XRGB8888, &[1, 2, 3, 4])
        .unwrap();
    assert_eq!(words(&path), (0, 0), "a plain publish takes no requests");
    assert_eq!(fb.frame_request(), Some(0));
    assert!(fb.accept_frame_requests());
    assert_eq!(words(&path), (0, FRAME_REQUEST_MARKER));
    post(&path, 0x1234_5678_9abc);
    assert_eq!(fb.frame_request(), Some(0x1234_5678_9abc));
    std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
}

#[test]
fn the_virtio_gpu_sink_neither_marks_nor_overwrites_the_request_words() {
    let path = test_path();
    let mut sink = FbSink::at_path(path.clone());
    sink.write(1, 1, 4, DRM_FORMAT_XRGB8888, &[1, 2, 3, 4]);
    assert_eq!(words(&path), (0, 0), "the device's sink takes no requests");
    // A file that a ramfb writer once marked and a reader asked through.
    post(&path, 5);
    let file = OpenOptions::new().write(true).open(&path).unwrap();
    let marker = FRAME_REQUEST_MARKER.to_le_bytes();
    file.write_all_at(&marker, FRAME_REQUEST_MARKER_OFFSET as u64)
        .unwrap();
    sink.write(1, 1, 4, DRM_FORMAT_XRGB8888, &[5, 6, 7, 8]);
    let whole = Rect {
        x: 0,
        y: 0,
        width: 1,
        height: 1,
    };
    sink.write_damage(1, 1, 4, DRM_FORMAT_XRGB8888, &[9; 4], whole);
    assert_eq!(words(&path), (5, FRAME_REQUEST_MARKER));
    let file = std::fs::read(&path).unwrap();
    assert_eq!(
        (
            &file[64..],
            u64::from_le_bytes(file[24..32].try_into().unwrap())
        ),
        (&[9u8; 4][..], 6)
    );
    drop(sink);
    std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
}

#[test]
fn publishing_never_writes_the_request_words() {
    let path = test_path();
    let mut fb = DisplayFramebuffer::at_path(&path);
    fb.publish(1, 1, 4, DRM_FORMAT_XRGB8888, &[1, 2, 3, 4])
        .unwrap();
    assert!(fb.accept_frame_requests());
    post(&path, 77);
    fb.publish(1, 1, 4, DRM_FORMAT_XRGB8888, &[5, 6, 7, 8])
        .unwrap();
    assert_eq!(words(&path), (77, FRAME_REQUEST_MARKER));
    // A larger frame remaps the file in place; the words stay.
    fb.publish(4, 4, 16, DRM_FORMAT_XRGB8888, &[9; 64]).unwrap();
    assert_eq!(words(&path), (77, FRAME_REQUEST_MARKER));
    assert_eq!(fb.frame_request(), Some(77));
    // The next process's writer continues the file and sees them too.
    drop(fb);
    let mut next = DisplayFramebuffer::at_path(&path);
    next.publish(4, 4, 16, DRM_FORMAT_XRGB8888, &[3; 64])
        .unwrap();
    assert_eq!(words(&path), (77, FRAME_REQUEST_MARKER));
    std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
}
