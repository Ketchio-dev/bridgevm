//! The test WAV capture source (BRIDGEVM_HDA_CAPTURE_TEST_WAV).

use super::super::*;
use super::helpers::temp_path;

/// A RIFF WAVE with a `fmt ` chunk of these fields, an odd-sized LIST chunk
/// (padded) between it and the data, and `data`.
fn wav(tag: u16, channels: u16, rate: u32, bits: u16, data: &[u8]) -> Vec<u8> {
    let mut fmt = Vec::new();
    fmt.extend_from_slice(&tag.to_le_bytes());
    fmt.extend_from_slice(&channels.to_le_bytes());
    fmt.extend_from_slice(&rate.to_le_bytes());
    let block_align = channels * bits / 8;
    fmt.extend_from_slice(&(rate * u32::from(block_align)).to_le_bytes());
    fmt.extend_from_slice(&block_align.to_le_bytes());
    fmt.extend_from_slice(&bits.to_le_bytes());
    let mut body = b"WAVE".to_vec();
    let chunks: [(&[u8; 4], &[u8]); 3] = [(b"fmt ", &fmt), (b"LIST", b"abc"), (b"data", data)];
    for (id, chunk) in chunks {
        body.extend_from_slice(id);
        body.extend_from_slice(&(chunk.len() as u32).to_le_bytes());
        body.extend_from_slice(chunk);
        if chunk.len() % 2 == 1 {
            body.push(0);
        }
    }
    let mut file = b"RIFF".to_vec();
    file.extend_from_slice(&(body.len() as u32).to_le_bytes());
    file.extend_from_slice(&body);
    file
}

#[test]
fn a_wav_plays_its_frames_in_a_loop_as_one_byte_stream() {
    let frames: Vec<u8> = (1..=12).collect();
    let mut source = WavPcmSource::parse(&wav(1, 2, 48_000, 16, &frames)).unwrap();
    assert_eq!(source.frames(), 3);
    // Reads need not align to frames or to the loop: the stream continues.
    let mut out = [0u8; 5];
    assert_eq!(source.read_pcm(&mut out, 48_000, 2, 16), 5);
    assert_eq!(out, [1, 2, 3, 4, 5]);
    let mut out = [0u8; 10];
    assert_eq!(source.read_pcm(&mut out, 48_000, 2, 16), 10);
    assert_eq!(out, [6, 7, 8, 9, 10, 11, 12, 1, 2, 3]);
    // Another stream format records silence and leaves the position.
    assert_eq!(source.read_pcm(&mut out, 44_100, 2, 16), 0);
    assert_eq!(source.read_pcm(&mut out[..1], 48_000, 2, 16), 1);
    assert_eq!(out[0], 4);
}

#[test]
fn only_s16le_48k_stereo_pcm_with_frames_is_accepted() {
    let frame = [0u8; 4];
    for (tag, channels, rate, bits) in [
        (3, 2, 48_000, 32),
        (1, 1, 48_000, 16),
        (1, 2, 44_100, 16),
        (1, 2, 48_000, 24),
    ] {
        let error = WavPcmSource::parse(&wav(tag, channels, rate, bits, &frame))
            .err()
            .unwrap();
        assert!(
            error.starts_with("PCM s16le 48000 Hz stereo only"),
            "{error}"
        );
    }
    let refused = |bytes: &[u8]| WavPcmSource::parse(bytes).err().unwrap();
    assert_eq!(refused(b"RIFF\0\0\0\0AVI "), "not a RIFF WAVE file");
    assert_eq!(
        refused(&wav(1, 2, 48_000, 16, &[1, 2, 3])),
        "no audio frames"
    );
    let mut no_data = wav(1, 2, 48_000, 16, &frame);
    no_data.truncate(no_data.len() - 12);
    assert_eq!(refused(&no_data), "no data chunk");
}

#[test]
fn a_data_chunk_longer_than_the_file_keeps_its_whole_frames() {
    let mut bytes = wav(1, 2, 48_000, 16, &[7; 8]);
    let size_at = bytes.len() - 12;
    bytes[size_at..size_at + 4].copy_from_slice(&u32::MAX.to_le_bytes());
    bytes.push(7); // a partial frame
    let path = temp_path("capture-test.wav");
    std::fs::write(&path, &bytes).unwrap();
    let source = WavPcmSource::open(&path).unwrap();
    std::fs::remove_file(&path).unwrap();
    assert_eq!(source.frames(), 2);
}
