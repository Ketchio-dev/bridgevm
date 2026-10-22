//! Raw swtpm socket backend for the TPM TIS frontend.
//!
//! A failed exchange can leave a partial command or unread response bytes in a
//! socket, after which every read returns part of another command's response.
//! The first data- or control-socket failure (an I/O error, including a
//! timeout, a short read, or a response size outside the TIS buffer) therefore
//! latches the backend failed. Neither socket is read or written again; every
//! later command fails, so the frontend answers it with `TPM_RC_FAILURE`, and
//! every reset fails, so the caller does not boot again with this TPM.

use super::{packet_size, validate_packet, Tpm2Backend, TpmBackendError};
use super::{MAX_BUFFER_SIZE, TPM2_HEADER_SIZE};
use std::{
    io::{ErrorKind, Read, Write},
    os::unix::net::UnixStream,
    path::Path,
    time::Duration,
};

/// swtpm control-channel command that performs a `_TPM_Init` power cycle.
///
/// Sent as a 4-byte big-endian command number followed by a 4-byte big-endian
/// `init_flags`; the response is a 4-byte big-endian result code (`0` = success).
/// The connection survives reset while TIS-local state returns to its defaults.
const SWTPM_CMD_INIT: u32 = 0x0000_0002;

/// Raw swtpm command-channel backend.
///
/// Start swtpm with a dedicated `--server type=unixio,path=...` data socket and
/// a `--ctrl type=unixio,path=...` control socket, plus
/// `--flags not-need-init,startup-clear`. The data channel carries complete TPM
/// commands and responses without an extra frame header; the optional control
/// channel is used only to power-cycle the TPM on guest reset.
#[derive(Debug)]
pub struct SwtpmUnixBackend {
    stream: UnixStream,
    control: Option<UnixStream>,
    /// The first channel failure. Once set, neither socket is used again.
    failure: Option<String>,
}

fn connect_swtpm_stream(path: &Path) -> Result<UnixStream, TpmBackendError> {
    let stream = UnixStream::connect(path)?;
    let timeout = Some(Duration::from_secs(30));
    stream.set_read_timeout(timeout)?;
    stream.set_write_timeout(timeout)?;
    Ok(stream)
}

impl SwtpmUnixBackend {
    pub fn connect(path: impl AsRef<Path>) -> Result<Self, TpmBackendError> {
        Self::connect_with_control(path, None::<&Path>)
    }

    /// Connect the data channel and, when `control_path` is supplied, a
    /// persistent control channel used to power-cycle the TPM on guest reset.
    pub fn connect_with_control(
        data_path: impl AsRef<Path>,
        control_path: Option<impl AsRef<Path>>,
    ) -> Result<Self, TpmBackendError> {
        let stream = connect_swtpm_stream(data_path.as_ref())?;
        let control = match control_path {
            Some(path) => Some(connect_swtpm_stream(path.as_ref())?),
            None => None,
        };
        Ok(Self::from_streams(stream, control))
    }

    /// Wrap connected data and optional control streams. The caller owns the
    /// streams' timeouts.
    pub(crate) fn from_streams(stream: UnixStream, control: Option<UnixStream>) -> Self {
        Self {
            stream,
            control,
            failure: None,
        }
    }

    fn refuse_if_failed(&self) -> Result<(), TpmBackendError> {
        match &self.failure {
            Some(failure) => Err(TpmBackendError::Protocol(format!(
                "swtpm backend failed closed earlier ({failure})"
            ))),
            None => Ok(()),
        }
    }

    /// Latch the first failed exchange on `channel` and report it once on stderr.
    fn latch<T>(
        &mut self,
        channel: &str,
        result: Result<T, TpmBackendError>,
    ) -> Result<T, TpmBackendError> {
        if let Err(error) = &result {
            let cause = match error {
                TpmBackendError::Io(io)
                    if matches!(io.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) =>
                {
                    format!("timed out: {io}")
                }
                other => other.to_string(),
            };
            let failure = format!("{channel}: {cause}");
            eprintln!(
                "TPM2 swtpm backend failed closed on the {failure}; later TPM commands get TPM_RC_FAILURE and resets fail"
            );
            self.failure = Some(failure);
        }
        result
    }

    /// Issue swtpm's control-channel `CMD_INIT`, performing a `_TPM_Init` power
    /// cycle. Volatile state (platform authorization, PCRs, transient objects,
    /// sessions) is reset while persisted permanent state is reloaded — exactly
    /// what a hardware TPM does on a system reset. Without this, the firmware's
    /// randomized platform authorization from the prior boot generation
    /// persists and rejects the firmware's empty-auth physical-presence
    /// `TPM2_ClearControl` with `TPM_RC_BAD_AUTH`.
    fn power_cycle(control: &mut UnixStream) -> Result<(), TpmBackendError> {
        let mut request = [0u8; 8];
        request[..4].copy_from_slice(&SWTPM_CMD_INIT.to_be_bytes());
        // init_flags = 0: reset volatile state without resuming a saved-volatile blob.
        control.write_all(&request)?;
        let mut result = [0u8; 4];
        control.read_exact(&mut result)?;
        let code = u32::from_be_bytes(result);
        if code != 0 {
            return Err(TpmBackendError::Protocol(format!(
                "swtpm CMD_INIT returned control result {code:#010x}"
            )));
        }
        Ok(())
    }
}

/// One command/response exchange. Every error leaves the stream unpaired.
fn exchange(stream: &mut UnixStream, command: &[u8]) -> Result<Vec<u8>, TpmBackendError> {
    stream.write_all(command)?;
    let mut header = [0u8; TPM2_HEADER_SIZE];
    stream.read_exact(&mut header)?;
    let response_size = packet_size(&header)?;
    if !(TPM2_HEADER_SIZE..=MAX_BUFFER_SIZE).contains(&response_size) {
        return Err(TpmBackendError::Protocol(format!(
            "swtpm response size {response_size} is outside {TPM2_HEADER_SIZE}..={MAX_BUFFER_SIZE}"
        )));
    }
    let mut response = vec![0u8; response_size];
    response[..TPM2_HEADER_SIZE].copy_from_slice(&header);
    stream.read_exact(&mut response[TPM2_HEADER_SIZE..])?;
    validate_packet("response", &response)?;
    Ok(response)
}

impl Tpm2Backend for SwtpmUnixBackend {
    fn execute(&mut self, locality: u8, command: &[u8]) -> Result<Vec<u8>, TpmBackendError> {
        self.refuse_if_failed()?;
        // These refusals precede any socket I/O, so they leave the stream paired.
        if locality != 0 {
            return Err(TpmBackendError::Protocol(format!(
                "raw swtpm data socket supports locality 0, got {locality}"
            )));
        }
        validate_packet("command", command)?;
        let result = exchange(&mut self.stream, command);
        self.latch("data socket", result)
    }

    fn reset(&mut self) -> Result<(), TpmBackendError> {
        self.refuse_if_failed()?;
        let Some(control) = self.control.as_mut() else {
            return Ok(());
        };
        let result = Self::power_cycle(control);
        self.latch("control socket", result)
    }
}

#[cfg(test)]
#[path = "tpm_swtpm_tests.rs"]
mod tests;
