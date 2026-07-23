//! Bounded newline-delimited envelope framing.

use crate::*;
use bridgevm_agent_protocol::AgentEnvelope;
use std::io::BufRead;
use std::io::ErrorKind;
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

pub fn read_envelope_line(
    reader: &mut impl BufRead,
) -> Result<Option<AgentEnvelope>, AgentCodecError> {
    // Bounded line read (vs `read_line`, which grows without limit): accumulate
    // up to MAX_FRAME_BYTES looking for a newline, erroring out rather than
    // letting a hostile guest exhaust host memory by never sending one.
    let mut line: Vec<u8> = Vec::new();
    loop {
        let available = match reader.fill_buf() {
            Ok(buffer) => buffer,
            Err(error) => {
                return Err(AgentCodecError::Io {
                    kind: error.kind(),
                    message: error.to_string(),
                })
            }
        };
        if available.is_empty() {
            // EOF: nothing buffered -> end of stream; a partial line -> let
            // decode_envelope_line report the missing terminator.
            if line.is_empty() {
                return Ok(None);
            }
            break;
        }
        if let Some(newline) = available.iter().position(|&byte| byte == b'\n') {
            if line.len() + newline + 1 > MAX_FRAME_BYTES {
                return Err(AgentCodecError::FrameTooLarge);
            }
            line.extend_from_slice(&available[..=newline]);
            reader.consume(newline + 1);
            break;
        }
        if line.len() + available.len() > MAX_FRAME_BYTES {
            return Err(AgentCodecError::FrameTooLarge);
        }
        let consumed = available.len();
        line.extend_from_slice(available);
        reader.consume(consumed);
    }

    let line = String::from_utf8(line).map_err(|error| AgentCodecError::Io {
        kind: ErrorKind::InvalidData,
        message: error.to_string(),
    })?;
    decode_envelope_line(&line).map(Some)
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
