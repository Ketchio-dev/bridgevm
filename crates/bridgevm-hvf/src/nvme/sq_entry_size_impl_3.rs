//! Continuation of the `sq_entry_size` impl block, split for the 1000-line rule.

use super::*;

use crate::fwcfg::GuestMemoryMut;
use crate::pcie::NVME_MSIX_VECTOR_COUNT;

impl NvmeController {
    pub(crate) fn record_command_trace(&mut self, trace: NvmeCommandTrace) {
        if self.command_trace.len() == COMMAND_TRACE_CAPACITY {
            self.command_trace.pop_front();
        }
        self.command_trace.push_back(trace);
    }

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

    /// IDENTIFY (CNS in CDW10 bits 7:0). Writes a 4 KiB structure to PRP1.
    pub(crate) fn admin_identify(
        &self,
        cmd: &SubmissionEntry,
        mem: &mut dyn GuestMemoryMut,
    ) -> u16 {
        let cns = cmd.cdw10 & 0xff;
        let data = match cns {
            IDENTIFY_CNS_CONTROLLER => self.identify_controller(),
            IDENTIFY_CNS_COMMAND_SET_CONTROLLER => {
                let csi = ((cmd.cdw11 >> 24) & 0xff) as u8;
                if csi != COMMAND_SET_NVM {
                    return SC_INVALID_FIELD;
                }
                self.identify_command_set_controller()
            }
            IDENTIFY_CNS_ACTIVE_NAMESPACE_LIST => self.identify_active_namespace_list(cmd.nsid),
            IDENTIFY_CNS_NAMESPACE_DESCRIPTOR_LIST => {
                if self.backend_for_nsid(cmd.nsid).is_some() {
                    self.identify_namespace_descriptor_list(cmd.nsid)
                } else {
                    return SC_INVALID_FIELD;
                }
            }
            IDENTIFY_CNS_NAMESPACE => {
                if self.backend_for_nsid(cmd.nsid).is_some() {
                    self.identify_namespace(cmd.nsid)
                } else {
                    // Unallocated namespace ⇒ a zeroed structure (NVMe 1.4).
                    [0u8; PAGE_SIZE]
                }
            }
            _ => return SC_INVALID_FIELD,
        };
        if nvme_trace_enabled() {
            let label = identify_cns_name(cns);
            let preview_len = data.len().min(32);
            println!(
                "NVME identify {label} cns={cns:#x} nsid={} len={} first={} block_count={}",
                cmd.nsid,
                data.len(),
                hex_preview(&data[..preview_len]),
                self.block_count()
            );
        }
        if mem.write_bytes(cmd.prp1, &data) {
            SC_SUCCESS
        } else {
            SC_INVALID_FIELD
        }
    }

    /// Build a 4 KiB Identify Controller structure (NVMe 1.4 §5.15.2.2).
    pub(crate) fn identify_controller(&self) -> NvmePage {
        let mut d = [0u8; PAGE_SIZE];
        // VID (0..2) / SSVID (2..4): a recognisable but inert vendor id.
        d[0..2].copy_from_slice(&0x1b36u16.to_le_bytes()); // Red Hat / QEMU
        d[2..4].copy_from_slice(&0x1b36u16.to_le_bytes());
        // SN (4..24), MN (24..64), FR (64..72): ASCII, space-padded.
        write_ascii(&mut d[4..24], "BRIDGEVM0000000001");
        write_ascii(&mut d[24..64], "BridgeVM NVMe");
        write_ascii(&mut d[64..72], "1.0");
        // RAB (72) recommended arbitration burst.
        d[72] = 0;
        // VER (80..84): identify data agrees with VS.
        d[80..84].copy_from_slice(&NVME_VERSION_1_4_0.to_le_bytes());
        // AERL (259): maximum concurrently outstanding async event requests,
        // zero-based. Windows submits AERs during setup; they should remain
        // pending rather than completing as invalid opcodes.
        d[259] = MAX_ASYNC_EVENT_REQUESTS - 1;
        // OACS (256..258): advertise Security Send/Receive now that the minimal
        // QEMU-compatible security protocol information query is implemented.
        d[256..258].copy_from_slice(&1u16.to_le_bytes());
        // SQES (512): min/max submission-queue entry size = 2^6 = 64 bytes.
        d[512] = 0x66;
        // CQES (513): min/max completion-queue entry size = 2^4 = 16 bytes.
        d[513] = 0x44;
        // NN (516..520): maximum/number of namespaces (1 or 2).
        d[516..520].copy_from_slice(&self.namespace_count().to_le_bytes());
        // VWC (525): QEMU advertises a present volatile write cache and support
        // for broadcast-NSID flushes.
        d[525] = VWC_QEMU_DEFAULT;
        // SUBNQN (768..1024): NUL-terminated subsystem NQN. Linux warns for
        // NVMe >= 1.2.1 if this field is empty or consumes the whole NQN field.
        write_cstr(
            &mut d[768..1024],
            "nqn.2026-06.dev.bridgevm:bridgevm-hvf:nvme0",
        );
        d
    }

    /// Build a 4 KiB command-set-specific Identify Controller structure for the
    /// NVM command set (CNS=0x06, CSI=0). QEMU answers this Windows probe with
    /// an otherwise boring `NvmeIdCtrlNvm`; keep the BridgeVM page conservative
    /// rather than advertising optional NVM commands that are not implemented.
    pub(crate) fn identify_command_set_controller(&self) -> NvmePage {
        [0u8; PAGE_SIZE]
    }

    /// Build a 4 KiB Identify Namespace structure (NVMe 1.4 §5.15.2.1).
    pub(crate) fn identify_namespace(&self, nsid: u32) -> NvmePage {
        let mut d = [0u8; PAGE_SIZE];
        let nsze = self.block_count_for(nsid);
        // NSZE (0..8), NCAP (8..16), NUSE (16..24): all in logical blocks.
        d[0..8].copy_from_slice(&nsze.to_le_bytes());
        d[8..16].copy_from_slice(&nsze.to_le_bytes());
        d[16..24].copy_from_slice(&nsze.to_le_bytes());
        // NLBAF (25): number of LBA formats minus one ⇒ 0 ⇒ one format.
        d[25] = 0;
        // FLBAS (26): formatted LBA size ⇒ format index 0.
        d[26] = 0;
        // NGUID (104..120) / EUI64 (120..128): stable non-zero namespace IDs.
        let (nguid, eui64, _uuid) = namespace_identifiers(nsid);
        d[104..120].copy_from_slice(&nguid);
        d[120..128].copy_from_slice(&eui64);
        // LBAF0 (128..132): MS=0, LBADS = log2(512) = 9 (bits 23:16), RP=0.
        let lbads: u32 = 9 << 16;
        d[128..132].copy_from_slice(&lbads.to_le_bytes());
        d
    }

    /// Build a 4 KiB Identify Active Namespace ID List (CNS=0x02). The list
    /// contains active namespace IDs greater than the command NSID, in ascending
    /// order, terminated by zero.
    pub(crate) fn identify_active_namespace_list(&self, after_nsid: u32) -> NvmePage {
        let mut d = [0u8; PAGE_SIZE];
        let mut off = 0usize;
        for nsid in [NSID, NSID2] {
            if nsid > after_nsid && self.backend_for_nsid(nsid).is_some() {
                d[off..off + 4].copy_from_slice(&nsid.to_le_bytes());
                off += 4;
            }
        }
        d
    }

    /// Build a 4 KiB Identify Namespace Identification Descriptor List
    /// (CNS=0x03). UUID, NGUID and EUI64 descriptors mirror the stable namespace
    /// identifiers in Identify Namespace, followed by a zero descriptor header to
    /// terminate the list.
    pub(crate) fn identify_namespace_descriptor_list(&self, nsid: u32) -> NvmePage {
        let mut d = [0u8; PAGE_SIZE];
        let mut off = 0usize;
        let (nguid, eui64, uuid) = namespace_identifiers(nsid);
        append_namespace_id_descriptor(&mut d, &mut off, 0x03, &uuid);
        append_namespace_id_descriptor(&mut d, &mut off, 0x02, &nguid);
        append_namespace_id_descriptor(&mut d, &mut off, 0x01, &eui64);
        d
    }

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

    /// SECURITY SEND. QEMU advertises the opcode, but without an SPDM socket it
    /// rejects every protocol as invalid-field. Keep that shape while the
    /// controller only supports the discovery receive path below.
    pub(crate) fn admin_security_send(&self, _cmd: &SubmissionEntry) -> u16 {
        SC_INVALID_FIELD_DNR
    }

    /// SECURITY RECEIVE. Match QEMU's default no-SPDM behavior: the only
    /// successful request is SECP=0/SPSP=0, which returns the supported security
    /// protocol list. SPDM and certificate paths remain invalid-field.
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
                // QEMU reports a two-byte supported-protocol list containing
                // Security Protocol Information and a second zero entry when no
                // SPDM socket is configured.
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

    /// CREATE I/O COMPLETION QUEUE (NVMe 1.4 §5.3). CDW10: QID bits 15:0,
    /// QSIZE bits 31:16 (0-based). CDW11: PC bit 0, IEN bit 1, interrupt
    /// vector bits 31:16. PRP1 is the queue base.
    pub(crate) fn admin_create_io_cq(&mut self, cmd: &SubmissionEntry) -> u16 {
        let qid = (cmd.cdw10 & 0xffff) as usize;
        let qsize_zero_based = ((cmd.cdw10 >> 16) & 0xffff) as u16;
        let interrupt_vector = ((cmd.cdw11 >> CREATE_IO_CQ_IV_SHIFT) & 0xffff) as u16;
        let interrupts_enabled = cmd.cdw11 & CREATE_IO_CQ_IEN_BIT != 0;
        if qid == 0 || qid > usize::from(self.max_io_queues) {
            return SC_INVALID_FIELD; // QID 0 is admin; higher QIDs lack doorbells.
        }
        if qsize_zero_based >= MAX_QUEUE_ENTRIES {
            return SC_INVALID_FIELD;
        }
        let qsize = qsize_zero_based + 1;
        if cmd.cdw11 & CREATE_IO_CQ_PC_BIT == 0 {
            return SC_INVALID_FIELD; // CAP.CQR requires physically contiguous queues.
        }
        if interrupts_enabled && interrupt_vector >= NVME_MSIX_VECTOR_COUNT {
            return SC_INVALID_FIELD;
        }
        ensure_slot(&mut self.cqs, qid);
        self.cqs[qid] = Some(CompletionQueue {
            base: cmd.prp1,
            size: qsize,
            tail: 0,
            phase: true,
            head: 0,
            interrupt_vector,
            interrupts_enabled,
        });
        SC_SUCCESS
    }

    /// CREATE I/O SUBMISSION QUEUE (NVMe 1.4 §5.4). CDW10: QID / QSIZE as for
    /// the CQ; CDW11 bits 31:16 carry the associated CQID. PRP1 is the base.
    pub(crate) fn admin_create_io_sq(&mut self, cmd: &SubmissionEntry) -> u16 {
        let qid = (cmd.cdw10 & 0xffff) as usize;
        let qsize_zero_based = ((cmd.cdw10 >> 16) & 0xffff) as u16;
        let cqid = ((cmd.cdw11 >> 16) & 0xffff) as u16;
        if qid == 0 || qid > usize::from(self.max_io_queues) {
            return SC_INVALID_FIELD;
        }
        if qsize_zero_based >= MAX_QUEUE_ENTRIES {
            return SC_INVALID_FIELD;
        }
        let qsize = qsize_zero_based + 1;
        // The completion queue this SQ targets must already exist.
        if self.cqs.get(cqid as usize).map(Option::is_some) != Some(true) {
            return SC_INVALID_FIELD;
        }
        ensure_slot(&mut self.sqs, qid);
        self.sqs[qid] = Some(SubmissionQueue {
            base: cmd.prp1,
            size: qsize,
            head: 0,
            tail_doorbell: 0,
            cqid,
        });
        self.clear_sq_pending(qid);
        SC_SUCCESS
    }

    /// SET FEATURES (NVMe 1.4 §5.21). Keep the small set Windows probes aligned
    /// with QEMU defaults; unsupported features remain harmless no-ops here.
    pub(crate) fn admin_set_features(&mut self, cmd: &SubmissionEntry) -> u16 {
        let fid = (cmd.cdw10 & 0xff) as u8;
        match fid {
            FEATURE_NUMBER_OF_QUEUES => {
                // CDW11: NSQR bits 15:0, NCQR bits 31:16 (both 0-based requests).
                let nsqr = (cmd.cdw11 & 0xffff) as u16;
                let ncqr = ((cmd.cdw11 >> 16) & 0xffff) as u16;
                // Grant the smaller of each request and our capacity (all 0-based).
                let capacity = self.max_io_queues.saturating_sub(1);
                let sq_granted = nsqr.min(capacity);
                let cq_granted = ncqr.min(capacity);
                // The completion DW0 carries the allocated counts (0-based: NSQA in
                // bits 15:0, NCQA in bits 31:16); the generic completion path emits
                // it via `last_feature_result`.
                self.last_feature_result = (u32::from(cq_granted) << 16) | u32::from(sq_granted);
            }
            FEATURE_VOLATILE_WRITE_CACHE => {
                self.volatile_write_cache_enabled = (cmd.cdw11 & 1) != 0;
                if !self.volatile_write_cache_enabled {
                    let _ = self.disk.flush();
                }
            }
            _ => {}
        }
        SC_SUCCESS
    }

    /// GET FEATURES (NVMe 1.4 §5.14). Windows probes several optional features
    /// during setup. Return boring, disabled defaults for the generic features
    /// this tiny controller can safely expose, and report invalid-field (not
    /// invalid-opcode) for reserved/vendor-specific feature IDs.
    pub(crate) fn admin_get_features(
        &mut self,
        cmd: &SubmissionEntry,
        mem: &mut dyn GuestMemoryMut,
    ) -> u16 {
        let fid = (cmd.cdw10 & 0xff) as u8;
        let select = (cmd.cdw10 >> GET_FEATURE_SELECT_SHIFT) & 0x7;
        if select == GET_FEATURE_SELECT_CAPABILITIES {
            let Some(capabilities) = feature_capabilities(fid) else {
                return SC_INVALID_FIELD_DNR;
            };
            self.last_feature_result = capabilities;
            return SC_SUCCESS;
        }
        let wants_default = matches!(
            select,
            GET_FEATURE_SELECT_DEFAULT | GET_FEATURE_SELECT_SAVED
        );
        let value = match fid {
            FEATURE_ARBITRATION => 0,
            FEATURE_POWER_MANAGEMENT => 0,
            FEATURE_TEMPERATURE_THRESHOLD => 0,
            FEATURE_ERROR_RECOVERY => 0,
            FEATURE_VOLATILE_WRITE_CACHE => {
                if wants_default {
                    0
                } else {
                    u32::from(self.volatile_write_cache_enabled)
                }
            }
            FEATURE_NUMBER_OF_QUEUES => {
                let granted = u32::from(self.max_io_queues.saturating_sub(1));
                (granted << 16) | granted
            }
            FEATURE_INTERRUPT_COALESCING => 0,
            FEATURE_INTERRUPT_VECTOR_CONFIGURATION => cmd.cdw11 & 0xffff,
            FEATURE_WRITE_ATOMICITY_NORMAL => 0,
            FEATURE_ASYNC_EVENT_CONFIGURATION => 0,
            FEATURE_AUTONOMOUS_POWER_STATE_TRANSITION => {
                if cmd.prp1 != 0 && !mem.write_bytes(cmd.prp1, &ZERO_APST_FEATURE_DATA) {
                    return SC_INVALID_FIELD;
                }
                0
            }
            _ => return SC_INVALID_FIELD_DNR,
        };
        self.last_feature_result = value;
        SC_SUCCESS
    }
}
