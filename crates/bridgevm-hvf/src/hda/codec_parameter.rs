//! Split out of hda.rs to keep files under 850 lines.

use super::*;

use std::sync::OnceLock;

pub(crate) fn codec_parameter(nid: u8, parameter: u8) -> u32 {
    if nid == CODEC_AFG {
        return afg_parameter(parameter).unwrap_or(0);
    }
    match (nid, parameter) {
        (CODEC_ROOT, 0x00) => CODEC_VENDOR_ID,
        // AC_PAR_SUBSYSTEM_ID (0x01). QEMU's output codec exposes it on the
        // root AND the audio function group; hdaudio.sys queries it during
        // enumeration and treats a missing/zero value as an invalid codec,
        // aborting before it reads the AFG's SUBORDINATE_NODE_COUNT (0x04) —
        // which is exactly the "reads AFG basics then never descends" wall.
        (CODEC_ROOT, 0x01) => CODEC_IMPLEMENTATION_ID,
        (CODEC_ROOT, 0x02) => CODEC_REVISION_ID,
        (CODEC_ROOT, 0x04) => 0x0001_0001, // node 1, one function group.
        (CODEC_DAC, 0x0a) => CODEC_PCM_SIZE_RATES,
        (CODEC_DAC, 0x0b) => CODEC_STREAM_FORMATS,
        (CODEC_DAC, 0x0f) | (CODEC_SPEAKER, 0x0f) => CODEC_WIDGET_POWER_STATES,
        (CODEC_DAC, 0x09) => DAC_WIDGET_CAPABILITIES,
        (CODEC_DAC, 0x0d) => 0,
        (CODEC_DAC, 0x12) => CODEC_OUTPUT_AMP_CAPS,
        (CODEC_SPEAKER, 0x09) => SPEAKER_WIDGET_CAPABILITIES,
        (CODEC_SPEAKER, 0x0c) => SPEAKER_PIN_CAPABILITIES,
        (CODEC_SPEAKER, 0x0d | 0x12) => 0,
        (CODEC_SPEAKER, 0x0e) => 1, // one short-form connection.
        _ => 0,
    }
}

pub(crate) fn afg_parameter(parameter: u8) -> Option<u32> {
    Some(match parameter {
        0x01 => CODEC_IMPLEMENTATION_ID, // AC_PAR_SUBSYSTEM_ID (QEMU AFG has it)
        0x04 => CODEC_AFG_CHILD_NODE_COUNT, // NID 2 DAC, NID 3 pin
        0x05 => 0x0000_0001,             // audio function group
        0x08 => CODEC_AFG_CAPABILITIES,
        0x0a => CODEC_PCM_SIZE_RATES,
        0x0b => CODEC_STREAM_FORMATS,
        0x0d => 0, // default input amp capabilities: none
        0x0f => CODEC_AFG_POWER_STATES,
        0x11 => 0, // GPIO count: none
        0x12 => 0, // default output amp capabilities: none
        _ => return None,
    })
}

pub(crate) fn power_state_response(state: u8) -> u32 {
    u32::from(state) | (u32::from(state) << 4)
}

pub(crate) fn ring_entries(size: u8) -> u16 {
    match size & 0x03 {
        0 => 2,
        1 => 16,
        _ => 256,
    }
}

pub(crate) fn stream_sample_rate(fmt: u16) -> Option<u32> {
    if fmt & 0x8000 != 0 {
        return None;
    }
    let base = if fmt & 0x4000 != 0 { 44_100 } else { 48_000 };
    let multiplier = u32::from((fmt >> 11) & 0x7) + 1;
    let divisor = u32::from((fmt >> 8) & 0x7) + 1;
    Some(base * multiplier / divisor)
}

pub(crate) fn stream_frame_bytes(fmt: u16) -> Option<u16> {
    // The codec advertises 16-bit PCM only, which keeps the capture file's
    // promised raw s16le format true even if a guest programs an invalid FMT.
    let sample_bytes = ((fmt >> 4) & 0x7 == 1).then_some(2)?;
    let channels = (fmt & 0x0f) + 1;
    Some(sample_bytes * channels)
}

pub(crate) fn stream_pcm_format(fmt: u16) -> Option<(u32, u8, u8)> {
    let rate = stream_sample_rate(fmt)?;
    let bits = ((fmt >> 4) & 0x7 == 1).then_some(16)?;
    let channels = u8::try_from((fmt & 0x0f) + 1).ok()?;
    Some((rate, channels, bits))
}

pub(crate) fn stream_bytes_per_second(fmt: u16) -> Option<u64> {
    Some(u64::from(stream_sample_rate(fmt)?) * u64::from(stream_frame_bytes(fmt)?))
}

pub(crate) fn hda_trace_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("BRIDGEVM_TRACE_HDA")
            .ok()
            .is_some_and(|value| matches!(value.trim(), "1" | "true" | "yes" | "on"))
    })
}
