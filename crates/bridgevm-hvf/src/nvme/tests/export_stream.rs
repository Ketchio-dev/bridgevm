use super::super::{DiskBackend, NvmeController, EXPORT_CHUNK_SIZE};
use super::export_staging::Fixture;
use std::{fs, io};

struct ShortWriter {
    bytes: Vec<u8>,
    limit: Option<usize>,
}
impl io::Write for ShortWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.limit == Some(self.bytes.len()) {
            return Err(io::Error::other("injected writer failure"));
        }
        let left = self.limit.map_or(bytes.len(), |n| n - self.bytes.len());
        let count = bytes.len().min(8191).min(left);
        self.bytes.extend_from_slice(&bytes[..count]);
        Ok(count)
    }
    fn flush(&mut self) -> io::Result<()> {
        panic!("streaming must not own publication or sync")
    }
}

#[test]
fn streamed_image_has_complete_chunks_tail_and_cross_chunk_overlay() {
    let f = Fixture::new();
    let source = f.0.join("source.raw");
    let mut expected = vec![0x31; EXPORT_CHUNK_SIZE + 512];
    fs::write(&source, &expected).unwrap();
    let mut disk = DiskBackend::raw_file(&source, false).unwrap();
    let offset = EXPORT_CHUNK_SIZE - 256;
    disk.write_at(offset as u64, &[0xa7; 512]).unwrap();
    expected[offset..offset + 512].fill(0xa7);
    let mut writer = ShortWriter {
        bytes: vec![],
        limit: None,
    };
    assert_eq!(
        disk.export_into(&mut writer).unwrap(),
        expected.len() as u64
    );
    assert_eq!(writer.bytes, expected);
    assert_eq!(fs::read(&source).unwrap(), vec![0x31; expected.len()]);
}

#[test]
fn stream_propagates_reader_and_writer_failures() {
    let f = Fixture::new();
    let source = f.0.join("source.raw");
    fs::write(&source, vec![0x41; EXPORT_CHUNK_SIZE + 512]).unwrap();
    let mut disk = DiskBackend::raw_file(&source, false).unwrap();
    let mut writer = ShortWriter {
        bytes: vec![],
        limit: Some(27),
    };
    assert!(disk.export_into(&mut writer).is_err());
    assert_eq!(writer.bytes, vec![0x41; 27]);
    fs::OpenOptions::new()
        .write(true)
        .open(&source)
        .unwrap()
        .set_len(EXPORT_CHUNK_SIZE as u64)
        .unwrap();
    let mut output = Vec::new();
    assert_eq!(
        disk.export_into(&mut output).unwrap_err().kind(),
        io::ErrorKind::UnexpectedEof
    );
    assert_eq!(output.len(), EXPORT_CHUNK_SIZE);
}

#[test]
fn namespace_streams_keep_memory_padding_and_missing_target_errors() {
    let mut controller = NvmeController::with_disk_image(vec![0x51; 513]);
    let mut output = Vec::new();
    assert_eq!(controller.export_disk_into(&mut output).unwrap(), 1024);
    assert_eq!(&output[..513], &[0x51; 513]);
    assert!(output[513..].iter().all(|b| *b == 0));
    output.clear();
    assert!(controller
        .export_second_namespace_into(&mut output)
        .is_err());
    assert!(output.is_empty());
    controller.attach_second_namespace(512);
    assert_eq!(
        controller
            .export_second_namespace_into(&mut output)
            .unwrap(),
        512
    );
    assert_eq!(output, vec![0; 512]);
}
