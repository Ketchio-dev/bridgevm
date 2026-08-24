//! Fail-closed virtqueue descriptor walking and bounded rejection evidence.

use super::*;
use crate::fwcfg::GuestMemoryMut;
use std::fmt::Write as _;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DescriptorChainError {
    HeadOutOfRange,
    DescriptorAddressOverflow { index: u16 },
    DescriptorUnreadable { index: u16 },
    NextOutOfRange { index: u16, next: u16 },
    CycleOrTooLong { index: u16 },
}

impl VirtioGpu {
    pub(crate) fn descriptor_chain_into(
        mem: &dyn GuestMemoryMut,
        queue: &VirtioGpuQueue,
        head: u16,
        out: &mut Vec<Descriptor>,
    ) -> Result<(), DescriptorChainError> {
        out.clear();
        let queue_size = queue.effective_size();
        if head >= queue_size {
            return Err(DescriptorChainError::HeadOutOfRange);
        }
        let mut index = head;
        for _ in 0..queue_size {
            let Some(gpa) = queue.desc.checked_add(u64::from(index) * DESC_SIZE) else {
                return Err(DescriptorChainError::DescriptorAddressOverflow { index });
            };
            let Some(desc) = Descriptor::read(mem, gpa) else {
                return Err(DescriptorChainError::DescriptorUnreadable { index });
            };
            let has_next = desc.flags & DESC_F_NEXT != 0;
            out.push(desc);
            if !has_next {
                return Ok(());
            }
            let next = desc.next;
            if next >= queue_size {
                return Err(DescriptorChainError::NextOutOfRange { index, next });
            }
            index = next;
        }
        Err(DescriptorChainError::CycleOrTooLong { index })
    }

    pub(crate) fn record_descriptor_chain_rejected(
        &mut self,
        queue_index: usize,
        head: u16,
        partial: &[Descriptor],
        error: DescriptorChainError,
    ) {
        self.trace_descriptor_chain_reject_count =
            self.trace_descriptor_chain_reject_count.saturating_add(1);
        if !trace_sample(self.trace_descriptor_chain_reject_count) {
            return;
        }
        let queue_size = self.queues[queue_index].effective_size();
        let last_avail_idx = self.queues[queue_index].last_avail_idx;
        let partial_count = partial.len();
        let (reason, index, next) = match error {
            DescriptorChainError::HeadOutOfRange => ("head_out_of_range", None, None),
            DescriptorChainError::DescriptorAddressOverflow { index } => {
                ("descriptor_address_overflow", Some(index), None)
            }
            DescriptorChainError::DescriptorUnreadable { index } => {
                ("descriptor_unreadable", Some(index), None)
            }
            DescriptorChainError::NextOutOfRange { index, next } => {
                ("next_out_of_range", Some(index), Some(next))
            }
            DescriptorChainError::CycleOrTooLong { index } => {
                ("cycle_or_too_long", Some(index), None)
            }
        };
        self.record_trace_fields("descriptor_chain_rejected", |fields| {
            let _ = write!(fields, ",\"queue\":{queue_index},\"head\":{head},\"queue_size\":{queue_size},\"last_avail_idx\":{last_avail_idx},\"partial_descriptor_count\":{partial_count},\"reason\":\"{reason}\"");
            if let Some(index) = index { let _ = write!(fields, ",\"index\":{index}"); }
            if let Some(next) = next { let _ = write!(fields, ",\"next\":{next}"); }
        });
    }
}

impl Descriptor {
    pub(crate) fn read(mem: &dyn GuestMemoryMut, gpa: u64) -> Option<Self> {
        let mut bytes = [0u8; DESC_SIZE as usize];
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
