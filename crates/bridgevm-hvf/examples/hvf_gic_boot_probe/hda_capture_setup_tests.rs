//! Which capture source the probe installs. No test opens a Mac input device.

use super::*;

fn never_open() -> Option<Box<dyn HdaPcmSource>> {
    panic!("the Mac microphone must not open");
}

/// A one-frame s16le 48 kHz stereo WAV, `ok`, or a mono one.
fn test_wav(name: &str, ok: bool) -> PathBuf {
    let channels: u16 = if ok { 2 } else { 1 };
    let mut fmt = vec![1, 0];
    fmt.extend_from_slice(&channels.to_le_bytes());
    fmt.extend_from_slice(&48_000u32.to_le_bytes());
    fmt.extend_from_slice(&(48_000 * 2 * u32::from(channels)).to_le_bytes());
    fmt.extend_from_slice(&(2 * channels).to_le_bytes());
    fmt.extend_from_slice(&16u16.to_le_bytes());
    let mut file = b"RIFF\0\0\0\0WAVEfmt \x10\0\0\0".to_vec();
    file.extend_from_slice(&fmt);
    file.extend_from_slice(b"data\x04\0\0\0\x01\x02\x03\x04");
    let path = std::env::temp_dir().join(format!("bridgevm-{name}-{}.wav", std::process::id()));
    std::fs::write(&path, file).unwrap();
    path
}

#[test]
fn a_test_wav_wins_and_the_microphone_never_opens() {
    let good = test_wav("capture-good", true);
    let bad = test_wav("capture-mono", false);
    // A test WAV with the microphone flag set: the WAV, refused or not.
    assert!(capture_source(Some(good.clone()), true, true, never_open).is_some());
    assert!(capture_source(Some(bad.clone()), true, true, never_open).is_none());
    // Without HDA, nothing; without either setting, nothing.
    assert!(capture_source(Some(good.clone()), true, false, never_open).is_none());
    assert!(capture_source(None, true, false, never_open).is_none());
    assert!(capture_source(None, false, true, never_open).is_none());
    std::fs::remove_file(good).unwrap();
    std::fs::remove_file(bad).unwrap();
    // Only the microphone flag on an HDA VM opens it.
    let mut opened = false;
    capture_source(None, true, true, || {
        opened = true;
        None
    });
    assert!(opened);
}
