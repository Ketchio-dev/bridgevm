//! Split-virtqueue mechanics: chain walking, scatter read/write, used ring, RX delivery.

use super::*;
use crate::fwcfg::GuestMemoryMut;

pub(crate) struct RxQueueDeliveryState<'a> {
    pub(crate) queues: &'a mut [VirtioConsoleQueue; QUEUE_COUNT],
    pub(crate) pending_msix_queue_bits: &'a mut u8,
    pub(crate) interrupt_status: &'a mut u32,
    pub(crate) descriptor_scratch: &'a mut Vec<Descriptor>,
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
                queues: &mut self.pci.queues,
                pending_msix_queue_bits: &mut self.pci.pending_msix_queue_bits,
                interrupt_status: &mut self.pci.interrupt_status,
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
        let avail_idx = read_avail_idx(mem, queue.driver)?;
        state.queues[queue_index].counters.last_avail_seen = avail_idx;
        let last_avail_idx = state.queues[queue_index].last_avail_idx;
        if last_avail_idx == avail_idx {
            // The guest has not published a fresh avail buffer since we last
            // consumed. If this keeps firing while notify_count / last_avail_seen
            // stay flat, the guest stopped replenishing (not our consume path).
            state.queues[queue_index].counters.rx_no_buffers = state.queues[queue_index]
                .counters
                .rx_no_buffers
                .saturating_add(1);
            console_trace!(
                "rx q{queue_index} NO-BUFFERS last_consumed={last_avail_idx} avail_idx={avail_idx} bytes={}",
                bytes_len
            );
            return None;
        }
        let ring_off = 4 + u64::from(last_avail_idx % queue.size) * 2;
        let head = read_u16(mem, queue.driver, ring_off)?;
        let mut descs = std::mem::take(state.descriptor_scratch);
        if !descriptor_chain_into(mem, queue.desc, queue.size, head, &mut descs) {
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
        write_used(
            mem,
            queue.device,
            queue.size,
            head,
            u32::try_from(written).unwrap_or(u32::MAX),
        );
        state.queues[queue_index].last_avail_idx = last_avail_idx.wrapping_add(1);
        state.queues[queue_index].counters.used_produced = state.queues[queue_index]
            .counters
            .used_produced
            .saturating_add(1);
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

    pub(crate) fn scatter_write_partial_slices(
        mem: &mut dyn GuestMemoryMut,
        descs: &[Descriptor],
        first: &[u8],
        second: &[u8],
    ) -> Option<usize> {
        let bytes_len = first.len().checked_add(second.len())?;
        if descs.iter().any(|desc| desc.flags & DESC_F_WRITE == 0) {
            return None;
        }
        let mut offset = 0usize;
        for desc in descs {
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
}
