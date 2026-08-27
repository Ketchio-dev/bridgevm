//! Host PCM destinations for the HDA playback stream.

use std::{
    fs::{File, OpenOptions},
    io::Write,
    path::Path,
};

/// Host-provided destination for decoded interleaved PCM stream bytes.
///
/// Implementations run on the vCPU thread while the platform lock is held, so
/// live sinks must return promptly and move potentially blocking work elsewhere.
pub trait HdaPcmSink: Send {
    fn write_pcm(&mut self, samples: &[u8], rate: u32, channels: u8, bits: u8);
}

/// Raw PCM file sink used by `BRIDGEVM_HDA_PCM_OUT`.
pub struct FilePcmSink {
    pub(crate) file: Option<File>,
}

impl FilePcmSink {
    pub fn create<P: AsRef<Path>>(path: P) -> std::io::Result<Self> {
        let file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(path)?;
        Ok(Self { file: Some(file) })
    }
}

impl HdaPcmSink for FilePcmSink {
    fn write_pcm(&mut self, samples: &[u8], _rate: u32, _channels: u8, _bits: u8) {
        let Some(file) = self.file.as_mut() else {
            return;
        };
        if let Err(error) = file.write_all(samples) {
            eprintln!("hda: disabling PCM capture after write error: {error}");
            self.file = None;
        }
    }
}
