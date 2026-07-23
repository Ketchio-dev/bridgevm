//! NVMe wire format: geometry limits, opcodes, status codes, and the decoded submission entry / command result.

/// Logical block (LBA) size this model exposes to the guest.
pub const LBA_SIZE: usize = 512;

/// Guest-visible memory page size assumed for PRP transfers (single page).
pub const PAGE_SIZE: usize = 4096;

pub(crate) type NvmePage = [u8; PAGE_SIZE];

pub(crate) const PAGE_SIZE_U64: u64 = PAGE_SIZE as u64;

// ---- Admin opcodes (NVMe 1.4 §5, Figure 139) ------------------------------
pub(crate) const ADMIN_OP_DELETE_IO_SQ: u8 = 0x00;

pub(crate) const ADMIN_OP_CREATE_IO_SQ: u8 = 0x01;

pub(crate) const ADMIN_OP_GET_LOG_PAGE: u8 = 0x02;

pub(crate) const ADMIN_OP_DELETE_IO_CQ: u8 = 0x04;

pub(crate) const ADMIN_OP_CREATE_IO_CQ: u8 = 0x05;

pub(crate) const ADMIN_OP_IDENTIFY: u8 = 0x06;

pub(crate) const ADMIN_OP_SET_FEATURES: u8 = 0x09;

pub(crate) const ADMIN_OP_GET_FEATURES: u8 = 0x0a;

pub(crate) const ADMIN_OP_ASYNC_EVENT_REQUEST: u8 = 0x0c;

pub(crate) const ADMIN_OP_SECURITY_SEND: u8 = 0x81;

pub(crate) const ADMIN_OP_SECURITY_RECV: u8 = 0x82;

// ---- NVM (I/O) opcodes (NVMe NVM Command Set) -----------------------------
pub(crate) const NVM_OP_FLUSH: u8 = 0x00;

pub(crate) const NVM_OP_WRITE: u8 = 0x01;

pub(crate) const NVM_OP_READ: u8 = 0x02;

// ---- Command Set Identifiers (NVMe 1.4 §7.1) ------------------------------
pub(crate) const COMMAND_SET_NVM: u8 = 0x00;

// ---- Completion status codes (NVMe 1.4 §4.6.1, generic command status) ----
/// Successful completion (status code type 0, code 0).
pub(crate) const SC_SUCCESS: u16 = 0x0000;

/// Invalid Field in Command.
pub(crate) const SC_INVALID_FIELD: u16 = 0x0002;

/// Internal Device Error. Used when a valid command reaches the backend but
/// the host cannot complete the requested I/O operation.
pub(crate) const SC_INTERNAL_DEVICE_ERROR: u16 = 0x0006;

/// Do Not Retry bit, carried in the NVMe completion status code field.
pub(crate) const SC_DNR: u16 = 0x4000;

/// QEMU's default for unsupported optional/vendor command surfaces.
pub(crate) const SC_INVALID_FIELD_DNR: u16 = SC_INVALID_FIELD | SC_DNR;

/// Invalid Opcode.
pub(crate) const SC_INVALID_OPCODE: u16 = 0x0001;

/// A decoded 64-byte NVMe submission-queue entry. Only the fields this minimal
/// model consumes are surfaced; everything is read from guest RAM little-endian.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SubmissionEntry {
    /// Command Dword 0: opcode in bits 7:0, command identifier in bits 31:16.
    pub opcode: u8,
    pub command_id: u16,
    /// Namespace Identifier (CDW1).
    pub nsid: u32,
    /// First PRP entry / data pointer (bytes 24..32).
    pub prp1: u64,
    /// Second PRP entry (bytes 32..40) — unused for single-page transfers.
    pub prp2: u64,
    /// Command Dwords 10..16 (command-specific).
    pub cdw10: u32,
    pub cdw11: u32,
    pub cdw12: u32,
    pub cdw13: u32,
    pub cdw14: u32,
    pub cdw15: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CommandResult {
    pub(crate) status: u16,
    pub(crate) complete: bool,
}

impl SubmissionEntry {
    /// Decode a 64-byte submission-queue entry from guest RAM (little-endian).
    pub fn from_bytes(b: &[u8; 64]) -> Self {
        let dw = |i: usize| u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
        let qw = |i: usize| {
            u64::from_le_bytes([
                b[i],
                b[i + 1],
                b[i + 2],
                b[i + 3],
                b[i + 4],
                b[i + 5],
                b[i + 6],
                b[i + 7],
            ])
        };
        let cdw0 = dw(0);
        Self {
            opcode: (cdw0 & 0xff) as u8,
            command_id: (cdw0 >> 16) as u16,
            nsid: dw(4),
            prp1: qw(24),
            prp2: qw(32),
            cdw10: dw(40),
            cdw11: dw(44),
            cdw12: dw(48),
            cdw13: dw(52),
            cdw14: dw(56),
            cdw15: dw(60),
        }
    }
}

impl CommandResult {
    pub(crate) const fn complete(status: u16) -> Self {
        Self {
            status,
            complete: true,
        }
    }

    pub(crate) const fn pending() -> Self {
        Self {
            status: SC_SUCCESS,
            complete: false,
        }
    }
}

pub(crate) const ZERO_APST_FEATURE_DATA: [u8; 256] = [0u8; 256];
// ---- SET FEATURES feature IDs (NVMe 1.4 §5.21.1) --------------------------
pub(crate) const FEATURE_ARBITRATION: u8 = 0x01;
pub(crate) const FEATURE_POWER_MANAGEMENT: u8 = 0x02;
pub(crate) const FEATURE_TEMPERATURE_THRESHOLD: u8 = 0x04;
pub(crate) const FEATURE_ERROR_RECOVERY: u8 = 0x05;
pub(crate) const FEATURE_VOLATILE_WRITE_CACHE: u8 = 0x06;
pub(crate) const FEATURE_NUMBER_OF_QUEUES: u8 = 0x07;
pub(crate) const FEATURE_INTERRUPT_COALESCING: u8 = 0x08;
pub(crate) const FEATURE_INTERRUPT_VECTOR_CONFIGURATION: u8 = 0x09;
pub(crate) const FEATURE_WRITE_ATOMICITY_NORMAL: u8 = 0x0a;
pub(crate) const FEATURE_ASYNC_EVENT_CONFIGURATION: u8 = 0x0b;
pub(crate) const FEATURE_AUTONOMOUS_POWER_STATE_TRANSITION: u8 = 0x0c;
pub(crate) const GET_FEATURE_SELECT_SHIFT: u32 = 8;
pub(crate) const GET_FEATURE_SELECT_DEFAULT: u32 = 0x1;
pub(crate) const GET_FEATURE_SELECT_SAVED: u32 = 0x2;
pub(crate) const GET_FEATURE_SELECT_CAPABILITIES: u32 = 0x3;
pub(crate) const FEATURE_CAP_NAMESPACE_SPECIFIC: u32 = 1 << 1;
pub(crate) const FEATURE_CAP_CHANGEABLE: u32 = 1 << 2;
// ---- GET LOG PAGE log identifiers (NVMe 1.4 §5.14.1) ----------------------
pub(crate) const LOG_PAGE_SMART_HEALTH: u8 = 0x02;
pub(crate) const LOG_PAGE_FIRMWARE_SLOT_INFO: u8 = 0x03;
pub(crate) const LOG_PAGE_COMMAND_EFFECTS: u8 = 0x05;
// ---- Command Effects log bits (NVMe 1.4 §5.14.1.5) ------------------------
pub(crate) const CMD_EFFECT_CSUPP: u32 = 1 << 0;
pub(crate) const CMD_EFFECT_LBCC: u32 = 1 << 1;
