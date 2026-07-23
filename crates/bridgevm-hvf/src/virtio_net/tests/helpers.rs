//! Split test module.

use super::super::*;
use crate::{fwcfg::GuestMemoryMut, pcie::VIRTIO_NET_MSIX_TABLE_OFFSET};

#[derive(Debug)]
pub(super) struct TestMem {
    pub(super) base: u64,
    pub(super) bytes: Vec<u8>,
}

impl TestMem {
    pub(super) fn new(base: u64, len: usize) -> Self {
        Self {
            base,
            bytes: vec![0; len],
        }
    }

    pub(super) fn write(&mut self, gpa: u64, data: &[u8]) {
        assert!(self.write_bytes(gpa, data));
    }

    pub(super) fn read(&self, gpa: u64, len: usize) -> Vec<u8> {
        self.read_bytes(gpa, len).unwrap()
    }
}

impl GuestMemoryMut for TestMem {
    fn write_bytes(&mut self, gpa: u64, data: &[u8]) -> bool {
        let Some(off) = gpa.checked_sub(self.base).map(|v| v as usize) else {
            return false;
        };
        let Some(end) = off.checked_add(data.len()) else {
            return false;
        };
        if end > self.bytes.len() {
            return false;
        }
        self.bytes[off..end].copy_from_slice(data);
        true
    }

    fn read_bytes(&self, gpa: u64, len: usize) -> Option<Vec<u8>> {
        let off = gpa.checked_sub(self.base)? as usize;
        let end = off.checked_add(len)?;
        (end <= self.bytes.len()).then(|| self.bytes[off..end].to_vec())
    }

    fn read_into(&self, gpa: u64, dst: &mut [u8]) -> bool {
        let Some(off) = gpa.checked_sub(self.base).map(|v| v as usize) else {
            return false;
        };
        let Some(end) = off.checked_add(dst.len()) else {
            return false;
        };
        if end > self.bytes.len() {
            return false;
        }
        dst.copy_from_slice(&self.bytes[off..end]);
        true
    }
}

pub(super) fn pci_write(
    dev: &mut VirtioPciNet,
    offset: u64,
    size: u8,
    value: u64,
    mem: &mut TestMem,
) {
    assert_eq!(
        dev.access(offset, VirtioPciNetOp::Write { size, value }, mem),
        VirtioNetResult::WriteAck
    );
}

pub(super) fn pci_read(dev: &mut VirtioPciNet, offset: u64, size: u8, mem: &mut TestMem) -> u64 {
    match dev.access(offset, VirtioPciNetOp::Read { size }, mem) {
        VirtioNetResult::ReadValue(value) => value,
        VirtioNetResult::WriteAck => panic!("read returned write ack"),
    }
}

pub(super) fn pci_write_split_u64(
    dev: &mut VirtioPciNet,
    offset: u64,
    value: u64,
    mem: &mut TestMem,
) {
    pci_write(dev, offset, 4, value & 0xffff_ffff, mem);
    pci_write(dev, offset + 4, 4, value >> 32, mem);
}

pub(super) fn write_desc(
    mem: &mut TestMem,
    table: u64,
    index: u16,
    addr: u64,
    len: u32,
    flags: u16,
    next: u16,
) {
    let gpa = table + u64::from(index) * DESC_SIZE;
    mem.write(gpa, &addr.to_le_bytes());
    mem.write(gpa + 8, &len.to_le_bytes());
    mem.write(gpa + 12, &flags.to_le_bytes());
    mem.write(gpa + 14, &next.to_le_bytes());
}

pub(super) fn setup_queue(
    dev: &mut VirtioPciNet,
    mem: &mut TestMem,
    queue: u16,
    desc: u64,
    avail: u64,
    used: u64,
    vector: u16,
) {
    pci_write(dev, COMMON_QUEUE_SELECT, 2, u64::from(queue), mem);
    pci_write(dev, COMMON_QUEUE_SIZE, 2, 8, mem);
    pci_write_split_u64(dev, COMMON_QUEUE_DESC, desc, mem);
    pci_write_split_u64(dev, COMMON_QUEUE_DRIVER, avail, mem);
    pci_write_split_u64(dev, COMMON_QUEUE_DEVICE, used, mem);
    pci_write(dev, COMMON_QUEUE_MSIX_VECTOR, 2, u64::from(vector), mem);
    pci_write(dev, COMMON_QUEUE_ENABLE, 2, 1, mem);
}

pub(super) fn program_msix_vector(dev: &mut VirtioPciNet, vector: u16, address: u64, data: u32) {
    let off = u64::from(VIRTIO_NET_MSIX_TABLE_OFFSET) + u64::from(vector) * 16;
    assert_eq!(
        dev.msix_bar_access(
            off,
            VirtioPciNetOp::Write {
                size: 8,
                value: address,
            },
        ),
        VirtioNetResult::WriteAck
    );
    assert_eq!(
        dev.msix_bar_access(
            off + 8,
            VirtioPciNetOp::Write {
                size: 4,
                value: u64::from(data),
            },
        ),
        VirtioNetResult::WriteAck
    );
    assert_eq!(
        dev.msix_bar_access(off + 12, VirtioPciNetOp::Write { size: 4, value: 0 },),
        VirtioNetResult::WriteAck
    );
}

#[test]
fn rx_pump_scatters_header_and_frame_across_split_descriptors() {
    let mut dev = VirtioPciNet::new_loopback();
    let mut mem = TestMem::new(0x4000_0000, 0x20000);
    let desc = 0x4000_1000;
    let avail = 0x4000_2000;
    let used = 0x4000_3000;
    let header_prefix = 0x4000_4000;
    let header_suffix = 0x4000_4100;
    let payload_buf = 0x4000_4200;
    let frame = b"\x52\x54\x00\x42\x56\x01\x02\x00\x00\x00\x00\x01\x08\x00split-rx";

    setup_queue(&mut dev, &mut mem, 0, desc, avail, used, 0);
    write_desc(
        &mut mem,
        desc,
        0,
        header_prefix,
        10,
        DESC_F_WRITE | DESC_F_NEXT,
        1,
    );
    write_desc(
        &mut mem,
        desc,
        1,
        header_suffix,
        4,
        DESC_F_WRITE | DESC_F_NEXT,
        2,
    );
    write_desc(&mut mem, desc, 2, payload_buf, 128, DESC_F_WRITE, 0);
    mem.write(avail + 2, &1u16.to_le_bytes());
    mem.write(avail + 4, &0u16.to_le_bytes());
    dev.backend_mut().push_receive(frame.to_vec());

    assert!(dev.pump_receive(&mut mem));

    assert_eq!(mem.read(header_prefix, 10), [0; 10]);
    assert_eq!(&mem.read(header_suffix, 4)[..2], &1u16.to_le_bytes());
    assert_eq!(&mem.read(header_suffix + 2, 2), &frame[..2]);
    assert_eq!(&mem.read(payload_buf, frame.len() - 2), &frame[2..]);
    assert_eq!(
        u32::from_le_bytes(mem.read(used + 8, 4).try_into().unwrap()),
        (VIRTIO_NET_HDR_LEN + frame.len()) as u32
    );
}
