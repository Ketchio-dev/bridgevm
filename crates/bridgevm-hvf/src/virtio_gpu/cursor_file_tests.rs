use super::*;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

pub(crate) fn temp_cursor_path(label: &str) -> std::path::PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    std::env::temp_dir().join(format!(
        "bridgevm-cursor-{label}-{}-{}.cursor",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}

/// What a reader finds in the file: (sequence, state, publish time, image).
pub(crate) fn read_cursor_file(path: &Path) -> (u64, CursorState, u64, Vec<u8>) {
    let bytes = std::fs::read(path).unwrap();
    assert_eq!(bytes.len(), CURSOR_FILE_LEN);
    let word = |offset: usize| u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
    let quad = |offset: usize| u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap());
    assert_eq!(
        [word(0), word(4), word(8), word(12), word(16), word(20)],
        [CURSOR_FILE_MAGIC, 1, 64, 64, 256, CURSOR_FOURCC]
    );
    let flags = word(36);
    let state = CursorState {
        serial: word(32),
        visible: flags & FLAG_VISIBLE != 0,
        shape_this_boot: flags & FLAG_SHAPE_THIS_BOOT != 0,
        x: word(40) as i32,
        y: word(44) as i32,
        hot_x: word(56),
        hot_y: word(60),
    };
    (
        quad(24),
        state,
        quad(48),
        bytes[CURSOR_HEADER_LEN..].to_vec(),
    )
}

fn state(serial: u32, x: i32, y: i32) -> CursorState {
    CursorState {
        serial,
        visible: true,
        shape_this_boot: true,
        x,
        y,
        hot_x: 3,
        hot_y: 9,
    }
}

#[test]
fn publish_writes_header_and_image_under_an_even_sequence() {
    let path = temp_cursor_path("publish");
    let (mut file, serial) = CursorFile::open(&path).unwrap();
    assert_eq!(serial, 0, "a new file has no shape");
    let image: Vec<u8> = (0..CURSOR_IMAGE_BYTES).map(|i| i as u8).collect();
    file.publish(&state(1, -4, 700), Some(&image));
    let (seq, published, time, pixels) = read_cursor_file(&path);
    assert_eq!(seq, 2);
    assert_eq!(published, state(1, -4, 700));
    assert!(time > 0, "the publish time is stamped");
    assert_eq!(pixels, image);

    // A state without an image leaves the image alone.
    file.publish(&state(1, 5, 6), None);
    let (seq, published, _, pixels) = read_cursor_file(&path);
    assert_eq!((seq, published.x, published.y), (4, 5, 6));
    assert_eq!(pixels, image);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn a_later_writer_continues_the_sequence_and_the_shape_serial() {
    let path = temp_cursor_path("continue");
    let (mut first, _) = CursorFile::open(&path).unwrap();
    first.publish(&state(7, 1, 1), Some(&[1; CURSOR_IMAGE_BYTES]));
    first.publish(&state(7, 2, 2), None);
    drop(first);
    let (mut second, serial) = CursorFile::open(&path).unwrap();
    assert_eq!(serial, 7);
    second.publish(&state(serial, 3, 3), None);
    assert_eq!(
        read_cursor_file(&path).0,
        6,
        "never back to a sequence a reader saw"
    );
    std::fs::remove_file(path).unwrap();
}

#[test]
fn open_does_not_follow_a_symlink() {
    let path = temp_cursor_path("symlink");
    let target = temp_cursor_path("target");
    std::fs::write(&target, b"not a cursor").unwrap();
    std::os::unix::fs::symlink(&target, &path).unwrap();
    assert!(CursorFile::open(&path).is_err());
    assert_eq!(std::fs::read(&target).unwrap(), b"not a cursor");
    std::fs::remove_file(path).unwrap();
    std::fs::remove_file(target).unwrap();
}

/// A reader that follows the seqlock (even sequence, the same sequence
/// after its copy) never takes a state whose fields and image disagree.
#[test]
fn a_concurrent_seqlock_reader_never_takes_a_torn_state() {
    let path = temp_cursor_path("seqlock");
    let (mut file, _) = CursorFile::open(&path).unwrap();
    file.publish(&state(0, 0, 0), Some(&[0; CURSOR_IMAGE_BYTES]));
    let done = Arc::new(AtomicBool::new(false));
    let writer_done = Arc::clone(&done);
    let writer = std::thread::spawn(move || {
        for serial in 1..=3000u32 {
            let image = [serial as u8; CURSOR_IMAGE_BYTES];
            file.publish(
                &state(serial, serial as i32, -(serial as i32)),
                Some(&image),
            );
            // Leave the reader windows between states, as a 120 Hz guest does.
            let pause = std::time::Instant::now();
            while pause.elapsed() < std::time::Duration::from_micros(20) {
                std::hint::spin_loop();
            }
        }
        writer_done.store(true, Ordering::Release);
    });

    let fd = std::fs::File::open(&path).unwrap();
    let map = unsafe {
        libc::mmap(
            std::ptr::null_mut(),
            CURSOR_FILE_LEN,
            libc::PROT_READ,
            libc::MAP_SHARED,
            std::os::unix::io::AsRawFd::as_raw_fd(&fd),
            0,
        )
    } as *const u8;
    assert_ne!(map as *mut libc::c_void, libc::MAP_FAILED);
    let sequence = unsafe { &*(map.add(24) as *const AtomicU64) };
    // One seqlock attempt: the serial of a consistent state, or None to retry.
    let attempt = || {
        let before = sequence.load(Ordering::Acquire);
        if before & 1 == 1 {
            return None;
        }
        let mut copy = vec![0u8; CURSOR_FILE_LEN];
        unsafe { std::ptr::copy_nonoverlapping(map, copy.as_mut_ptr(), CURSOR_FILE_LEN) };
        fence(Ordering::Acquire);
        if sequence.load(Ordering::Acquire) != before {
            return None;
        }
        let word = |offset: usize| u32::from_le_bytes(copy[offset..offset + 4].try_into().unwrap());
        let serial = word(32);
        assert_eq!(word(40) as i32, serial as i32);
        assert_eq!(word(44) as i32, -(serial as i32));
        assert!(copy[CURSOR_HEADER_LEN..]
            .iter()
            .all(|byte| *byte == serial as u8));
        Some(serial)
    };
    let (mut taken, mut retried) = (0u32, 0u32);
    while !done.load(Ordering::Acquire) {
        match attempt() {
            Some(_) => taken += 1,
            None => retried += 1,
        }
    }
    writer.join().unwrap();
    assert_eq!(
        attempt(),
        Some(3000),
        "the last state, once the writer is done"
    );
    unsafe { libc::munmap(map as *mut libc::c_void, CURSOR_FILE_LEN) };
    eprintln!("seqlock reader: {taken} states taken, {retried} retries while writing");
    std::fs::remove_file(path).unwrap();
}
