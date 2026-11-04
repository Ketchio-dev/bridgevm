//! The HDA capture source the probe installs for the guest's microphone.
//!
//! `BRIDGEVM_HDA_CAPTURE_TEST_WAV=<file>` plays a WAV file in a loop and never
//! opens a Mac input device. Without a source the guest's microphone records
//! silence. A source that cannot start leaves silence and one line saying why;
//! the VM runs on.

use super::*;
use bridgevm_hvf::hda::WavPcmSource;

pub(crate) fn install_hda_capture(platform: &mut VirtPlatform, hda_present: bool) {
    let Some(path) = std::env::var_os("BRIDGEVM_HDA_CAPTURE_TEST_WAV").filter(|p| !p.is_empty())
    else {
        return;
    };
    let path = PathBuf::from(path);
    if !hda_present {
        eprintln!("BRIDGEVM_HDA_CAPTURE_TEST_WAV ignored because BRIDGEVM_HDA is not enabled");
        return;
    }
    match WavPcmSource::open(&path) {
        Ok(source) => {
            println!(
                "HDA capture: test WAV {}, {} frames, looped",
                path.display(),
                source.frames()
            );
            platform.set_hda_pcm_source(Some(Box::new(source)));
        }
        Err(error) => eprintln!(
            "HDA capture: test WAV {} refused: {error}; the guest microphone records silence",
            path.display()
        ),
    }
}
