//! One BDL entry's progress: each DMA chunk goes to the host sink and then to
//! the stale-replay diagnostics; a finished entry raises IOC when flagged and
//! advances; the cursor wraps at CBL, and the position buffer follows LPIB.

use super::*;

impl HdaController {
    /// Hand one DMA chunk (`pcm_scratch`, at the current entry cursor) to the
    /// stale-replay diagnostics as the guest wrote it, then to the host sink at
    /// the DAC's output amp level.
    pub(crate) fn emit_pcm_chunk(&mut self, length: u32, (rate, channels, bits): (u32, u8, u8)) {
        let (index, offset) = (self.stream.bdl_index, self.stream.bdl_offset);
        self.diagnostics
            .repeats
            .note_chunk(index, offset, length, &self.pcm_scratch);
        if let Some(output) = self.pcm_sink.as_mut() {
            apply_output_amp(
                &mut self.pcm_scratch,
                channels,
                bits,
                self.codec.dac_amp_gain_mute,
            );
            output.write_pcm(&self.pcm_scratch, rate, channels, bits);
        }
    }

    pub(crate) fn complete_bdl_entry(&mut self, flags: u32) {
        if flags & BDL_IOC != 0 {
            self.stream.sts |= SDSTS_BCIS;
            self.diagnostics.note_ioc();
            if hda_trace_enabled() {
                println!(
                    "hda: BDL IOC index={} lpib={}",
                    self.stream.bdl_index, self.stream.lpib
                );
            }
        }
        self.stream.bdl_offset = 0;
        self.stream.bdl_index = if self.stream.bdl_index >= self.stream.lvi {
            0
        } else {
            self.stream.bdl_index + 1
        };
    }

    pub(crate) fn wrap_cyclic_buffer(&mut self) {
        self.stream.lpib = 0;
        self.stream.bdl_index = 0;
        self.stream.bdl_offset = 0;
    }

    pub(crate) fn write_position_buffer(&self, mem: &mut dyn GuestMemoryMut) {
        if self.dp_base & 1 == 0 {
            return;
        }
        let base = self.dp_base & !0x7f;
        let entry = base.wrapping_add(self.playback_position_entry());
        let _ = mem.write_bytes(entry, &self.stream.lpib.to_le_bytes());
    }
}

/// The DAC's output amp as SET_AMP_GAIN_MUTE left it, per channel (bit 7 mute, bits 6:0 the step;
/// CODEC_OUTPUT_AMP_CAPS: 1 dB steps, 0 dB at 0x4a). Windows sets its main volume and mute there,
/// not in the samples, so without this they changed nothing. Chunks start on a frame: the budget,
/// the guest's buffers and the cyclic buffer are whole frames.
pub(crate) fn apply_output_amp(samples: &mut [u8], channels: u8, bits: u8, gain_mute: [u8; 2]) {
    const ZERO_DB: u8 = 0x4a;
    if bits != 16 || gain_mute == [ZERO_DB; 2] {
        return;
    }
    let factor = |value: u8| -> i64 {
        if value & 0x80 != 0 {
            return 0;
        }
        let db = f64::from(value.min(ZERO_DB)) - f64::from(ZERO_DB);
        (10f64.powf(db / 20.0) * 65536.0).round() as i64
    };
    let factors = [factor(gain_mute[0]), factor(gain_mute[1])];
    let channels = usize::from(channels.max(1));
    for (index, sample) in samples.chunks_exact_mut(2).enumerate() {
        let value = i64::from(i16::from_le_bytes([sample[0], sample[1]]));
        let scaled = (value * factors[(index % channels).min(1)]) >> 16;
        sample.copy_from_slice(&(scaled as i16).to_le_bytes());
    }
}
