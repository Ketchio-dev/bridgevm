//! Split-virtqueue mechanics: chain walking, scatter write, used ring, interrupt marking.

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

impl<B: NetBackend> VirtioNet<B> {
    pub(crate) fn scatter_write_slices(
        mem: &mut dyn GuestMemoryMut,
        descs: &[Descriptor],
        slices: &[&[u8]],
    ) -> bool {
        let total_len = slices
            .iter()
            .try_fold(0usize, |sum, slice| sum.checked_add(slice.len()));
        let Some(total_len) = total_len else {
            return false;
        };
        if total_len == 0 {
            return true;
        }

        let mut slice_index = 0usize;
        let mut slice_offset = 0usize;
        let mut written = 0usize;

        for desc in descs {
            if desc.flags & DESC_F_WRITE == 0 {
                return false;
            }
            let mut desc_offset = 0usize;
            let desc_len = desc.len as usize;
            while desc_offset < desc_len && written < total_len {
                while slice_index < slices.len() && slice_offset == slices[slice_index].len() {
                    slice_index += 1;
                    slice_offset = 0;
                }
                if slice_index == slices.len() {
                    return written == total_len;
                }

                let slice = slices[slice_index];
                let copy_len = (desc_len - desc_offset).min(slice.len() - slice_offset);
                let Some(gpa) = desc.addr.checked_add(desc_offset as u64) else {
                    return false;
                };
                if !mem.write_bytes(gpa, &slice[slice_offset..slice_offset + copy_len]) {
                    return false;
                }
                desc_offset += copy_len;
                slice_offset += copy_len;
                written += copy_len;
            }
        }
        written == total_len
    }

    pub(crate) fn mark_queue_interrupt(&mut self, queue_index: usize) {
        if let Some(queue) = self.queues.get_mut(queue_index) {
            queue.pending_msix = true;
            if let Some(bit) = queue_bit(queue_index) {
                self.pending_msix_queue_bits |= bit;
            }
        }
        self.interrupt_status |= 1;
    }

    pub(crate) fn descriptor_chain_into(
        mem: &dyn GuestMemoryMut,
        queue: &VirtioNetQueue,
        head: u16,
        out: &mut Vec<Descriptor>,
    ) -> bool {
        out.clear();
        if head >= queue.size {
            return false;
        }
        let mut index = head;
        for _ in 0..queue.size {
            let Some(desc) = Descriptor::read(mem, queue.desc + u64::from(index) * DESC_SIZE)
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
            if index >= queue.size {
                out.clear();
                return false;
            }
        }
        out.clear();
        false
    }

    pub(crate) fn write_used(
        mem: &mut dyn GuestMemoryMut,
        queue: &VirtioNetQueue,
        id: u16,
        len: u32,
    ) {
        if queue.size == 0 || queue.device == 0 {
            return;
        }
        let Some(used_idx) = read_u16(mem, queue.device + 2) else {
            return;
        };
        let elem = queue.device + 4 + u64::from(used_idx % queue.size) * 8;
        let _ = mem.write_bytes(elem, &u32::from(id).to_le_bytes());
        let _ = mem.write_bytes(elem + 4, &len.to_le_bytes());
        let _ = mem.write_bytes(queue.device + 2, &used_idx.wrapping_add(1).to_le_bytes());
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
