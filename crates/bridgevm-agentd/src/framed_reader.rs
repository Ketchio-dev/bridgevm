//! Bounded envelope reader that retains partial frames across idle socket reads.

use crate::*;
use bridgevm_agent_protocol::AgentEnvelope;
use std::io::BufRead;
use std::io::ErrorKind;

pub struct EnvelopeLineReader<R> {
    reader: R,
    pending: Vec<u8>,
}

impl<R: BufRead> EnvelopeLineReader<R> {
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            pending: Vec::new(),
        }
    }

    /// Access the inner reader for socket writes or configuration. Direct reads
    /// bypass the pending framing state and must not be mixed with this reader.
    pub fn get_mut(&mut self) -> &mut R {
        &mut self.reader
    }

    pub fn read_envelope(&mut self) -> Result<Option<AgentEnvelope>, AgentCodecError> {
        loop {
            let available = match self.reader.fill_buf() {
                Ok(buffer) => buffer,
                Err(error) if error.kind() == ErrorKind::Interrupted => continue,
                Err(error) => {
                    return Err(AgentCodecError::Io {
                        kind: error.kind(),
                        message: error.to_string(),
                    });
                }
            };
            if available.is_empty() {
                if self.pending.is_empty() {
                    return Ok(None);
                }
                // EOF with pending bytes is a truncated frame, never idle.
                break;
            }
            let newline = available.iter().position(|&byte| byte == b'\n');
            let consumed = newline.map_or(available.len(), |index| index + 1);
            if consumed > MAX_FRAME_BYTES - self.pending.len() {
                return Err(AgentCodecError::FrameTooLarge);
            }
            self.pending.extend_from_slice(&available[..consumed]);
            self.reader.consume(consumed);
            if newline.is_some() {
                break;
            }
        }

        let line = String::from_utf8(std::mem::take(&mut self.pending)).map_err(|error| {
            AgentCodecError::Io {
                kind: ErrorKind::InvalidData,
                message: error.to_string(),
            }
        })?;
        decode_envelope_line(&line).map(Some)
    }
}
