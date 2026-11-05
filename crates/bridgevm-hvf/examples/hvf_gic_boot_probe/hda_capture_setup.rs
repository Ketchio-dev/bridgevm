//! The HDA capture source the probe installs for the guest's microphone.
//!
//! - `BRIDGEVM_HDA_CAPTURE_TEST_WAV=<file>` plays a WAV file in a loop. It
//!   wins over the microphone, so a test run never opens a Mac input device.
//! - `BRIDGEVM_HDA_MICROPHONE=1` records the Mac's default input while the
//!   guest captures (hda_coreaudio/hda_microphone.rs). The app passes it only
//!   when the VM's microphone setting is on and macOS allowed access.
//!
//! Without a source the guest's microphone records silence. A source that
//! cannot start leaves silence and one line saying why; the VM runs on.
//!
//! The input stream and the microphone pin exist only when one of these is set
//! at launch. Otherwise the guest sees main's HDA device (GCAP 0x1001, playback
//! as SD0, a codec without ADC or mic pin), so a VM with the setting off is
//! unchanged.

use super::*;
use bridgevm_hvf::hda::{HdaPcmSource, WavPcmSource};

pub(crate) fn install_hda_capture(platform: &mut VirtPlatform, hda_present: bool) {
    let test_wav = std::env::var_os("BRIDGEVM_HDA_CAPTURE_TEST_WAV")
        .filter(|path| !path.is_empty())
        .map(PathBuf::from);
    let microphone = env_flag("BRIDGEVM_HDA_MICROPHONE");
    let layout = microphone_layout(test_wav.is_some(), microphone, hda_present);
    platform.set_hda_microphone(layout);
    if hda_present {
        println!(
            "HDA layout: {}",
            if layout {
                "microphone (GCAP 0x1101, capture SD0, playback SD1)"
            } else {
                "no microphone (GCAP 0x1001, playback SD0)"
            }
        );
    }
    if let Some(source) = capture_source(test_wav, microphone, hda_present, open_microphone) {
        platform.set_hda_pcm_source(Some(source));
    }
}

/// Whether the guest gets the input stream and the microphone pin: only with
/// HDA and the microphone setting or a test WAV at launch.
fn microphone_layout(test_wav: bool, microphone: bool, hda_present: bool) -> bool {
    hda_present && (test_wav || microphone)
}

/// The source for these settings. A test WAV wins, refused or not, so a test
/// run never calls `open_microphone`.
fn capture_source(
    test_wav: Option<PathBuf>,
    microphone: bool,
    hda_present: bool,
    open_microphone: impl FnOnce() -> Option<Box<dyn HdaPcmSource>>,
) -> Option<Box<dyn HdaPcmSource>> {
    if test_wav.is_none() && !microphone {
        return None;
    }
    if !hda_present {
        eprintln!("HDA capture ignored because BRIDGEVM_HDA is not enabled");
        return None;
    }
    let Some(path) = test_wav else {
        return open_microphone();
    };
    if microphone {
        println!("HDA capture: BRIDGEVM_HDA_MICROPHONE ignored; the test WAV is the source");
    }
    match WavPcmSource::open(&path) {
        Ok(source) => {
            println!(
                "HDA capture: test WAV {}, {} frames, looped",
                path.display(),
                source.frames()
            );
            Some(Box::new(source))
        }
        Err(error) => {
            eprintln!(
                "HDA capture: test WAV {} refused: {error}; the guest microphone records silence",
                path.display()
            );
            None
        }
    }
}

#[cfg(target_os = "macos")]
fn open_microphone() -> Option<Box<dyn HdaPcmSource>> {
    use hda_coreaudio::hda_microphone::{open_default_input, MicrophoneSource, STOP_GRACE};
    match MicrophoneSource::new(Box::new(open_default_input), STOP_GRACE) {
        Ok(source) => {
            println!("HDA capture: the Mac's default input, open while the guest records");
            Some(Box::new(source))
        }
        Err(error) => {
            eprintln!(
                "HDA capture: the Mac microphone is off ({error}); the guest microphone records silence"
            );
            None
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn open_microphone() -> Option<Box<dyn HdaPcmSource>> {
    eprintln!(
        "BRIDGEVM_HDA_MICROPHONE is only supported on macOS; the guest microphone records silence"
    );
    None
}

#[cfg(test)]
#[path = "hda_capture_setup_tests.rs"]
mod tests;
