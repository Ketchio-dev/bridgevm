//! TPM 2.0 FIFO/TIS frontend used by the Windows ARM `virt` platform.
//!
//! The frontend owns the guest-visible locality and FIFO state machine. The
//! command engine is deliberately a trait: production can supervise swtpm (or
//! replace it with an in-process backend) without changing the ACPI/MMIO ABI.

use std::{fmt, io};

#[cfg(unix)]
use std::{
    io::{Read, Write},
    os::unix::net::UnixStream,
    path::Path,
    time::Duration,
};

pub const LOCALITY_COUNT: usize = 5;
pub const LOCALITY_SIZE: u64 = 0x1000;
pub const MMIO_SIZE: u64 = LOCALITY_COUNT as u64 * LOCALITY_SIZE;
pub const MAX_BUFFER_SIZE: usize = 4096;

pub const REG_ACCESS: u64 = 0x000;
pub const REG_INT_ENABLE: u64 = 0x008;
pub const REG_INT_VECTOR: u64 = 0x00c;
pub const REG_INT_STATUS: u64 = 0x010;
pub const REG_INTF_CAPABILITY: u64 = 0x014;
pub const REG_STS: u64 = 0x018;
pub const REG_DATA_FIFO: u64 = 0x024;
pub const REG_INTERFACE_ID: u64 = 0x030;
pub const REG_DID_VID: u64 = 0xf00;
pub const REG_RID: u64 = 0xf04;

pub const ACCESS_VALID: u8 = 0x80;
pub const ACCESS_ACTIVE_LOCALITY: u8 = 0x20;
pub const ACCESS_PENDING_REQUEST: u8 = 0x04;
pub const ACCESS_REQUEST_USE: u8 = 0x02;

pub const STS_VALID: u8 = 0x80;
pub const STS_COMMAND_READY: u8 = 0x40;
pub const STS_TPM_GO: u8 = 0x20;
pub const STS_DATA_AVAILABLE: u8 = 0x10;
pub const STS_EXPECT: u8 = 0x08;
pub const STS_RESPONSE_RETRY: u8 = 0x02;

const STS_TPM_FAMILY_2_0: u32 = 1 << 26;
// TIS 1.3 for TPM 2.0, low-level interrupt capability, dynamic burst count,
// 64-byte transfer support, and the four interrupt classes QEMU advertises.
const INTERFACE_CAPABILITY_TPM2: u32 = 0x3000_0697;
// FIFO interface, five localities, and TIS supported. The selector remains
// FIFO and intentionally does not advertise CRB.
const INTERFACE_ID_TPM2_FIFO: u32 = 0x0000_2100;

const TPM2_HEADER_SIZE: usize = 10;
const TPM2_RC_FAILURE: [u8; TPM2_HEADER_SIZE] =
    [0x80, 0x01, 0x00, 0x00, 0x00, 0x0a, 0x00, 0x00, 0x01, 0x01];
const TPM2_CC_CLEAR: u32 = 0x0000_0126;
const TPM2_CC_CREATE_PRIMARY: u32 = 0x0000_0131;
const TPM2_CC_SELF_TEST: u32 = 0x0000_0143;
const TPM2_CC_STARTUP: u32 = 0x0000_0144;
const TPM2_CC_NV_READ_PUBLIC: u32 = 0x0000_0169;
const TPM2_CC_READ_PUBLIC: u32 = 0x0000_0173;
const TPM2_CC_START_AUTH_SESSION: u32 = 0x0000_0176;
const TPM2_CC_GET_CAPABILITY: u32 = 0x0000_017a;
const TPM2_CC_GET_RANDOM: u32 = 0x0000_017b;
const TPM2_CC_PCR_READ: u32 = 0x0000_017e;
const TPM2_CC_PCR_EXTEND: u32 = 0x0000_0182;

#[derive(Debug)]
pub enum TpmBackendError {
    Io(io::Error),
    Protocol(String),
}

impl fmt::Display for TpmBackendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "{error}"),
            Self::Protocol(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for TpmBackendError {}

impl From<io::Error> for TpmBackendError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

pub trait Tpm2Backend: fmt::Debug + Send {
    fn execute(&mut self, locality: u8, command: &[u8]) -> Result<Vec<u8>, TpmBackendError>;

    fn reset(&mut self) -> Result<(), TpmBackendError> {
        Ok(())
    }
}

/// swtpm control-channel command that performs a `_TPM_Init` power cycle.
///
/// Sent as a 4-byte big-endian command number followed by a 4-byte big-endian
/// `init_flags`; the response is a 4-byte big-endian result code (`0` = success).
/// This mirrors QEMU's `tpm_emulator` machine-reset path.
#[cfg(unix)]
const SWTPM_CMD_INIT: u32 = 0x0000_0002;

/// Raw swtpm command-channel backend.
///
/// Start swtpm with a dedicated `--server type=unixio,path=...` data socket and
/// a `--ctrl type=unixio,path=...` control socket, plus
/// `--flags not-need-init,startup-clear`. The data channel carries complete TPM
/// commands and responses without an extra frame header; the optional control
/// channel is used only to power-cycle the TPM on guest reset.
#[cfg(unix)]
#[derive(Debug)]
pub struct SwtpmUnixBackend {
    stream: UnixStream,
    control: Option<UnixStream>,
}

#[cfg(unix)]
fn connect_swtpm_stream(path: &Path) -> Result<UnixStream, TpmBackendError> {
    let stream = UnixStream::connect(path)?;
    let timeout = Some(Duration::from_secs(30));
    stream.set_read_timeout(timeout)?;
    stream.set_write_timeout(timeout)?;
    Ok(stream)
}

#[cfg(unix)]
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
        Ok(Self { stream, control })
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

#[cfg(unix)]
impl Tpm2Backend for SwtpmUnixBackend {
    fn execute(&mut self, locality: u8, command: &[u8]) -> Result<Vec<u8>, TpmBackendError> {
        if locality != 0 {
            return Err(TpmBackendError::Protocol(format!(
                "raw swtpm data socket supports locality 0, got {locality}"
            )));
        }
        validate_packet("command", command)?;
        self.stream.write_all(command)?;

        let mut header = [0u8; TPM2_HEADER_SIZE];
        self.stream.read_exact(&mut header)?;
        let response_size = packet_size(&header)?;
        if !(TPM2_HEADER_SIZE..=MAX_BUFFER_SIZE).contains(&response_size) {
            return Err(TpmBackendError::Protocol(format!(
                "swtpm response size {response_size} is outside {TPM2_HEADER_SIZE}..={MAX_BUFFER_SIZE}"
            )));
        }
        let mut response = vec![0u8; response_size];
        response[..TPM2_HEADER_SIZE].copy_from_slice(&header);
        self.stream.read_exact(&mut response[TPM2_HEADER_SIZE..])?;
        validate_packet("response", &response)?;
        Ok(response)
    }

    fn reset(&mut self) -> Result<(), TpmBackendError> {
        if let Some(control) = self.control.as_mut() {
            Self::power_cycle(control)?;
        }
        Ok(())
    }
}

fn packet_size(packet: &[u8]) -> Result<usize, TpmBackendError> {
    let bytes: [u8; 4] = packet
        .get(2..6)
        .ok_or_else(|| TpmBackendError::Protocol("TPM packet has no size field".into()))?
        .try_into()
        .expect("four-byte slice");
    Ok(u32::from_be_bytes(bytes) as usize)
}

fn validate_packet(kind: &str, packet: &[u8]) -> Result<(), TpmBackendError> {
    if packet.len() < TPM2_HEADER_SIZE || packet.len() > MAX_BUFFER_SIZE {
        return Err(TpmBackendError::Protocol(format!(
            "TPM {kind} length {} is outside {TPM2_HEADER_SIZE}..={MAX_BUFFER_SIZE}",
            packet.len()
        )));
    }
    let declared = packet_size(packet)?;
    if declared != packet.len() {
        return Err(TpmBackendError::Protocol(format!(
            "TPM {kind} declares {declared} bytes but carries {}",
            packet.len()
        )));
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TisState {
    Idle,
    Ready,
    Reception,
    Completion,
}

#[derive(Debug, Clone, Copy)]
struct Locality {
    access: u8,
    status: u8,
    state: TisState,
    int_enable: u32,
    int_status: u32,
}

impl Default for Locality {
    fn default() -> Self {
        Self {
            access: ACCESS_VALID,
            status: STS_VALID,
            state: TisState::Idle,
            int_enable: 0,
            int_status: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TpmTisStats {
    pub commands: u64,
    pub successful_responses: u64,
    pub error_responses: u64,
    pub backend_failures: u64,
    pub malformed_commands: u64,
    pub malformed_responses: u64,
    pub last_command_code: Option<u32>,
    pub clear_commands: u64,
    pub startup_commands: u64,
    pub self_test_commands: u64,
    pub get_capability_commands: u64,
    pub pcr_read_commands: u64,
    pub pcr_extend_commands: u64,
    pub start_auth_session_commands: u64,
    pub create_primary_commands: u64,
    pub read_public_commands: u64,
    pub nv_read_public_commands: u64,
    pub get_random_commands: u64,
    pub other_commands: u64,
}

impl TpmTisStats {
    fn record_command(&mut self, command: &[u8]) {
        let Some(code) = packet_code(command) else {
            return;
        };
        self.last_command_code = Some(code);
        match code {
            TPM2_CC_CLEAR => self.clear_commands += 1,
            TPM2_CC_STARTUP => self.startup_commands += 1,
            TPM2_CC_SELF_TEST => self.self_test_commands += 1,
            TPM2_CC_GET_CAPABILITY => self.get_capability_commands += 1,
            TPM2_CC_PCR_READ => self.pcr_read_commands += 1,
            TPM2_CC_PCR_EXTEND => self.pcr_extend_commands += 1,
            TPM2_CC_START_AUTH_SESSION => self.start_auth_session_commands += 1,
            TPM2_CC_CREATE_PRIMARY => self.create_primary_commands += 1,
            TPM2_CC_READ_PUBLIC => self.read_public_commands += 1,
            TPM2_CC_NV_READ_PUBLIC => self.nv_read_public_commands += 1,
            TPM2_CC_GET_RANDOM => self.get_random_commands += 1,
            _ => self.other_commands += 1,
        }
    }

    fn record_response(&mut self, response: &[u8]) {
        match packet_code(response) {
            Some(0) => self.successful_responses += 1,
            Some(_) => self.error_responses += 1,
            None => {}
        }
    }
}

#[derive(Debug)]
pub struct TpmTis {
    backend: Box<dyn Tpm2Backend>,
    localities: [Locality; LOCALITY_COUNT],
    active_locality: Option<u8>,
    buffer: Vec<u8>,
    rw_offset: usize,
    stats: TpmTisStats,
}

impl TpmTis {
    pub fn new(backend: Box<dyn Tpm2Backend>) -> Self {
        Self {
            backend,
            localities: [Locality::default(); LOCALITY_COUNT],
            active_locality: None,
            buffer: Vec::with_capacity(MAX_BUFFER_SIZE),
            rw_offset: 0,
            stats: TpmTisStats::default(),
        }
    }

    pub fn reset(&mut self) -> Result<(), TpmBackendError> {
        self.backend.reset()?;
        self.localities = [Locality::default(); LOCALITY_COUNT];
        self.active_locality = None;
        self.buffer.clear();
        self.rw_offset = 0;
        Ok(())
    }

    pub fn stats(&self) -> TpmTisStats {
        self.stats
    }

    pub fn mmio_read(&mut self, offset: u64, size: u8) -> u64 {
        if !matches!(size, 1 | 2 | 4) || offset >= MMIO_SIZE {
            return u64::MAX;
        }
        let locality = (offset / LOCALITY_SIZE) as u8;
        let register_offset = offset % LOCALITY_SIZE;
        if is_fifo(register_offset) {
            return self.read_fifo(locality, size);
        }
        let aligned = register_offset & !3;
        let value = self.register_value(locality, aligned, size);
        let shift = (register_offset & 3) * 8;
        let mask = if size == 4 {
            u32::MAX as u64
        } else {
            (1u64 << (size as u32 * 8)) - 1
        };
        (value >> shift) & mask
    }

    pub fn mmio_write(&mut self, offset: u64, size: u8, value: u64) {
        if !matches!(size, 1 | 2 | 4) || offset >= MMIO_SIZE {
            return;
        }
        let locality = (offset / LOCALITY_SIZE) as u8;
        let register_offset = offset % LOCALITY_SIZE;
        if is_fifo(register_offset) {
            self.write_fifo(locality, register_offset, size, value);
            return;
        }
        let aligned = register_offset & !3;
        let shift = (register_offset & 3) * 8;
        let shifted = (value as u32) << shift;
        match aligned {
            REG_ACCESS => self.write_access(locality, shifted as u8),
            REG_STS => self.write_status(locality, shifted),
            REG_INT_ENABLE => self.localities[locality as usize].int_enable = shifted,
            REG_INT_STATUS => self.localities[locality as usize].int_status &= !shifted,
            _ => {}
        }
    }

    fn register_value(&self, locality: u8, register: u64, size: u8) -> u64 {
        let state = &self.localities[locality as usize];
        match register {
            REG_ACCESS => {
                let mut access = state.access;
                if self.active_locality == Some(locality) {
                    access |= ACCESS_ACTIVE_LOCALITY;
                }
                if self.localities.iter().enumerate().any(|(index, item)| {
                    index != locality as usize && item.access & ACCESS_REQUEST_USE != 0
                }) {
                    access |= ACCESS_PENDING_REQUEST;
                }
                access as u64
            }
            REG_INT_ENABLE => state.int_enable as u64,
            REG_INT_VECTOR => 0,
            REG_INT_STATUS => state.int_status as u64,
            // The driver may poll; IRQ wiring is a separate platform concern.
            REG_INTF_CAPABILITY => INTERFACE_CAPABILITY_TPM2 as u64,
            REG_STS => {
                if self.active_locality != Some(locality) {
                    return 0;
                }
                let available = match state.state {
                    TisState::Completion => self.buffer.len().saturating_sub(self.rw_offset),
                    _ => MAX_BUFFER_SIZE.saturating_sub(self.rw_offset),
                };
                let burst = if size == 1 {
                    available.min(0xff)
                } else {
                    available.min(u16::MAX as usize)
                };
                u64::from(STS_TPM_FAMILY_2_0 | u32::from(state.status)) | ((burst as u64) << 8)
            }
            // TIS 1.3 FIFO interface, TPM 2.0 capable, FIFO selected and
            // interface locked. This is intentionally stable across reset.
            REG_INTERFACE_ID => INTERFACE_ID_TPM2_FIFO as u64,
            REG_DID_VID => 0x0001_1014,
            REG_RID => 1,
            _ => u32::MAX as u64,
        }
    }

    fn write_access(&mut self, locality: u8, value: u8) {
        let index = locality as usize;
        if value & ACCESS_ACTIVE_LOCALITY != 0 && self.active_locality == Some(locality) {
            self.active_locality = None;
            self.localities[index] = Locality::default();
        }
        if value & ACCESS_REQUEST_USE != 0 {
            self.localities[index].access |= ACCESS_REQUEST_USE;
            if self.active_locality.is_none() {
                self.active_locality = Some(locality);
                self.localities[index].access &= !ACCESS_REQUEST_USE;
            }
        }
    }

    fn write_status(&mut self, locality: u8, value: u32) {
        if self.active_locality != Some(locality) {
            return;
        }
        let index = locality as usize;
        if value & STS_COMMAND_READY as u32 != 0 {
            self.buffer.clear();
            self.rw_offset = 0;
            self.localities[index].state = TisState::Ready;
            self.localities[index].status = STS_VALID | STS_COMMAND_READY;
            return;
        }
        if value & STS_RESPONSE_RETRY as u32 != 0
            && self.localities[index].state == TisState::Completion
        {
            self.rw_offset = 0;
            self.localities[index].status = STS_VALID | STS_DATA_AVAILABLE;
            return;
        }
        if value & STS_TPM_GO as u32 != 0
            && self.localities[index].state == TisState::Reception
            && self.localities[index].status & STS_EXPECT == 0
        {
            self.execute_command(locality);
        }
    }

    fn write_fifo(&mut self, locality: u8, register_offset: u64, size: u8, mut value: u64) {
        if self.active_locality != Some(locality) {
            return;
        }
        let index = locality as usize;
        if self.localities[index].state == TisState::Ready {
            self.buffer.clear();
            self.rw_offset = 0;
            self.localities[index].state = TisState::Reception;
            self.localities[index].status = STS_VALID | STS_EXPECT;
        }
        if self.localities[index].state != TisState::Reception {
            return;
        }
        let bytes = usize::from(size).min(4 - (register_offset as usize & 3));
        for _ in 0..bytes {
            if self.buffer.len() >= MAX_BUFFER_SIZE {
                self.localities[index].status = STS_VALID;
                self.stats.malformed_commands += 1;
                break;
            }
            self.buffer.push(value as u8);
            value >>= 8;
        }
        self.rw_offset = self.buffer.len();
        if self.buffer.len() >= 6 {
            match packet_size(&self.buffer) {
                Ok(declared) if declared == self.buffer.len() => {
                    self.localities[index].status = STS_VALID;
                }
                Ok(declared) if declared > self.buffer.len() && declared <= MAX_BUFFER_SIZE => {
                    self.localities[index].status = STS_VALID | STS_EXPECT;
                }
                _ => {
                    self.localities[index].status = STS_VALID;
                    self.stats.malformed_commands += 1;
                }
            }
        }
    }

    fn read_fifo(&mut self, locality: u8, size: u8) -> u64 {
        let index = locality as usize;
        if self.active_locality != Some(locality)
            || self.localities[index].state != TisState::Completion
        {
            return u64::MAX;
        }
        let mut value = 0u64;
        for byte_index in 0..usize::from(size) {
            let byte = self.buffer.get(self.rw_offset).copied().unwrap_or(0xff);
            value |= u64::from(byte) << (byte_index * 8);
            if self.rw_offset < self.buffer.len() {
                self.rw_offset += 1;
            }
        }
        if self.rw_offset >= self.buffer.len() {
            self.localities[index].status = STS_VALID;
        }
        value
    }

    fn execute_command(&mut self, locality: u8) {
        let index = locality as usize;
        self.stats.commands += 1;
        self.stats.record_command(&self.buffer);
        let response = match validate_packet("command", &self.buffer) {
            Ok(()) => match self.backend.execute(locality, &self.buffer) {
                Ok(response) => match validate_packet("response", &response) {
                    Ok(()) => response,
                    Err(_) => {
                        self.stats.malformed_responses += 1;
                        TPM2_RC_FAILURE.to_vec()
                    }
                },
                Err(_) => {
                    self.stats.backend_failures += 1;
                    TPM2_RC_FAILURE.to_vec()
                }
            },
            Err(_) => {
                self.stats.malformed_commands += 1;
                TPM2_RC_FAILURE.to_vec()
            }
        };
        self.stats.record_response(&response);
        self.buffer = response;
        self.rw_offset = 0;
        self.localities[index].state = TisState::Completion;
        self.localities[index].status = STS_VALID | STS_DATA_AVAILABLE;
    }
}

fn is_fifo(register_offset: u64) -> bool {
    register_offset == REG_DATA_FIFO || (0x080..=0x0ff).contains(&register_offset)
}

fn packet_code(packet: &[u8]) -> Option<u32> {
    Some(u32::from_be_bytes(packet.get(6..10)?.try_into().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    type RecordedCommands = Arc<Mutex<Vec<(u8, Vec<u8>)>>>;

    #[derive(Debug)]
    struct RecordingBackend {
        commands: RecordedCommands,
        response: Vec<u8>,
    }

    impl Tpm2Backend for RecordingBackend {
        fn execute(&mut self, locality: u8, command: &[u8]) -> Result<Vec<u8>, TpmBackendError> {
            self.commands
                .lock()
                .unwrap()
                .push((locality, command.to_vec()));
            Ok(self.response.clone())
        }
    }

    fn success_response() -> Vec<u8> {
        vec![0x80, 0x01, 0, 0, 0, 10, 0, 0, 0, 0]
    }

    #[test]
    fn locality_fifo_command_round_trip() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let mut tis = TpmTis::new(Box::new(RecordingBackend {
            commands: calls.clone(),
            response: success_response(),
        }));
        let command: [u8; TPM2_HEADER_SIZE] = [0x80, 0x01, 0, 0, 0, 10, 0, 0, 0x01, 0x44];

        tis.mmio_write(REG_ACCESS, 1, ACCESS_REQUEST_USE as u64);
        assert_eq!(
            tis.mmio_read(REG_ACCESS, 1) as u8 & (ACCESS_VALID | ACCESS_ACTIVE_LOCALITY),
            ACCESS_VALID | ACCESS_ACTIVE_LOCALITY
        );
        tis.mmio_write(REG_STS, 1, STS_COMMAND_READY as u64);
        for byte in command {
            tis.mmio_write(REG_DATA_FIFO, 1, u64::from(byte));
        }
        assert_eq!(tis.mmio_read(REG_STS, 1) as u8 & STS_EXPECT, 0);
        tis.mmio_write(REG_STS, 1, STS_TPM_GO as u64);
        assert_ne!(tis.mmio_read(REG_STS, 1) as u8 & STS_DATA_AVAILABLE, 0);

        let mut response = Vec::new();
        for _ in 0..TPM2_HEADER_SIZE {
            response.push(tis.mmio_read(REG_DATA_FIFO, 1) as u8);
        }
        assert_eq!(response, success_response());
        assert_eq!(&*calls.lock().unwrap(), &[(0, command.to_vec())]);
        assert_eq!(
            tis.stats(),
            TpmTisStats {
                commands: 1,
                successful_responses: 1,
                startup_commands: 1,
                last_command_code: Some(0x144),
                ..TpmTisStats::default()
            }
        );
    }

    #[test]
    fn malformed_backend_response_becomes_tpm2_failure() {
        let mut tis = TpmTis::new(Box::new(RecordingBackend {
            commands: Arc::new(Mutex::new(Vec::new())),
            response: vec![0; 4],
        }));
        let command: [u8; TPM2_HEADER_SIZE] = [0x80, 0x01, 0, 0, 0, 10, 0, 0, 0x01, 0x44];
        tis.mmio_write(REG_ACCESS, 1, ACCESS_REQUEST_USE as u64);
        tis.mmio_write(REG_STS, 1, STS_COMMAND_READY as u64);
        for byte in command {
            tis.mmio_write(REG_DATA_FIFO, 1, u64::from(byte));
        }
        tis.mmio_write(REG_STS, 1, STS_TPM_GO as u64);

        let response = (0..TPM2_HEADER_SIZE)
            .map(|_| tis.mmio_read(REG_DATA_FIFO, 1) as u8)
            .collect::<Vec<_>>();
        assert_eq!(response, TPM2_RC_FAILURE);
        assert_eq!(tis.stats().malformed_responses, 1);
        assert_eq!(tis.stats().error_responses, 1);
        assert_eq!(tis.stats().successful_responses, 0);
    }

    #[test]
    fn command_stats_classify_security_runtime_operations_without_payload_logging() {
        let mut stats = TpmTisStats::default();
        for code in [
            TPM2_CC_CLEAR,
            TPM2_CC_STARTUP,
            TPM2_CC_SELF_TEST,
            TPM2_CC_GET_CAPABILITY,
            TPM2_CC_PCR_READ,
            TPM2_CC_PCR_EXTEND,
            TPM2_CC_START_AUTH_SESSION,
            TPM2_CC_CREATE_PRIMARY,
            TPM2_CC_READ_PUBLIC,
            TPM2_CC_NV_READ_PUBLIC,
            TPM2_CC_GET_RANDOM,
            0x153,
        ] {
            let mut command = [0x80, 0x01, 0, 0, 0, 10, 0, 0, 0, 0];
            command[6..10].copy_from_slice(&code.to_be_bytes());
            stats.record_command(&command);
        }

        assert_eq!(stats.clear_commands, 1);
        assert_eq!(stats.startup_commands, 1);
        assert_eq!(stats.self_test_commands, 1);
        assert_eq!(stats.get_capability_commands, 1);
        assert_eq!(stats.pcr_read_commands, 1);
        assert_eq!(stats.pcr_extend_commands, 1);
        assert_eq!(stats.start_auth_session_commands, 1);
        assert_eq!(stats.create_primary_commands, 1);
        assert_eq!(stats.read_public_commands, 1);
        assert_eq!(stats.nv_read_public_commands, 1);
        assert_eq!(stats.get_random_commands, 1);
        assert_eq!(stats.other_commands, 1);
        assert_eq!(stats.last_command_code, Some(0x153));
    }

    #[test]
    fn inactive_locality_cannot_use_fifo() {
        let mut tis = TpmTis::new(Box::new(RecordingBackend {
            commands: Arc::new(Mutex::new(Vec::new())),
            response: success_response(),
        }));
        tis.mmio_write(LOCALITY_SIZE + REG_DATA_FIFO, 1, 0xaa);
        assert_eq!(tis.mmio_read(LOCALITY_SIZE + REG_DATA_FIFO, 1), u64::MAX);
        assert_eq!(tis.stats().commands, 0);
    }

    #[test]
    fn identity_registers_advertise_tpm2_fifo_and_five_localities() {
        let mut tis = TpmTis::new(Box::new(RecordingBackend {
            commands: Arc::new(Mutex::new(Vec::new())),
            response: success_response(),
        }));
        assert_eq!(
            tis.mmio_read(REG_INTF_CAPABILITY, 4),
            u64::from(INTERFACE_CAPABILITY_TPM2)
        );
        assert_eq!(
            tis.mmio_read(REG_INTERFACE_ID, 4),
            u64::from(INTERFACE_ID_TPM2_FIFO)
        );
        tis.mmio_write(REG_ACCESS, 1, ACCESS_REQUEST_USE as u64);
        assert_ne!(tis.mmio_read(REG_STS, 4) as u32 & STS_TPM_FAMILY_2_0, 0);
    }

    #[cfg(unix)]
    #[test]
    fn swtpm_backend_reset_power_cycles_via_control_socket_cmd_init() {
        use std::io::{Read, Write};
        use std::os::unix::net::UnixListener;
        use std::thread;

        let dir = std::env::temp_dir().join(format!(
            "bridgevm-swtpm-ctrl-{}-{}",
            std::process::id(),
            std::time::Instant::now().elapsed().as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let data_path = dir.join("data.sock");
        let control_path = dir.join("control.sock");
        let data_listener = UnixListener::bind(&data_path).unwrap();
        let control_listener = UnixListener::bind(&control_path).unwrap();

        // The data server just accepts and holds the connection; the backend
        // does not send data-channel traffic during reset.
        let data_thread = thread::spawn(move || {
            let (_stream, _addr) = data_listener.accept().unwrap();
            // Keep the connection open until the test drops the backend.
            thread::sleep(Duration::from_millis(200));
        });

        // The control server captures the CMD_INIT request and replies success.
        let control_thread = thread::spawn(move || {
            let (mut stream, _addr) = control_listener.accept().unwrap();
            let mut request = [0u8; 8];
            stream.read_exact(&mut request).unwrap();
            stream.write_all(&0u32.to_be_bytes()).unwrap();
            request
        });

        let mut backend =
            SwtpmUnixBackend::connect_with_control(&data_path, Some(&control_path)).unwrap();
        backend.reset().expect("power cycle should succeed");

        let request = control_thread.join().unwrap();
        data_thread.join().unwrap();

        // CMD_INIT (0x00000002) big-endian, then init_flags = 0.
        assert_eq!(request, [0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00]);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn swtpm_backend_reset_fails_closed_when_control_reports_error() {
        use std::io::{Read, Write};
        use std::os::unix::net::UnixListener;
        use std::thread;

        let dir = std::env::temp_dir().join(format!(
            "bridgevm-swtpm-ctrl-err-{}-{}",
            std::process::id(),
            std::time::Instant::now().elapsed().as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let data_path = dir.join("data.sock");
        let control_path = dir.join("control.sock");
        let data_listener = UnixListener::bind(&data_path).unwrap();
        let control_listener = UnixListener::bind(&control_path).unwrap();

        let data_thread = thread::spawn(move || {
            let (_stream, _addr) = data_listener.accept().unwrap();
            thread::sleep(Duration::from_millis(200));
        });
        let control_thread = thread::spawn(move || {
            let (mut stream, _addr) = control_listener.accept().unwrap();
            let mut request = [0u8; 8];
            stream.read_exact(&mut request).unwrap();
            // Non-zero control result → swtpm rejected the power cycle.
            stream.write_all(&1u32.to_be_bytes()).unwrap();
        });

        let mut backend =
            SwtpmUnixBackend::connect_with_control(&data_path, Some(&control_path)).unwrap();
        let err = backend
            .reset()
            .expect_err("non-zero control result must fail closed");
        assert!(matches!(err, TpmBackendError::Protocol(_)));

        control_thread.join().unwrap();
        data_thread.join().unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }
}
