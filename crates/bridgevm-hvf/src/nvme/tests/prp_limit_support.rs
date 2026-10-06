//! Public queue fixtures for the advertised maximum I/O transfer.

use super::super::*;
use super::helpers::*;
use crate::fwcfg::GuestMemoryMut;

const LIST_BASE: u64 = MEM_BASE + 0x8000;
const DATA_START: u64 = MEM_BASE + 0x20000;
const GUARD: u8 = 0xd3;

pub(super) const MAX_TRANSFER: usize = 65_536 * LBA_SIZE;
pub(super) const SIXTEEN_LIST_PAGES: usize = (1 + 15 * 511 + 512) * PAGE_SIZE;

pub(super) struct Transfer {
    pub(super) bytes: usize,
    pub(super) first_offset: usize,
    pub(super) list_offset: usize,
}

pub(super) struct Fixture {
    pub(super) ctrl: NvmeController,
    pub(super) mem: FakeMem,
    pub(super) command: [u8; 64],
    pub(super) lists: Vec<u64>,
    expected_ram: Vec<u8>,
    expected_disk: Vec<u8>,
}

fn assert_bytes(actual: &[u8], expected: &[u8], label: &str) {
    assert_eq!(actual.len(), expected.len(), "{label} length");
    assert_eq!(
        actual.iter().zip(expected).position(|(a, b)| a != b),
        None,
        "{label}: first differing byte"
    );
}

impl Fixture {
    pub(super) fn new(transfer: Transfer, opcode: u8, direct: bool) -> Self {
        let Transfer {
            bytes,
            first_offset,
            list_offset,
        } = transfer;
        assert_eq!(bytes % LBA_SIZE, 0);
        let pages = (bytes + first_offset).div_ceil(PAGE_SIZE);
        let mem_len = (DATA_START - MEM_BASE) as usize + (pages + 1) * PAGE_SIZE;
        let payload: Vec<u8> = (0..bytes)
            .map(|i| ((i % 251) ^ (i / PAGE_SIZE) ^ (i / PAGE_SIZE / 256)) as u8)
            .collect();
        let mut disk = vec![0xa6; bytes + 2 * LBA_SIZE];
        if opcode == NVM_OP_READ {
            disk[LBA_SIZE..LBA_SIZE + bytes].copy_from_slice(&payload);
        }
        let mut expected_disk = disk.clone();
        if opcode == NVM_OP_WRITE {
            expected_disk[LBA_SIZE..LBA_SIZE + bytes].copy_from_slice(&payload);
        }
        let mut ctrl = NvmeController::with_disk_image(disk);
        let mut mem = FakeMem::new(MEM_BASE, mem_len);
        let sizes = u32::from(QDEPTH - 1);
        ctrl.mmio_write(REG_AQA, 4, u64::from((sizes << 16) | sizes));
        ctrl.mmio_write(REG_ASQ, 8, ASQ_BASE);
        ctrl.mmio_write(REG_ACQ, 8, ACQ_BASE);
        // Program valid entry sizes and 4 KiB pages before enabling.
        ctrl.mmio_write(REG_CC, 4, u64::from(CC_EN_BIT | (6 << 16) | (4 << 20)));
        assert_eq!((ctrl.mmio_read(REG_CAP, 8) >> 48) & 0xff, 0);
        let identify = encode_sqe(ADMIN_OP_IDENTIFY, 0x71, 0, DATA_BASE, 1, 0, 0);
        submit_admin(&mut ctrl, &mut mem, 0, &identify);
        assert_eq!(
            completion_status(&read_completion(&mem, ACQ_BASE, 0)),
            SC_SUCCESS
        );
        assert_eq!(mem.read_bytes(DATA_BASE + 77, 1).unwrap(), [0], "MDTS");
        for (slot, opcode, base, flags) in [
            (1, ADMIN_OP_CREATE_IO_CQ, IO_CQ_BASE, 1),
            (2, ADMIN_OP_CREATE_IO_SQ, IO_SQ_BASE, (1 << 16) | 1),
        ] {
            let command = encode_sqe(opcode, slot, 0, base, (sizes << 16) | 1, flags, 0);
            submit_admin(&mut ctrl, &mut mem, slot, &command);
            assert_eq!(
                completion_status(&read_completion(&mem, ACQ_BASE, slot)),
                SC_SUCCESS
            );
        }
        ctrl.set_direct_dma_enabled(direct);
        if direct {
            mem.enable_host_ptr();
        }
        mem.bytes[(DATA_START - MEM_BASE) as usize - PAGE_SIZE..].fill(GUARD);
        let data_pages: Vec<u64> = (0..pages)
            .map(|i| {
                let physical = if i ^ 1 < pages { i ^ 1 } else { i };
                DATA_START + physical as u64 * PAGE_SIZE_U64
            })
            .collect();
        let mut expected_ram = mem.bytes.clone();
        let mut copied = 0;
        for (i, &gpa) in data_pages.iter().enumerate() {
            let offset = if i == 0 { first_offset } else { 0 };
            let count = (bytes - copied).min(PAGE_SIZE - offset);
            let start = (gpa - MEM_BASE) as usize + offset;
            expected_ram[start..start + count].copy_from_slice(&payload[copied..copied + count]);
            if opcode == NVM_OP_WRITE {
                mem.bytes[start..start + count].copy_from_slice(&payload[copied..copied + count]);
            }
            copied += count;
        }
        assert_eq!(copied, bytes);
        let mut lists = Vec::new();
        let mut entries = data_pages[1..].iter().copied().peekable();
        let mut list = LIST_BASE + list_offset as u64;
        while entries.peek().is_some() {
            lists.push(list);
            let capacity = (PAGE_SIZE - (list % PAGE_SIZE_U64) as usize) / 8;
            let count = entries.len();
            let data_count = if count > capacity {
                capacity - 1
            } else {
                count
            };
            for index in 0..data_count {
                let entry = entries.next().unwrap();
                assert!(mem.write_bytes(list + index as u64 * 8, &entry.to_le_bytes()));
            }
            if entries.peek().is_some() {
                let next = LIST_BASE + lists.len() as u64 * PAGE_SIZE_U64;
                assert!(mem.write_bytes(list + (capacity - 1) as u64 * 8, &next.to_le_bytes()));
                list = next;
            }
        }
        let command = encode_sqe_with_prps(
            opcode,
            0x72,
            NSID,
            data_pages[0] + first_offset as u64,
            LIST_BASE + list_offset as u64,
            [1, 0, (bytes / LBA_SIZE - 1) as u32],
        );
        Self {
            ctrl,
            mem,
            command,
            lists,
            expected_ram,
            expected_disk,
        }
    }

    pub(super) fn submit(&mut self, expected_status: u16) {
        assert_eq!(
            submit_io(&mut self.ctrl, &mut self.mem, 0, &self.command),
            expected_status
        );
        let cqe = read_completion(&self.mem, IO_CQ_BASE, 0);
        assert_eq!(
            u16::from_le_bytes(cqe[8..10].try_into().unwrap()),
            1,
            "SQ head"
        );
        assert_eq!(
            u16::from_le_bytes(cqe[10..12].try_into().unwrap()),
            1,
            "SQ ID"
        );
        assert_eq!(
            u16::from_le_bytes(cqe[12..14].try_into().unwrap()),
            0x72,
            "CID"
        );
        assert_eq!(cqe[14] & 1, 1, "phase");
    }

    pub(super) fn assert_payload_and_guards(&self) {
        let start = (DATA_START - MEM_BASE) as usize - PAGE_SIZE;
        assert_bytes(
            &self.mem.bytes[start..],
            &self.expected_ram[start..],
            "RAM payload/guards",
        );
        assert_bytes(
            self.ctrl.disk_image(),
            &self.expected_disk,
            "disk payload/guards",
        );
    }

    pub(super) fn assert_unchanged(&self, ram: &[u8], disk: &[u8]) {
        let start = (DATA_START - MEM_BASE) as usize - PAGE_SIZE;
        assert_bytes(&self.mem.bytes[start..], &ram[start..], "rejected RAM");
        assert_bytes(self.ctrl.disk_image(), disk, "rejected disk");
    }
}
