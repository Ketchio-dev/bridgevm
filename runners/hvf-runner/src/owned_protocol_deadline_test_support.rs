use super::*;

pub(super) fn assert_late_frame_rejected() {
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
