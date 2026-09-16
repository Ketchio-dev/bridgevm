use super::*;
use std::io::Write;
use std::os::unix::net::UnixStream;
#[test]
fn oversized_truncated_and_late_frames_fail_without_allocation_growth() {
    for bytes in [vec![0, 0, 32, 1], vec![0, 0, 0, 0], vec![0, 0, 0, 5, b'{']] {
        let (mut writer, reader) = UnixStream::pair().unwrap();
        reader.set_nonblocking(true).unwrap();
        writer.write_all(&bytes).unwrap();
        drop(writer);
        let mut frames = Reader::new();
        let mut rejected = false;
        for _ in 0..4 {
            if frames.step(reader.as_raw_fd()).is_err() {
                rejected = true;
                break;
            }
        }
        assert!(rejected);
        assert!(frames.bytes.len() <= MAX_FRAME + 4);
    }
    let (mut writer, reader) = UnixStream::pair().unwrap();
    reader.set_nonblocking(true).unwrap();
    writer.write_all(&[0, 0, 0, 2, b'{', b'}']).unwrap();
    let mut frames = Reader::new();
    assert!(matches!(
        frames.step(reader.as_raw_fd()).unwrap(),
        ReadResult::Pending
    ));
    frames.started = Some(Instant::now() - IO_BOUND);
    assert!(frames.step(reader.as_raw_fd()).is_err());
}
