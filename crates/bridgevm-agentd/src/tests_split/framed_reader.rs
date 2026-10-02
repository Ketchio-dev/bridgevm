//! Fragmented transport, retry, and framing-bound regressions.

use crate::*;
use bridgevm_agent_protocol::AgentEnvelope;
use bridgevm_agent_protocol::AgentMessage;
use std::collections::VecDeque;
use std::io::{self, BufRead, ErrorKind, Read};

struct ScriptedReader {
    steps: VecDeque<Result<Vec<u8>, ErrorKind>>,
    current: Vec<u8>,
    offset: usize,
}

impl ScriptedReader {
    fn new(steps: Vec<Result<Vec<u8>, ErrorKind>>) -> Self {
        Self {
            steps: steps.into(),
            current: Vec::new(),
            offset: 0,
        }
    }
}

impl BufRead for ScriptedReader {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        if self.offset == self.current.len() {
            self.current = self
                .steps
                .pop_front()
                .unwrap_or(Ok(Vec::new()))
                .map_err(io::Error::from)?;
            self.offset = 0;
        }
        Ok(&self.current[self.offset..])
    }

    fn consume(&mut self, amount: usize) {
        self.offset += amount;
    }
}

impl Read for ScriptedReader {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        let available = self.fill_buf()?;
        let amount = output.len().min(available.len());
        output[..amount].copy_from_slice(&available[..amount]);
        self.consume(amount);
        Ok(amount)
    }
}

fn heartbeat_line() -> (AgentEnvelope, Vec<u8>) {
    let envelope = AgentEnvelope::new(AgentMessage::Heartbeat);
    let bytes = encode_envelope_line(&envelope).unwrap().into_bytes();
    (envelope, bytes)
}

#[test]
fn partial_frames_survive_repeated_idle_errors_and_preserve_next_frame() {
    let (envelope, bytes) = heartbeat_line();
    let split = bytes.len() / 3;
    let mut remainder = bytes[split * 2..].to_vec();
    remainder.extend_from_slice(&bytes);
    let mut reader = EnvelopeLineReader::new(ScriptedReader::new(vec![
        Ok(bytes[..split].to_vec()),
        Err(ErrorKind::WouldBlock),
        Ok(bytes[split..split * 2].to_vec()),
        Err(ErrorKind::TimedOut),
        Ok(remainder),
    ]));
    assert!(reader.read_envelope().unwrap_err().is_idle_io());
    assert!(reader.read_envelope().unwrap_err().is_idle_io());
    assert_eq!(reader.read_envelope(), Ok(Some(envelope.clone())));
    assert_eq!(reader.read_envelope(), Ok(Some(envelope)));
    assert_eq!(reader.read_envelope(), Ok(None));
}

#[test]
fn interrupted_reads_retry_before_and_during_frames() {
    let (envelope, bytes) = heartbeat_line();
    let mut reader = EnvelopeLineReader::new(ScriptedReader::new(vec![
        Err(ErrorKind::Interrupted),
        Ok(bytes[..5].to_vec()),
        Err(ErrorKind::Interrupted),
        Ok(bytes[5..].to_vec()),
    ]));
    assert_eq!(reader.read_envelope(), Ok(Some(envelope)));
}

#[test]
fn utf8_codepoint_split_across_idle_accepts_crlf_frame() {
    let envelope = AgentEnvelope::new(AgentMessage::CommandResult {
        request_id: "unicode-result".to_string(),
        ok: true,
        error_code: None,
        message: Some("한글🙂".to_string()),
        result: None,
        metadata: None,
    });
    let mut bytes = encode_envelope_line(&envelope).unwrap().into_bytes();
    bytes.pop();
    bytes.extend_from_slice(b"\r\n");
    let split = bytes.iter().position(|&byte| byte == 0xed).unwrap() + 1;
    let mut reader = EnvelopeLineReader::new(ScriptedReader::new(vec![
        Ok(bytes[..split].to_vec()),
        Err(ErrorKind::WouldBlock),
        Ok(bytes[split..].to_vec()),
    ]));
    assert!(reader.read_envelope().unwrap_err().is_idle_io());
    assert_eq!(reader.read_envelope(), Ok(Some(envelope)));
}

#[test]
fn partial_frame_eof_after_idle_is_terminal() {
    let mut reader = EnvelopeLineReader::new(ScriptedReader::new(vec![
        Ok(b"{}".to_vec()),
        Err(ErrorKind::TimedOut),
        Ok(Vec::new()),
    ]));
    assert!(reader.read_envelope().unwrap_err().is_idle_io());
    let error = reader.read_envelope().unwrap_err();
    assert_eq!(error, AgentCodecError::MissingFrameTerminator);
    assert!(!error.is_idle_io());
}

#[test]
fn fragmented_frame_bound_counts_prefix_and_terminator() {
    for suffix in *b"A\n" {
        let mut reader = EnvelopeLineReader::new(ScriptedReader::new(vec![
            Ok(vec![b' '; MAX_FRAME_BYTES]),
            Err(ErrorKind::WouldBlock),
            Ok(vec![suffix]),
        ]));
        assert!(reader.read_envelope().unwrap_err().is_idle_io());
        assert_eq!(reader.read_envelope(), Err(AgentCodecError::FrameTooLarge));
    }
}

#[test]
fn maximum_sized_fragmented_valid_frame_is_accepted() {
    let (envelope, mut bytes) = heartbeat_line();
    bytes.pop();
    bytes.resize(MAX_FRAME_BYTES - 1, b' ');
    let mut reader = EnvelopeLineReader::new(ScriptedReader::new(vec![
        Ok(bytes),
        Err(ErrorKind::WouldBlock),
        Ok(vec![b'\n']),
    ]));
    assert!(reader.read_envelope().unwrap_err().is_idle_io());
    assert_eq!(reader.read_envelope(), Ok(Some(envelope)));
}

#[test]
fn fragmented_invalid_utf8_and_json_remain_terminal() {
    for bytes in [vec![0xff, b'\n'], b"invalid\n".to_vec()] {
        let mut reader = EnvelopeLineReader::new(ScriptedReader::new(vec![
            Ok(bytes[..1].to_vec()),
            Err(ErrorKind::WouldBlock),
            Ok(bytes[1..].to_vec()),
        ]));
        assert!(reader.read_envelope().unwrap_err().is_idle_io());
        let error = reader.read_envelope().unwrap_err();
        assert!(!error.is_idle_io());
        assert!(matches!(
            error,
            AgentCodecError::Io {
                kind: ErrorKind::InvalidData,
                ..
            } | AgentCodecError::Json(_)
        ));
    }
}
