//! Split-virtqueue mechanics: chain walking, scatter read/write, used ring, RX delivery.

use super::*;
use crate::fwcfg::GuestMemoryMut;

pub(crate) struct RxQueueDeliveryState<'a> {
    pub(crate) queues: &'a mut [VirtioConsoleQueue; QUEUE_COUNT],
    pub(crate) pending_msix_queue_bits: &'a mut u8,
    pub(crate) interrupt_status: &'a mut u32,
    pub(crate) descriptor_scratch: &'a mut Vec<Descriptor>,
}

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

impl VirtioConsole {
    pub(crate) fn deliver_to_rx_queue(
        &mut self,
        queue_index: usize,
        bytes: &[u8],
        mem: &mut dyn GuestMemoryMut,
    ) -> bool {
        self.deliver_partial_to_rx_queue(queue_index, bytes, mem)
            .is_some_and(|written| written == bytes.len())
    }

    pub(crate) fn deliver_partial_to_rx_queue(
        &mut self,
        queue_index: usize,
        bytes: &[u8],
        mem: &mut dyn GuestMemoryMut,
    ) -> Option<usize> {
        Self::deliver_partial_slices_to_rx_queue(
            RxQueueDeliveryState {
                queues: &mut self.queues,
                pending_msix_queue_bits: &mut self.pending_msix_queue_bits,
                interrupt_status: &mut self.interrupt_status,
                descriptor_scratch: &mut self.descriptor_scratch,
            },
            queue_index,
            bytes,
            &[],
            mem,
        )
    }

    pub(crate) fn deliver_partial_slices_to_rx_queue(
        state: RxQueueDeliveryState<'_>,
        queue_index: usize,
        first: &[u8],
        second: &[u8],
        mem: &mut dyn GuestMemoryMut,
    ) -> Option<usize> {
        let bytes_len = first.len().checked_add(second.len())?;
        let queue = state.queues[queue_index];
        if !queue.ready || queue.size == 0 || queue.desc == 0 || bytes_len == 0 {
            return None;
        }
        let avail_idx = read_u16(mem, queue.driver + 2)?;
        state.queues[queue_index].last_avail_seen = avail_idx;
        let last_avail_idx = state.queues[queue_index].last_avail_idx;
        if last_avail_idx == avail_idx {
            // The guest has not published a fresh avail buffer since we last
            // consumed. If this keeps firing while notify_count / last_avail_seen
            // stay flat, the guest stopped replenishing (not our consume path).
            state.queues[queue_index].rx_no_buffers =
                state.queues[queue_index].rx_no_buffers.saturating_add(1);
            console_trace!(
                "rx q{queue_index} NO-BUFFERS last_consumed={last_avail_idx} avail_idx={avail_idx} bytes={}",
                bytes_len
            );
            return None;
        }
        let ring_off = 4 + u64::from(last_avail_idx % queue.size) * 2;
        let head = read_u16(mem, queue.driver + ring_off)?;
        let mut descs = std::mem::take(state.descriptor_scratch);
        if !Self::descriptor_chain_into(mem, &queue, head, &mut descs) {
            descs.clear();
            *state.descriptor_scratch = descs;
            // avail advanced but we could not walk the chain (head >= size, or a
            // bad next link). This is the "replenished buffers are invisible to
            // us" signature -> our consume path or a size mismatch.
            console_trace!(
                "rx q{queue_index} CHAIN-FAIL head={head} size={} last_consumed={last_avail_idx} avail_idx={avail_idx}",
                queue.size
            );
            return None;
        }
        let Some(written) = Self::scatter_write_partial_slices(mem, &descs, first, second) else {
            console_trace!(
                "rx q{queue_index} SCATTER-FAIL head={head} descs={}",
                descs.len()
            );
            descs.clear();
            *state.descriptor_scratch = descs;
            return None;
        };
        descs.clear();
        *state.descriptor_scratch = descs;
        Self::write_used(
            mem,
            &queue,
            head,
            u32::try_from(written).unwrap_or(u32::MAX),
        );
        state.queues[queue_index].last_avail_idx = last_avail_idx.wrapping_add(1);
        state.queues[queue_index].used_produced =
            state.queues[queue_index].used_produced.saturating_add(1);
        state.queues[queue_index].pending_msix = true;
        if let Some(bit) = queue_bit(queue_index) {
            *state.pending_msix_queue_bits |= bit;
        }
        *state.interrupt_status |= 1;
        console_trace!(
            "rx q{queue_index} DELIVER head={head} len={written} last_consumed->{} avail_idx={avail_idx}",
            last_avail_idx.wrapping_add(1)
        );
        Some(written)
    }

    pub(crate) fn read_chain_into(
        mem: &dyn GuestMemoryMut,
        queue: &VirtioConsoleQueue,
        head: u16,
        descs: &mut Vec<Descriptor>,
        out: &mut Vec<u8>,
        max_len: usize,
    ) -> bool {
        out.clear();
        if !Self::descriptor_chain_into(mem, queue, head, descs) {
            return false;
        }
        for desc in descs.iter() {
            if desc.flags & DESC_F_WRITE != 0 {
                return false;
            }
            let start = out.len();
            let Some(end) = start.checked_add(desc.len as usize) else {
                return false;
            };
            if end > max_len {
                return false;
            }
            // `read_bytes` validates the guest range before allocating in the
            // live RAM implementation. Only append after that validation so an
            // unbacked, oversized descriptor cannot resize reusable scratch.
            let Some(bytes) = mem.read_bytes(desc.addr, desc.len as usize) else {
                return false;
            };
            out.extend_from_slice(&bytes);
        }
        true
    }

    pub(crate) fn scatter_write_partial_slices(
        mem: &mut dyn GuestMemoryMut,
        descs: &[Descriptor],
        first: &[u8],
        second: &[u8],
    ) -> Option<usize> {
        let bytes_len = first.len().checked_add(second.len())?;
        let mut offset = 0usize;
        for desc in descs {
            if desc.flags & DESC_F_WRITE == 0 {
                return None;
            }
            let mut desc_addr = desc.addr;
            let mut desc_remaining = desc.len as usize;
            while desc_remaining > 0 && offset < bytes_len {
                let chunk = Self::slice_pair_chunk(first, second, offset)?;
                let writable = desc_remaining.min(chunk.len());
                if writable == 0 {
                    break;
                }
                if !mem.write_bytes(desc_addr, &chunk[..writable]) {
                    return None;
                }
                offset += writable;
                desc_addr = desc_addr.checked_add(writable as u64)?;
                desc_remaining -= writable;
            }
            if offset == bytes_len {
                break;
            }
        }
        (offset > 0).then_some(offset)
    }

    pub(crate) fn slice_pair_chunk<'a>(
        first: &'a [u8],
        second: &'a [u8],
        offset: usize,
    ) -> Option<&'a [u8]> {
        if offset < first.len() {
            return Some(&first[offset..]);
        }
        let second_offset = offset.checked_sub(first.len())?;
        (second_offset < second.len()).then_some(&second[second_offset..])
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
        queue: &VirtioConsoleQueue,
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
        queue: &VirtioConsoleQueue,
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
