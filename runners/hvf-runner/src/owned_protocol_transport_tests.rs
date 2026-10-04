use super::*;
use std::io::Write;
use std::os::unix::net::UnixStream;
#[path = "owned_protocol_deadline_test_support.rs"]
mod deadline_test_support;
#[test]
fn oversized_truncated_and_late_frames_fail_without_allocation_growth() {
    for bytes in [vec![0, 0, 32, 1], vec![0, 0, 0, 0], vec![0, 0, 0, 5, b'{']] {
        let (mut writer, reader) = UnixStream::pair().unwrap();
        reader.set_nonblocking(true).unwrap();
        writer.write_all(&bytes).unwrap();
        let _retained_writer_alias = writer.try_clone().unwrap();
        writer.shutdown(std::net::Shutdown::Write).unwrap();
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
    deadline_test_support::assert_late_frame_rejected();
}
