//! Raw PCM file capture for the HDA playback stream.

use std::{
    fs::{File, OpenOptions},
    io::Write,
    path::Path,
};

use super::HdaPcmSink;

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
