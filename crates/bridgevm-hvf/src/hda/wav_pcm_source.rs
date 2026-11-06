//! A looped WAV file as the HDA capture source, for tests:
//! `BRIDGEVM_HDA_CAPTURE_TEST_WAV`. It never opens a Mac input device.

use std::fs::File;
use std::io::Read;
use std::path::Path;

use super::HdaPcmSource;

/// The largest test WAV read: about 5.8 minutes of 48 kHz stereo s16le.
const MAX_WAV_BYTES: u64 = 64 << 20;
const FRAME_BYTES: usize = 4;

/// Signed 16-bit little-endian 48 kHz stereo PCM from a WAV file, played in a
/// loop while the guest's capture stream runs. That is the only format the
/// codec's input converter offers; a stream programmed otherwise records
/// silence.
pub struct WavPcmSource {
    pcm: Vec<u8>,
    position: usize,
}

impl WavPcmSource {
    pub fn open(path: &Path) -> Result<Self, String> {
        let mut bytes = Vec::new();
        File::open(path)
            .and_then(|file| file.take(MAX_WAV_BYTES + 1).read_to_end(&mut bytes))
            .map_err(|error| error.to_string())?;
        if bytes.len() as u64 > MAX_WAV_BYTES {
            return Err(format!("larger than {} MiB", MAX_WAV_BYTES >> 20));
        }
        Self::parse(&bytes)
    }

    /// A RIFF WAVE whose `fmt ` chunk is PCM (format tag 1), 2 channels,
    /// 48000 Hz and 16 bits, followed by a `data` chunk with at least one
    /// frame. Other chunks are skipped; a data chunk that claims more bytes
    /// than the file holds, as a WAV written while recording may, keeps the
    /// whole frames that are there.
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() < 12 || &bytes[..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
            return Err("not a RIFF WAVE file".into());
        }
        let mut format_ok = None;
        let mut offset = 12;
        while offset + 8 <= bytes.len() {
            let size = u32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().unwrap());
            let body = offset + 8;
            let end = body
                .checked_add(size as usize)
                .filter(|end| *end <= bytes.len());
            match &bytes[offset..offset + 4] {
                b"fmt " => {
                    let fmt = &bytes[body..end.filter(|_| size >= 16).ok_or("short fmt chunk")?];
                    let u16_at = |at: usize| u16::from_le_bytes([fmt[at], fmt[at + 1]]);
                    let rate = u32::from_le_bytes(fmt[4..8].try_into().unwrap());
                    let (tag, channels, bits) = (u16_at(0), u16_at(2), u16_at(14));
                    format_ok = Some(if (tag, channels, rate, bits) == (1, 2, 48_000, 16) {
                        Ok(())
                    } else {
                        Err(format!(
                            "PCM s16le 48000 Hz stereo only; this is format {tag}, \
                             {channels} channels, {rate} Hz, {bits} bits"
                        ))
                    });
                }
                b"data" => {
                    format_ok.ok_or("no fmt chunk before the data")??;
                    let end = end.unwrap_or(bytes.len());
                    let whole = (end - body) / FRAME_BYTES * FRAME_BYTES;
                    if whole == 0 {
                        return Err("no audio frames".into());
                    }
                    let pcm = bytes[body..body + whole].to_vec();
                    return Ok(Self { pcm, position: 0 });
                }
                _ => {}
            }
            // Chunks are padded to an even length.
            offset = end.ok_or("truncated chunk")? + (size as usize & 1);
        }
        Err("no data chunk".into())
    }

    pub fn frames(&self) -> usize {
        self.pcm.len() / FRAME_BYTES
    }
}

impl HdaPcmSource for WavPcmSource {
    fn read_pcm(&mut self, out: &mut [u8], rate: u32, channels: u8, bits: u8) -> usize {
        if (rate, channels, bits) != (48_000, 2, 16) {
            return 0;
        }
        let mut filled = 0;
        while filled < out.len() {
            let run = (out.len() - filled).min(self.pcm.len() - self.position);
            out[filled..filled + run]
                .copy_from_slice(&self.pcm[self.position..self.position + run]);
            filled += run;
            self.position = (self.position + run) % self.pcm.len();
        }
        filled
    }
}
