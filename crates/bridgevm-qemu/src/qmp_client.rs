//! QmpClient: connect, negotiate, execute, read and drain envelopes.

use crate::*;
use serde_json::Value;
use std::io::BufRead;
use std::io::BufReader;
use std::io::ErrorKind;
use std::io::Read;
use std::io::Write;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;

pub struct QmpClient {
    pub(crate) reader: BufReader<UnixStream>,
    pub(crate) writer: UnixStream,
}

impl QmpClient {
    pub fn connect(socket_path: &Path) -> Result<Self, QemuError> {
        Self::connect_with_timeout(socket_path, Duration::from_secs(1))
    }

    pub fn connect_with_timeout(socket_path: &Path, timeout: Duration) -> Result<Self, QemuError> {
        let stream = UnixStream::connect(socket_path)?;
        stream.set_read_timeout(Some(timeout))?;
        stream.set_write_timeout(Some(timeout))?;
        let writer = stream.try_clone()?;
        Ok(Self {
            reader: BufReader::new(stream),
            writer,
        })
    }

    pub fn negotiate(&mut self) -> Result<(), QemuError> {
        let _ = self.read_envelope()?;
        let _ = self.execute(QmpCommand::capabilities())?;
        Ok(())
    }

    pub fn execute(&mut self, command: QmpCommand) -> Result<Value, QemuError> {
        serde_json::to_writer(&mut self.writer, &command)?;
        self.writer.write_all(b"\n")?;
        for _ in 0..MAX_QMP_SKIPPED_ENVELOPES {
            let envelope = self.read_envelope()?;
            if envelope.event.is_some() {
                continue;
            }
            if let Some(error) = envelope.error {
                return Err(QemuError::QmpProtocol(error.to_string()));
            }
            return envelope
                .result
                .ok_or_else(|| QemuError::QmpProtocol("missing return".to_string()));
        }
        Err(QemuError::QmpProtocol(format!(
            "QMP command skipped more than {MAX_QMP_SKIPPED_ENVELOPES} event envelopes"
        )))
    }

    pub fn read_event(&mut self) -> Result<QmpEvent, QemuError> {
        for _ in 0..MAX_QMP_SKIPPED_ENVELOPES {
            let envelope = self.read_envelope()?;
            if let Some(event) = envelope.event {
                return Ok(event);
            }
        }
        Err(QemuError::QmpProtocol(format!(
            "QMP event wait skipped more than {MAX_QMP_SKIPPED_ENVELOPES} non-event envelopes"
        )))
    }

    pub fn drain_events(&mut self, max_envelopes: usize) -> Result<QmpEventDrain, QemuError> {
        let mut drain = QmpEventDrain::empty();

        for _ in 0..max_envelopes {
            match self.read_envelope() {
                Ok(envelope) => {
                    drain.envelopes_read += 1;
                    if let Some(event) = envelope.event {
                        if event.is_terminal() {
                            drain.terminal_event = Some(event.clone());
                        }
                        drain.events.push(event);

                        if drain.terminal_event.is_some() {
                            return Ok(drain);
                        }
                    }
                }
                Err(error) if error.is_qmp_idle() => return Ok(drain),
                Err(error) => return Err(error),
            }
        }

        drain.limit_reached = max_envelopes > 0;
        Ok(drain)
    }

    pub fn read_envelope(&mut self) -> Result<QmpEnvelope, QemuError> {
        let mut frame = Vec::new();
        if (&mut self.reader)
            .take(MAX_QMP_ENVELOPE_BYTES + 1)
            .read_until(b'\n', &mut frame)?
            == 0
        {
            return Err(QemuError::QmpIo(std::io::Error::new(
                ErrorKind::UnexpectedEof,
                "QMP stream closed",
            )));
        }
        if frame.len() as u64 > MAX_QMP_ENVELOPE_BYTES {
            return Err(QemuError::QmpProtocol(format!(
                "QMP envelope exceeded {MAX_QMP_ENVELOPE_BYTES} bytes"
            )));
        }
        if frame.last() != Some(&b'\n') {
            return Err(QemuError::QmpProtocol(
                "QMP stream returned an incomplete envelope".to_string(),
            ));
        }
        let value = serde_json::from_slice::<Value>(&frame)?;
        Ok(QmpEnvelope {
            greeting: value.get("QMP").cloned(),
            event: value
                .get("event")
                .and_then(Value::as_str)
                .map(|name| QmpEvent {
                    name: name.to_string(),
                    data: value.get("data").cloned(),
                }),
            result: value.get("return").cloned(),
            error: value.get("error").cloned(),
        })
    }
}
