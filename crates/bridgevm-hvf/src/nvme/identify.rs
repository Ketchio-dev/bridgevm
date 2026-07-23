//! IDENTIFY handling and construction of the Identify data structures.

use super::*;
use crate::fwcfg::GuestMemoryMut;

/// Copy `s` into `dst` as ASCII, space-padding the remainder (NVMe string
/// fields are space- not NUL-padded).
pub(crate) fn write_ascii(dst: &mut [u8], s: &str) {
    for b in dst.iter_mut() {
        *b = b' ';
    }
    let bytes = s.as_bytes();
    let n = bytes.len().min(dst.len());
    dst[..n].copy_from_slice(&bytes[..n]);
}

/// Copy `s` into `dst` as a C string, clearing the full destination first.
pub(crate) fn write_cstr(dst: &mut [u8], s: &str) {
    dst.fill(0);
    if dst.is_empty() {
        return;
    }
    let bytes = s.as_bytes();
    let n = bytes.len().min(dst.len() - 1);
    dst[..n].copy_from_slice(&bytes[..n]);
}

pub(crate) fn append_namespace_id_descriptor(dst: &mut [u8], off: &mut usize, nidt: u8, id: &[u8]) {
    let end = *off + 4 + id.len();
    assert!(end <= dst.len(), "namespace ID descriptor list overflow");
    dst[*off] = nidt;
    dst[*off + 1] = id.len() as u8;
    // bytes 2..4 are reserved and remain zero.
    dst[*off + 4..end].copy_from_slice(id);
    *off = end;
}

// ---- IDENTIFY CNS values (NVMe 1.4 §5.15.1) -------------------------------
pub(crate) const IDENTIFY_CNS_NAMESPACE: u32 = 0x00;

pub(crate) const IDENTIFY_CNS_CONTROLLER: u32 = 0x01;

pub(crate) const IDENTIFY_CNS_ACTIVE_NAMESPACE_LIST: u32 = 0x02;

pub(crate) const IDENTIFY_CNS_NAMESPACE_DESCRIPTOR_LIST: u32 = 0x03;

pub(crate) const IDENTIFY_CNS_COMMAND_SET_CONTROLLER: u32 = 0x06;

// ---- Identify Controller feature bits -------------------------------------
pub(crate) const VWC_PRESENT: u8 = 1 << 0;

pub(crate) const VWC_NSID_BROADCAST_SUPPORT: u8 = 3 << 1;

pub(crate) const VWC_QEMU_DEFAULT: u8 = VWC_PRESENT | VWC_NSID_BROADCAST_SUPPORT;

impl NvmeController {
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
}
