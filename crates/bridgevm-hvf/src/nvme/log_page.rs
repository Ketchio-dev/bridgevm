//! GET LOG PAGE handling and the SMART / firmware-slot / command-effects payloads.

use super::*;
use crate::fwcfg::GuestMemoryMut;

impl NvmeController {
    /// GET LOG PAGE. Linux reads SMART / health information during probe, and
    /// Windows asks for the command-effects log while sizing the controller.
    pub(crate) fn admin_get_log_page(
        &self,
        cmd: &SubmissionEntry,
        mem: &mut dyn GuestMemoryMut,
    ) -> u16 {
        let lid = (cmd.cdw10 & 0xff) as u8;
        let numdl = (cmd.cdw10 >> 16) & 0xffff;
        let numdu = cmd.cdw11 & 0xffff;
        let offset = ((u64::from(cmd.cdw13)) << 32) | u64::from(cmd.cdw12);
        if offset & 0x3 != 0 || offset >= PAGE_SIZE_U64 {
            return SC_INVALID_FIELD_DNR;
        }
        let dword_count = ((numdu << 16) | numdl).saturating_add(1);
        let max_len = PAGE_SIZE - offset as usize;
        let byte_count = (dword_count as usize).saturating_mul(4).min(max_len);

        let log = match lid {
            LOG_PAGE_SMART_HEALTH => self.smart_health_log(),
            LOG_PAGE_FIRMWARE_SLOT_INFO => self.firmware_slot_info_log(),
            LOG_PAGE_COMMAND_EFFECTS => self.command_effects_log(),
            _ => return SC_INVALID_FIELD_DNR,
        };
        let start = offset as usize;
        let data = &log[start..start + byte_count];
        if mem.write_bytes(cmd.prp1, data) {
            SC_SUCCESS
        } else {
            SC_INVALID_FIELD
        }
    }

    pub(crate) fn smart_health_log(&self) -> NvmePage {
        let mut d = [0u8; PAGE_SIZE];
        // Composite temperature in Kelvin, little-endian at bytes 1..3.
        // 300K is boring and healthy.
        d[1..3].copy_from_slice(&300u16.to_le_bytes());
        d[3] = 100; // available spare (%)
        d[4] = 10; // available spare threshold (%)
        d
    }

    pub(crate) fn firmware_slot_info_log(&self) -> NvmePage {
        let mut d = [0u8; PAGE_SIZE];
        // Active Firmware Info: active slot 1, no pending activation slot.
        d[0] = 1;
        write_ascii(&mut d[8..72], "BridgeVM NVMe firmware slot 1");
        d
    }

    pub(crate) fn command_effects_log(&self) -> NvmePage {
        let mut d = [0u8; PAGE_SIZE];
        let mut set_admin = |opcode: u8, effects: u32| {
            let off = usize::from(opcode) * 4;
            d[off..off + 4].copy_from_slice(&effects.to_le_bytes());
        };
        set_admin(ADMIN_OP_DELETE_IO_SQ, CMD_EFFECT_CSUPP);
        set_admin(ADMIN_OP_CREATE_IO_SQ, CMD_EFFECT_CSUPP);
        set_admin(ADMIN_OP_GET_LOG_PAGE, CMD_EFFECT_CSUPP);
        set_admin(ADMIN_OP_DELETE_IO_CQ, CMD_EFFECT_CSUPP);
        set_admin(ADMIN_OP_CREATE_IO_CQ, CMD_EFFECT_CSUPP);
        set_admin(ADMIN_OP_IDENTIFY, CMD_EFFECT_CSUPP);
        set_admin(ADMIN_OP_SET_FEATURES, CMD_EFFECT_CSUPP);
        set_admin(ADMIN_OP_GET_FEATURES, CMD_EFFECT_CSUPP);
        set_admin(ADMIN_OP_ASYNC_EVENT_REQUEST, CMD_EFFECT_CSUPP);
        set_admin(ADMIN_OP_SECURITY_SEND, CMD_EFFECT_CSUPP);
        set_admin(ADMIN_OP_SECURITY_RECV, CMD_EFFECT_CSUPP);

        let mut set_io = |opcode: u8, effects: u32| {
            let off = 1024 + usize::from(opcode) * 4;
            d[off..off + 4].copy_from_slice(&effects.to_le_bytes());
        };
        set_io(NVM_OP_FLUSH, CMD_EFFECT_CSUPP | CMD_EFFECT_LBCC);
        set_io(NVM_OP_WRITE, CMD_EFFECT_CSUPP | CMD_EFFECT_LBCC);
        set_io(NVM_OP_READ, CMD_EFFECT_CSUPP);
        d
    }
}
