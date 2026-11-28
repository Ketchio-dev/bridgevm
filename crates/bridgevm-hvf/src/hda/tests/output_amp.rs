//! Windows sets its main volume and mute on the DAC's output amp (SET_AMP_GAIN_MUTE): the samples
//! reaching the Mac follow it, per channel.

use super::super::*;
use super::helpers::*;
use crate::fwcfg::GuestMemoryMut;
use crate::platform_virt::FlatGuestRam;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

fn samples(values: &[i16]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect()
}

/// One stereo stream of `pcm` through the controller with the DAC amp set by `amp` (verb payloads).
fn played(amp: &[u16], pcm: &[i16]) -> Vec<i16> {
    let writes = Arc::new(Mutex::new(Vec::new()));
    let mut ctrl = HdaController::with_pcm_output_path::<&Path>(None);
    ctrl.set_pcm_sink(Some(Box::new(RecordingPcmSink {
        writes: Arc::clone(&writes),
    })));
    for &payload in amp {
        assert_eq!(ctrl.codec_verb(verb16(0, CODEC_DAC, 0x3, payload)), 0);
    }
    let mut mem = FlatGuestRam::new(RAM_BASE, 0x10000);
    let (bdl, data) = (RAM_BASE + 0x1000, RAM_BASE + 0x2000);
    let bytes = samples(pcm);
    assert!(mem.write_bytes(data, &bytes));
    let mut descriptor = [0u8; 16];
    descriptor[..8].copy_from_slice(&data.to_le_bytes());
    descriptor[8..12].copy_from_slice(&(bytes.len() as u32).to_le_bytes());
    assert!(mem.write_bytes(bdl, &descriptor));
    write(&mut ctrl, &mut mem, REG_GCTL, 4, 1);
    write(&mut ctrl, &mut mem, REG_SD_BDPL, 4, bdl);
    write(&mut ctrl, &mut mem, REG_SD_CBL, 4, bytes.len() as u64);
    write(&mut ctrl, &mut mem, REG_SD_LVI, 2, 0);
    write(&mut ctrl, &mut mem, REG_SD_FMT, 2, 0x0011);
    write(&mut ctrl, &mut mem, REG_SD_CTL, 1, u64::from(SDCTL_RUN));
    ctrl.poll_for_duration(&mut mem, Duration::from_millis(1));
    let out: Vec<u8> = writes
        .lock()
        .unwrap()
        .iter()
        .flat_map(|w| w.samples.clone())
        .collect();
    out.chunks_exact(2)
        .map(|s| i16::from_le_bytes([s[0], s[1]]))
        .collect()
}

#[test]
fn windows_volume_and_mute_reach_the_samples_sent_to_the_mac() {
    let pcm: Vec<i16> = [16000, -16000, 32767, -32768].repeat(24); // 1 ms: 192 bytes
    assert_eq!(
        played(&[], &pcm),
        pcm,
        "0 dB, the reset level, passes samples unchanged"
    );
    // Output amp, left and right, mute.
    assert!(played(&[0xb080], &pcm).iter().all(|&s| s == 0), "mute");
    // Step 0x44 is -6 dB: about half.
    let quieter = played(&[0xb044], &pcm);
    assert_eq!(&quieter[..4], &[8019, -8020, 16422, -16423]);
    // Left at -20 dB (0x36), right muted: each channel follows its own amp.
    let balanced = played(&[0xa036, 0x9080], &pcm);
    assert_eq!(&balanced[..4], &[1600, 0, 3276, 0]);
    // The amp reads back as Windows set it.
    let mut ctrl = HdaController::with_pcm_output_path::<&Path>(None);
    assert_eq!(ctrl.codec_verb(verb16(0, CODEC_DAC, 0x3, 0xb044)), 0);
    assert_eq!(ctrl.codec_verb(verb16(0, CODEC_DAC, 0xb, 0xa000)), 0x44);
}
