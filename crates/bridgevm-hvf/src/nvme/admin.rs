//! Admin command dispatch and the admin commands that own no other module (AER, Security Send/Receive).

use super::*;
use crate::fwcfg::GuestMemoryMut;

/// Maximum outstanding Asynchronous Event Request commands retained without
/// completion. The identify controller AERL field advertises this as zero-based.
pub const MAX_ASYNC_EVENT_REQUESTS: u8 = 4;

// ---- Security protocol values (NVMe 1.4 §5.22) -----------------------------
pub(crate) const SECURITY_PROTOCOL_INFORMATION: u8 = 0x00;

pub(crate) const SECURITY_PROTOCOL_DMTF_SPDM: u8 = 0xe8;

pub(crate) const SECURITY_PROTOCOL_INFO_LIST_LEN: usize = 10;

impl NvmeController {
    /// Execute an admin command, returning the NVMe status field to report.
    pub(crate) fn execute_admin(
        &mut self,
        cmd: &SubmissionEntry,
        mem: &mut dyn GuestMemoryMut,
    ) -> CommandResult {
        let status = match cmd.opcode {
            ADMIN_OP_IDENTIFY => self.admin_identify(cmd, mem),
            ADMIN_OP_GET_LOG_PAGE => self.admin_get_log_page(cmd, mem),
            ADMIN_OP_CREATE_IO_CQ => self.admin_create_io_cq(cmd),
            ADMIN_OP_CREATE_IO_SQ => self.admin_create_io_sq(cmd),
            ADMIN_OP_SET_FEATURES => self.admin_set_features(cmd),
            ADMIN_OP_GET_FEATURES => self.admin_get_features(cmd, mem),
            ADMIN_OP_ASYNC_EVENT_REQUEST => return self.admin_async_event_request(),
            ADMIN_OP_SECURITY_SEND => self.admin_security_send(cmd),
            ADMIN_OP_SECURITY_RECV => self.admin_security_receive(cmd, mem),
            ADMIN_OP_DELETE_IO_SQ | ADMIN_OP_DELETE_IO_CQ => SC_SUCCESS,
            _ => SC_INVALID_OPCODE,
        };
        CommandResult::complete(status)
    }

    pub(crate) fn admin_async_event_request(&mut self) -> CommandResult {
        if self.pending_async_event_requests >= MAX_ASYNC_EVENT_REQUESTS {
            return CommandResult::complete(SC_INVALID_FIELD);
        }
        self.pending_async_event_requests += 1;
        CommandResult::pending()
    }

    /// SECURITY SEND. This controller has no writable security-protocol
    /// backend, so every request fails closed as invalid-field with DNR.
    pub(crate) fn admin_security_send(&self, _cmd: &SubmissionEntry) -> u16 {
        SC_INVALID_FIELD_DNR
    }

    /// SECURITY RECEIVE. SECP=0/SPSP=0 returns the supported-protocol list;
    /// SPDM and certificate paths remain unsupported and fail closed.
    pub(crate) fn admin_security_receive(
        &self,
        cmd: &SubmissionEntry,
        mem: &mut dyn GuestMemoryMut,
    ) -> u16 {
        let secp = ((cmd.cdw10 >> 24) & 0xff) as u8;
        let spsp = (cmd.cdw10 >> 8) & 0xffff;
        let alloc_len = cmd.cdw11;
        match (secp, spsp) {
            (SECURITY_PROTOCOL_INFORMATION, 0) => {
                if alloc_len < SECURITY_PROTOCOL_INFO_LIST_LEN as u32 {
                    return SC_INVALID_FIELD_DNR;
                }
                let mut resp = [0u8; SECURITY_PROTOCOL_INFO_LIST_LEN];
                // Two-byte list: the discovery protocol followed by the
                // required zero terminator. No SPDM protocol is advertised.
                resp[7] = 2;
                resp[8] = SECURITY_PROTOCOL_INFORMATION;
                resp[9] = 0;
                if mem.write_bytes(cmd.prp1, &resp) {
                    SC_SUCCESS
                } else {
                    SC_INVALID_FIELD
                }
            }
            (SECURITY_PROTOCOL_DMTF_SPDM, _) => SC_INVALID_FIELD_DNR,
            _ => SC_INVALID_FIELD_DNR,
        }
    }
}
