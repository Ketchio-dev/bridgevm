//! Descriptor chain walking, the avail-ring consumption loop, and used-ring publish.

use super::*;
use crate::fwcfg::GuestMemoryMut;

#[derive(Debug, Clone, Copy)]
pub(crate) struct Descriptor {
    pub(crate) addr: u64,
    pub(crate) len: u32,
    pub(crate) flags: u16,
    pub(crate) next: u16,
}

pub(crate) fn read_u16(mem: &dyn GuestMemoryMut, gpa: u64) -> Option<u16> {
    let mut bytes = [0u8; 2];
    if !mem.read_into(gpa, &mut bytes) {
        return None;
    }
    Some(u16::from_le_bytes(bytes))
}

impl VirtioMmioBlock {
    pub(crate) fn process_queue(&mut self, mem: &mut dyn GuestMemoryMut) {
        if !self.queue_ready || self.queue_num == 0 || self.queue_desc == 0 {
            return;
        }
        let Some(avail_idx) = read_u16(mem, self.queue_driver + 2) else {
            return;
        };
        let mut descs = std::mem::take(&mut self.descriptor_scratch);
        let mut read_buf = std::mem::take(&mut self.read_scratch);
        while self.last_avail_idx != avail_idx {
            let ring_off = 4 + u64::from(self.last_avail_idx % self.queue_num) * 2;
            let Some(head) = read_u16(mem, self.queue_driver + ring_off) else {
                break;
            };
            let completion = self.process_descriptor_chain(mem, head, &mut descs, &mut read_buf);
            self.write_used(mem, head, completion.written_len);
            self.last_avail_idx = self.last_avail_idx.wrapping_add(1);
            self.interrupt_status |= 1;
        }
        descs.clear();
        read_buf.clear();
        self.descriptor_scratch = descs;
        self.read_scratch = read_buf;
    }

    pub(crate) fn descriptor_chain_into(
        mem: &dyn GuestMemoryMut,
        queue_num: u16,
        queue_desc: u64,
        head: u16,
        out: &mut Vec<Descriptor>,
    ) -> bool {
        out.clear();
        if head >= queue_num {
            return false;
        }
        let mut index = head;
        for _ in 0..queue_num {
            let Some(desc) = Descriptor::read(mem, queue_desc + u64::from(index) * DESC_SIZE)
            else {
                out.clear();
                return false;
            };
            let has_next = desc.flags & DESC_F_NEXT != 0;
            out.push(desc);
            if !has_next {
                return true;
            }
            index = desc.next;
            if index >= queue_num {
                out.clear();
                return false;
            }
        }
        out.clear();
        false
    }

    pub(crate) fn write_used(&self, mem: &mut dyn GuestMemoryMut, id: u16, len: u32) {
        let Some(used_idx) = read_u16(mem, self.queue_device + 2) else {
            return;
        };
        let elem = self.queue_device + 4 + u64::from(used_idx % self.queue_num) * 8;
        let _ = mem.write_bytes(elem, &u32::from(id).to_le_bytes());
        let _ = mem.write_bytes(elem + 4, &len.to_le_bytes());
        let _ = mem.write_bytes(
            self.queue_device + 2,
            &used_idx.wrapping_add(1).to_le_bytes(),
        );
    }
}

impl Descriptor {
    pub(crate) fn read(mem: &dyn GuestMemoryMut, gpa: u64) -> Option<Self> {
        let mut bytes = [0u8; 16];
        if !mem.read_into(gpa, &mut bytes) {
            return None;
        }
        Some(Self {
            addr: u64::from_le_bytes(bytes[0..8].try_into().unwrap()),
            len: u32::from_le_bytes(bytes[8..12].try_into().unwrap()),
            flags: u16::from_le_bytes(bytes[12..14].try_into().unwrap()),
            next: u16::from_le_bytes(bytes[14..16].try_into().unwrap()),
        })
    }
}
