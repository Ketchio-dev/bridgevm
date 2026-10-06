//! Split-ring descriptors frame logical buffers, not individual request fields.

use super::super::{Descriptor, DESC_F_WRITE};
use super::VIRTIO_BLK_T_OUT;
use crate::fwcfg::GuestMemoryMut;

pub(super) struct RequestLayout {
    pub(super) readable_len: u64,
    pub(super) data_len: u64,
    pub(super) status_addr: u64,
    first_writable: usize,
}

impl RequestLayout {
    pub(super) fn parse(descs: &[Descriptor]) -> Option<Self> {
        let mut readable_len = 0u64;
        let mut writable_len = 0u64;
        let mut first_writable = descs.len();
        let mut status_addr = None;
        for (index, desc) in descs.iter().enumerate() {
            let last = u64::from(desc.len).checked_sub(1);
            let end = match last {
                Some(last) => Some(desc.addr.checked_add(last)?),
                None => None,
            };
            if desc.flags & DESC_F_WRITE != 0 {
                first_writable = first_writable.min(index);
                writable_len = writable_len.checked_add(u64::from(desc.len))?;
                if end.is_some() {
                    status_addr = end;
                }
            } else {
                if first_writable != descs.len() {
                    return None;
                }
                readable_len = readable_len.checked_add(u64::from(desc.len))?;
            }
        }
        Some(Self {
            readable_len,
            data_len: writable_len.checked_sub(1)?,
            status_addr: status_addr?,
            first_writable,
        })
    }

    pub(super) fn payload_len(&self, req_type: u32) -> u64 {
        if req_type == VIRTIO_BLK_T_OUT {
            self.readable_len.saturating_sub(16)
        } else {
            self.data_len
        }
    }

    pub(super) fn header(
        &self,
        mem: &dyn GuestMemoryMut,
        descs: &[Descriptor],
    ) -> Option<[u8; 16]> {
        if self.readable_len < 16 {
            return None;
        }
        let mut header = [0; 16];
        let mut copied = 0;
        for desc in &descs[..self.first_writable] {
            let len = (desc.len as usize).min(header.len() - copied);
            if len > 0 && !mem.read_into(desc.addr, &mut header[copied..copied + len]) {
                return None;
            }
            copied += len;
            if copied == header.len() {
                return Some(header);
            }
        }
        None
    }

    pub(super) fn data<'a>(
        &self,
        descs: &'a [Descriptor],
    ) -> impl Iterator<Item = (u64, usize)> + 'a {
        let mut remaining = self.data_len;
        descs[self.first_writable..].iter().map(move |desc| {
            let len = u64::from(desc.len).min(remaining);
            remaining -= len;
            (desc.addr, len as usize)
        })
    }
}
