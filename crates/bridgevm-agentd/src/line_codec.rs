//! Bounded newline-delimited envelope framing.

use crate::*;
use bridgevm_agent_protocol::AgentEnvelope;
use std::io::BufRead;
use std::io::Write;

/// Largest single newline-delimited frame the host will buffer from the agent
/// channel. Bounds host memory against a hostile guest that streams bytes
/// without a terminating newline (a sustained flood would otherwise grow the
/// read buffer until OOM). Sized to comfortably hold any legitimate frame
/// (capability list, a base64 file-drop chunk).
pub const MAX_FRAME_BYTES: usize = 1024 * 1024;

pub fn encode_envelope_line(envelope: &AgentEnvelope) -> Result<String, AgentCodecError> {
    envelope.validate().map_err(AgentCodecError::Protocol)?;
    let mut line = serde_json::to_string(envelope)
        .map_err(|error| AgentCodecError::Json(error.to_string()))?;
    line.push('\n');
    Ok(line)
}

pub fn decode_envelope_line(line: &str) -> Result<AgentEnvelope, AgentCodecError> {
    if line.is_empty() {
        return Err(AgentCodecError::EmptyFrame);
    }
    if !line.ends_with('\n') {
        return Err(AgentCodecError::MissingFrameTerminator);
    }

    let frame = line.trim_end_matches('\n').trim_end_matches('\r');
    if frame.trim().is_empty() {
        return Err(AgentCodecError::EmptyFrame);
    }
    if frame.contains('\n') {
        return Err(AgentCodecError::MultipleFrames);
    }

    let envelope: AgentEnvelope =
        serde_json::from_str(frame).map_err(|error| AgentCodecError::Json(error.to_string()))?;
    envelope.validate().map_err(AgentCodecError::Protocol)?;
    Ok(envelope)
}

/// Read a frame from a blocking reader. Use `EnvelopeLineReader` when idle
/// errors are retried, so consumed prefixes survive between calls.
pub fn read_envelope_line(
    reader: &mut impl BufRead,
) -> Result<Option<AgentEnvelope>, AgentCodecError> {
    EnvelopeLineReader::new(reader).read_envelope()
}

pub fn write_envelope_line(
    writer: &mut impl Write,
    envelope: &AgentEnvelope,
) -> Result<(), AgentCodecError> {
    let line = encode_envelope_line(envelope)?;
    writer
        .write_all(line.as_bytes())
        .map_err(|error| AgentCodecError::Io {
            kind: error.kind(),
            message: error.to_string(),
        })?;
    writer.flush().map_err(|error| AgentCodecError::Io {
        kind: error.kind(),
        message: error.to_string(),
    })
}
